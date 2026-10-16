// GPU harness for the renderer (doc/design_screen_space_lines.md, Testing
// layer 4).
//
// Builds a real `Renderer` offscreen, uploads small hand-made meshes, renders
// and asserts on the read-back pixels. Every test is `#[ignore]`, so a plain
// `cargo test` stays GPU-free; run them with
//
//     cargo test -p atomcad-renderer --test renderer_gpu -- --ignored
//
// When no adapter is found a test prints that and returns rather than failing.
// Assertions use tolerances and pixel counts, never committed golden images,
// which would break across drivers.
//
// Setting `ATOMCAD_GPU_DUMP_DIR` writes every image a test renders into that
// directory as raw bytes, so two builds can be compared byte for byte (the
// "pixel-identical" check of a phase that must not change the output).
//
// Setting `ATOMCAD_GPU_COMPARE_DIR` makes that comparison automatic
// (`doc/design_depth_perception.md`, Testing layer 4): every image is also
// read back from `<dir>/<name>.raw`, a dump of an earlier build on the same
// machine, and must match it within `GPU_COMPARE_TOLERANCE` per channel. The
// reference is a local dump, never a committed golden image.

use atomcad_renderer::atom_impostor_mesh::AtomImpostorMesh;
use atomcad_renderer::bond_impostor_mesh::BondImpostorMesh;
use atomcad_renderer::image_export::IMAGE_EXPORT_SUPERSAMPLING;
use atomcad_renderer::label_mesh::LabelMesh;
use atomcad_renderer::line_mesh::{LineMesh, LineStyle};
use atomcad_renderer::mesh::{Material, Mesh, Vertex};
use atomcad_renderer::renderer::Renderer;
use atomcad_renderer::thumbnail::{THUMBNAIL_SIZE, fit_camera_to_bounds};
use atomcad_renderer::transparent_impostor_mesh::TransparentImpostorMesh;
use atomcad_renderer::transparent_surface_mesh::TransparentSurfaceMesh;
use glam::f32::Vec3;

const VIEWPORT_WIDTH: u32 = 320;
const VIEWPORT_HEIGHT: u32 = 200;
const BACKGROUND_RGB: [u8; 3] = [30, 30, 30];

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

/// A renderer at `width × height`, or `None` (with a note on stdout) when the
/// machine has no adapter. `Renderer::new` itself panics without one.
///
/// Automatic refinement is off (`doc/design_viewport_antialiasing.md` D5,
/// Testing layer 4), so a test that renders twice gets the same kind of
/// frame both times however slow the machine is. Refined frames come from
/// `render_still`, or from a test that sets a zero delay.
fn try_renderer(width: u32, height: u32) -> Option<Renderer> {
    let adapter = block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    );
    if adapter.is_none() {
        println!("renderer_gpu: no wgpu adapter found, skipping");
        return None;
    }
    let mut renderer = block_on(Renderer::new(width, height));
    renderer.set_refine_delay(None);
    Some(renderer)
}

/// The largest per-channel difference `ATOMCAD_GPU_COMPARE_DIR` accepts
/// against the reference dump.
const GPU_COMPARE_TOLERANCE: u8 = 0;

/// Images the current phase changes on purpose, by name prefix: compared with
/// `ATOMCAD_GPU_COMPARE_DIR` and reported, but not asserted, because the
/// reference is a dump of the *previous* phase. Each phase that changes
/// pixels lists them here and tests the change by its own assertions.
///
/// Depth perception Phases 3 and 4 (`doc/design_depth_perception.md`) change
/// no image the renderer's neutral default draws: the depth cue and AO are
/// off unless a test sets them, and every vertex a test builds without an
/// exposure is fully exposed, so every image is asserted against a dump of
/// the phase before. (Phase 2 listed the rimmed atoms and the top-down
/// orthographic flat squares here.)
const GPU_COMPARE_EXEMPT: &[&str] = &[];

/// Writes `pixels` to `$ATOMCAD_GPU_DUMP_DIR/<name>.raw` when that variable is
/// set, and compares them with `$ATOMCAD_GPU_COMPARE_DIR/<name>.raw` when that
/// one is: same length, and no channel more than [`GPU_COMPARE_TOLERANCE`]
/// apart, unless the name is in [`GPU_COMPARE_EXEMPT`]. An image the
/// reference directory lacks (a test newer than the dump) is reported and
/// skipped. Does nothing when neither is set.
fn dump(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("ATOMCAD_GPU_DUMP_DIR") {
        let path = std::path::Path::new(&dir).join(format!("{name}.raw"));
        std::fs::write(&path, pixels).expect("write GPU dump");
    }
    if let Ok(dir) = std::env::var("ATOMCAD_GPU_COMPARE_DIR") {
        let path = std::path::Path::new(&dir).join(format!("{name}.raw"));
        let Ok(reference) = std::fs::read(&path) else {
            println!(
                "renderer_gpu: no reference {}, not compared",
                path.display()
            );
            return;
        };
        assert_eq!(
            pixels.len(),
            reference.len(),
            "{name}: size differs from the reference"
        );
        let (worst, at) = pixels
            .iter()
            .zip(&reference)
            .enumerate()
            .map(|(i, (a, b))| (a.abs_diff(*b), i))
            .max()
            .unwrap_or((0, 0));
        let differing = pixels
            .chunks_exact(4)
            .zip(reference.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        if GPU_COMPARE_EXEMPT
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            println!(
                "renderer_gpu: {name} exempt from the comparison: differs by up to {worst}, {differing} pixels differ"
            );
            return;
        }
        // Written as an excess, not `worst <= tolerance`: with a tolerance
        // of 0 clippy rejects that comparison as always true or false.
        assert_eq!(
            worst.saturating_sub(GPU_COMPARE_TOLERANCE),
            0,
            "{name}: differs from the reference by {worst} at byte {at} \
             (pixel {}), {differing} pixels differ",
            at / 4
        );
    }
}

/// A fixed scene of one of each common primitive: an opaque quad in the
/// z = 0 plane, a wireframe square around it, one atom impostor, one bond
/// impostor and a few background grid lines.
fn upload_fixed_scene(renderer: &mut Renderer) {
    let mut main_mesh = Mesh::new();
    let material = Material::new(&Vec3::new(0.8, 0.3, 0.2), 0.5, 0.0);
    let normal = Vec3::Z;
    let corners = [
        Vec3::new(-4.0, -4.0, 0.0),
        Vec3::new(4.0, -4.0, 0.0),
        Vec3::new(4.0, 4.0, 0.0),
        Vec3::new(-4.0, 4.0, 0.0),
    ];
    let first = corners
        .iter()
        .map(|corner| main_mesh.add_vertex(Vertex::new(corner, &normal, &material)))
        .collect::<Vec<_>>()[0];
    main_mesh.add_quad(first, first + 1, first + 2, first + 3);

    let mut wireframe = LineMesh::new();
    let white = [1.0, 1.0, 1.0];
    for i in 0..4 {
        let a = corners[i] * 1.25 + Vec3::Z;
        let b = corners[(i + 1) % 4] * 1.25 + Vec3::Z;
        wireframe.add_line_with_uniform_color(&a, &b, &white);
    }

    let mut atoms = AtomImpostorMesh::new();
    atoms.add_atom_quad(
        &Vec3::new(0.0, 0.0, 3.0),
        1.5,
        &[0.2, 0.4, 0.9],
        0.5,
        0.0,
        &[0.0, 0.0, 0.0, 0.0],
    );
    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_with_positions(
        &Vec3::new(0.0, 0.0, 3.0),
        &Vec3::new(3.0, 0.0, 3.0),
        0.3,
        &[0.7, 0.7, 0.7],
    );

    renderer.update_all_gpu_meshes(
        &Mesh::new(),
        &LineMesh::new(),
        &main_mesh,
        &wireframe,
        &LineMesh::new(),
        &atoms,
        &bonds,
        &TransparentImpostorMesh::new(),
        &TransparentSurfaceMesh::new(),
        &LabelMesh::new(),
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
        true,
        Some((1, "Main".to_string())),
    );

    let mut background = LineMesh::new();
    let grey = [0.5, 0.5, 0.5];
    for i in -5..=5 {
        let t = i as f32 * 2.0;
        background.add_line_with_uniform_color(
            &Vec3::new(t, -10.0, -1.0),
            &Vec3::new(t, 10.0, -1.0),
            &grey,
        );
        background.add_line_with_uniform_color(
            &Vec3::new(-10.0, t, -1.0),
            &Vec3::new(10.0, t, -1.0),
            &grey,
        );
    }
    renderer.update_background_mesh(&background);
}

/// How many pixels differ from `background` by more than a few levels in any
/// colour channel. `stride` is 4 (BGRA or RGBA, alpha ignored).
fn count_non_background(pixels: &[u8], background: [u8; 3], bgra: bool) -> usize {
    let expected = if bgra {
        [background[2], background[1], background[0]]
    } else {
        background
    };
    pixels
        .chunks_exact(4)
        .filter(|p| (0..3).any(|c| (p[c] as i32 - expected[c] as i32).abs() > 4))
        .count()
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn fixed_scene_renders_through_live_thumbnail_and_export_paths() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_fixed_scene(&mut renderer);

    // Live viewport: BGRA at the viewport size.
    let live = renderer.render(BACKGROUND_RGB);
    dump("live", &live);
    assert_eq!(live.len(), (VIEWPORT_WIDTH * VIEWPORT_HEIGHT * 4) as usize);
    let lit = count_non_background(&live, BACKGROUND_RGB, true);
    assert!(lit > 1000, "live render is (almost) all background: {lit}");

    // Thumbnail: RGBA at `THUMBNAIL_SIZE`, on its own background.
    let bounds = renderer.content_bounds().expect("content has bounds");
    let thumbnail_camera = fit_camera_to_bounds(&renderer.camera, bounds);
    let thumbnail = renderer.render_thumbnail(&thumbnail_camera);
    dump("thumbnail", &thumbnail);
    assert_eq!(
        thumbnail.len(),
        (THUMBNAIL_SIZE * THUMBNAIL_SIZE * 4) as usize
    );
    let first = &thumbnail[0..3];
    let distinct = thumbnail
        .chunks_exact(4)
        .filter(|p| (0..3).any(|c| (p[c] as i32 - first[c] as i32).abs() > 4))
        .count();
    assert!(distinct > 100, "thumbnail is (almost) uniform: {distinct}");

    // Export: RGBA at the requested size, which differs from the viewport's.
    let (export_width, export_height) = (400, 240);
    let export = renderer.render_image(export_width, export_height, BACKGROUND_RGB);
    dump("export", &export);
    assert_eq!(export.len(), (export_width * export_height * 4) as usize);
    let lit = count_non_background(&export, BACKGROUND_RGB, false);
    assert!(lit > 1000, "export is (almost) all background: {lit}");
    assert_eq!(IMAGE_EXPORT_SUPERSAMPLING, 2);

    // The live target is untouched by the two offscreen renders.
    assert_eq!(
        renderer.get_viewport_size(),
        (VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
    );
    let live_again = renderer.render(BACKGROUND_RGB);
    assert_eq!(
        live, live_again,
        "offscreen renders disturbed the live view"
    );
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn resized_viewport_renders_at_the_new_size() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_fixed_scene(&mut renderer);

    let (width, height) = (256, 256);
    renderer.set_viewport_request(width as f64, height as f64, 1.0);
    let live = renderer.render(BACKGROUND_RGB);
    dump("resized", &live);
    assert_eq!(live.len(), (width * height * 4) as usize);
    let lit = count_non_background(&live, BACKGROUND_RGB, true);
    assert!(
        lit > 1000,
        "resized render is (almost) all background: {lit}"
    );
}

// ---- Screen-space lines, Phase 2 (doc/design_screen_space_lines.md) --------

use glam::f64::DVec3;

const BLACK: [u8; 3] = [0, 0, 0];

/// Uploads `main` and `wireframe` as the content, `lightweight` and
/// `gadget_lines` as the gadgets, and nothing else.
fn upload(
    renderer: &mut Renderer,
    main: &Mesh,
    wireframe: &LineMesh,
    lightweight: &Mesh,
    gadget_lines: &LineMesh,
) {
    renderer.update_all_gpu_meshes(
        lightweight,
        gadget_lines,
        main,
        wireframe,
        &LineMesh::new(),
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &TransparentSurfaceMesh::new(),
        &LabelMesh::new(),
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
        true,
        Some((1, "Main".to_string())),
    );
}

/// An opaque square of half-size `half` in the plane `z`, facing +Z.
fn square(half: f32, z: f32, albedo: Vec3) -> Mesh {
    let mut mesh = Mesh::new();
    let material = Material::new(&albedo, 0.5, 0.0);
    let corners = [
        Vec3::new(-half, -half, z),
        Vec3::new(half, -half, z),
        Vec3::new(half, half, z),
        Vec3::new(-half, half, z),
    ];
    for corner in &corners {
        mesh.add_vertex(Vertex::new(corner, &Vec3::Z, &material));
    }
    mesh.add_quad(0, 1, 2, 3);
    mesh
}

/// Orthographic, looking straight down the -Z axis at the origin, +Y up, at
/// 10 render pixels per world unit in the default viewport.
fn top_down_orthographic(renderer: &mut Renderer) {
    renderer.set_orthographic_mode(true);
    renderer.set_ortho_half_height(VIEWPORT_HEIGHT as f64 / 20.0);
    renderer.move_camera(
        &DVec3::new(0.0, 0.0, 50.0),
        &DVec3::ZERO,
        &DVec3::new(0.0, 1.0, 0.0),
    );
}

/// The BGRA pixel at `(column, row)` of a live render, as RGBA.
fn rgba_at(pixels: &[u8], width: u32, column: u32, row: u32) -> [u8; 4] {
    let i = ((row * width + column) * 4) as usize;
    [pixels[i + 2], pixels[i + 1], pixels[i], pixels[i + 3]]
}

/// The pixel of a top-down orthographic live render where world `(x, y)` is.
fn pixel_of(x: f32, y: f32) -> (u32, u32) {
    (
        (VIEWPORT_WIDTH as f32 / 2.0 + x * 10.0) as u32,
        (VIEWPORT_HEIGHT as f32 / 2.0 - y * 10.0) as u32,
    )
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn anti_aliased_lines_leave_every_exported_and_thumbnail_pixel_opaque() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_fixed_scene(&mut renderer);
    // Diagonal wireframe lines, so plenty of fringe pixels are partial.
    let mut wireframe = LineMesh::new();
    for i in 0..8 {
        let t = i as f32;
        wireframe.add_line_with_uniform_color(
            &Vec3::new(-6.0 + t, -6.0, 2.0),
            &Vec3::new(6.0 + t * 0.3, 6.0, 2.0),
            &[1.0, 1.0, 0.0],
        );
    }
    upload(
        &mut renderer,
        &square(4.0, 0.0, Vec3::new(0.8, 0.3, 0.2)),
        &wireframe,
        &Mesh::new(),
        &LineMesh::new(),
    );

    let export = renderer.render_image(300, 200, BACKGROUND_RGB);
    dump("alpha_export", &export);
    let translucent = export.chunks_exact(4).filter(|p| p[3] != 255).count();
    assert_eq!(translucent, 0, "translucent pixels in the export");

    let bounds = renderer.content_bounds().expect("content has bounds");
    let thumbnail = renderer.render_thumbnail(&fit_camera_to_bounds(&renderer.camera, bounds));
    dump("alpha_thumbnail", &thumbnail);
    let translucent = thumbnail.chunks_exact(4).filter(|p| p[3] != 255).count();
    assert_eq!(translucent, 0, "translucent pixels in the thumbnail");
}

/// A ground grid seen from just above it, looking at the horizon, with the
/// camera inside the grid: half of every line running away from the camera
/// lies behind it, and only the near-plane clip keeps the half in front.
///
/// What goes wrong without the clip depends on how the corners are emitted.
/// Dividing by `w` and emitting `w = 1` paints a wedge across the sky; this
/// shader emits each corner at its own end's `w`, and then the receding lines
/// vanish instead (their quads straddle `w = 0`). The test checks for both.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_grid_at_a_grazing_angle_lights_nothing_above_the_horizon() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let mut grid = LineMesh::new();
    let white = [1.0, 1.0, 1.0];
    for i in -100..=100 {
        let t = i as f32 * 2.0;
        grid.add_line_with_uniform_color(
            &Vec3::new(t, -200.0, 0.0),
            &Vec3::new(t, 200.0, 0.0),
            &white,
        );
        grid.add_line_with_uniform_color(
            &Vec3::new(-200.0, t, 0.0),
            &Vec3::new(200.0, t, 0.0),
            &white,
        );
    }
    renderer.update_background_mesh(&grid);
    // Eye 1 unit above the plane, looking horizontally along +Y: the
    // horizon is the middle row of the image.
    renderer.move_camera(
        &DVec3::new(3.0, 0.0, 1.0),
        &DVec3::new(3.0, 100.0, 1.0),
        &DVec3::Z,
    );

    let live = renderer.render(BLACK);
    dump("grazing_grid", &live);
    let row_bytes = VIEWPORT_WIDTH as usize * 4;
    let horizon = VIEWPORT_HEIGHT as usize / 2;
    let lit_below = count_non_background(&live[(horizon + 2) * row_bytes..], BLACK, true);
    assert!(lit_below > 1000, "the grid is not drawn: {lit_below}");
    // A couple of rows of margin for the fringe of the lines at the horizon.
    let lit_above = count_non_background(&live[..(horizon - 2) * row_bytes], BLACK, true);
    assert_eq!(lit_above, 0, "pixels lit above the horizon (a wedge)");

    // The lines running towards the horizon cross the rows below it as
    // separate short runs; the lines across the view light whole rows.
    let most_runs = (horizon + 10..VIEWPORT_HEIGHT as usize)
        .map(|row| {
            let lit: Vec<bool> = (0..VIEWPORT_WIDTH)
                .map(|column| rgba_at(&live, VIEWPORT_WIDTH, column, row as u32)[0] > 4)
                .collect();
            lit.windows(2).filter(|w| w[1] && !w[0]).count() + lit[0] as usize
        })
        .max()
        .unwrap();
    assert!(
        most_runs >= 3,
        "the lines running towards the horizon are missing: at most {most_runs} runs per row"
    );
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn an_axis_seen_end_on_is_a_small_dot() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let mut axis = LineMesh::new();
    axis.add_line_with_uniform_color(
        &Vec3::new(0.0, 0.0, -5.0),
        &Vec3::new(0.0, 0.0, 5.0),
        &[1.0, 1.0, 1.0],
    );
    upload(
        &mut renderer,
        &Mesh::new(),
        &axis,
        &Mesh::new(),
        &LineMesh::new(),
    );
    for orthographic in [false, true] {
        renderer.set_orthographic_mode(orthographic);
        renderer.move_camera(
            &DVec3::new(0.0, 0.0, 30.0),
            &DVec3::ZERO,
            &DVec3::new(0.0, 1.0, 0.0),
        );
        let live = renderer.render(BLACK);
        dump(&format!("end_on_{orthographic}"), &live);
        let (cx, cy) = (VIEWPORT_WIDTH as i32 / 2, VIEWPORT_HEIGHT as i32 / 2);
        let mut lit = 0;
        for (index, p) in live.chunks_exact(4).enumerate() {
            if p[0] > 4 || p[1] > 4 || p[2] > 4 {
                lit += 1;
                let (x, y) = (
                    (index as u32 % VIEWPORT_WIDTH) as i32,
                    (index as u32 / VIEWPORT_WIDTH) as i32,
                );
                assert!(
                    (x - cx).abs() <= 3 && (y - cy).abs() <= 3,
                    "lit pixel ({x}, {y}) far from the centre (orthographic: {orthographic})"
                );
            }
        }
        assert!(
            (1..=25).contains(&lit),
            "expected a small dot, got {lit} lit pixels (orthographic: {orthographic})"
        );
    }
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_one_pixel_line_sums_to_one_pixel_of_coverage_per_column() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let mut line = LineMesh::new();
    // A third of a pixel off a pixel centre, so the coverage is split over
    // two rows.
    line.add_line_with_uniform_color(
        &Vec3::new(-12.0, 0.083, 0.0),
        &Vec3::new(12.0, 0.083, 0.0),
        &[1.0, 1.0, 1.0],
    );
    upload(
        &mut renderer,
        &Mesh::new(),
        &line,
        &Mesh::new(),
        &LineMesh::new(),
    );
    top_down_orthographic(&mut renderer);
    let live = renderer.render(BLACK);
    dump("one_pixel_line", &live);

    let mut split_columns = 0;
    for column in 60..260 {
        let coverages: Vec<f32> = (0..VIEWPORT_HEIGHT)
            .map(|row| rgba_at(&live, VIEWPORT_WIDTH, column, row)[0] as f32 / 255.0)
            .collect();
        let sum: f32 = coverages.iter().sum();
        assert!(
            (sum - 1.0).abs() < 0.03,
            "column {column}: coverage sums to {sum}"
        );
        if coverages.iter().filter(|c| **c > 0.05).count() >= 2 {
            split_columns += 1;
        }
    }
    assert!(split_columns > 150, "the line is not anti-aliased");
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_line_behind_an_opaque_face_is_hidden_and_one_in_front_is_not() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let mut lines = LineMesh::new();
    let red = LineStyle::solid([1.0, 0.0, 0.0]).with_width(3.0);
    let green = LineStyle::solid([0.0, 1.0, 0.0]).with_width(3.0);
    lines.add_segment(
        &Vec3::new(-12.0, 2.0, -1.0),
        &Vec3::new(12.0, 2.0, -1.0),
        &red,
    );
    lines.add_segment(
        &Vec3::new(-12.0, -2.0, 1.0),
        &Vec3::new(12.0, -2.0, 1.0),
        &green,
    );
    upload(
        &mut renderer,
        &square(4.0, 0.0, Vec3::new(0.1, 0.1, 0.9)),
        &lines,
        &Mesh::new(),
        &LineMesh::new(),
    );
    top_down_orthographic(&mut renderer);
    let live = renderer.render(BLACK);
    dump("main_pass_order", &live);

    // Outside the square both lines show.
    let (x, y) = pixel_of(-8.0, 2.0);
    let outside_red = rgba_at(&live, VIEWPORT_WIDTH, x, y);
    assert!(
        outside_red[0] > 200,
        "red line outside the face: {outside_red:?}"
    );
    // Over the square: the red line behind is hidden, the green in front shows.
    let (x, y) = pixel_of(0.0, 2.0);
    let behind = rgba_at(&live, VIEWPORT_WIDTH, x, y);
    assert!(
        behind[0] < behind[2],
        "the line behind the face shows through: {behind:?}"
    );
    let (x, y) = pixel_of(0.0, -2.0);
    let in_front = rgba_at(&live, VIEWPORT_WIDTH, x, y);
    assert!(
        in_front[1] > 200 && in_front[0] < 50 && in_front[2] < 50,
        "the line in front of the face is not drawn over it: {in_front:?}"
    );
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_gadget_line_in_front_of_a_gadget_triangle_keeps_its_colour() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let mut gadget_lines = LineMesh::new();
    gadget_lines.add_segment(
        &Vec3::new(-12.0, 0.0, 1.0),
        &Vec3::new(12.0, 0.0, 1.0),
        &LineStyle::solid([0.0, 1.0, 0.0]).with_width(3.0),
    );
    upload(
        &mut renderer,
        &Mesh::new(),
        &LineMesh::new(),
        &square(4.0, 0.0, Vec3::new(0.1, 0.1, 0.9)),
        &gadget_lines,
    );
    top_down_orthographic(&mut renderer);
    let live = renderer.render(BLACK);
    dump("gadget_order", &live);

    let (x, y) = pixel_of(0.0, 0.0);
    let crossing = rgba_at(&live, VIEWPORT_WIDTH, x, y);
    assert!(
        crossing[1] > 200 && crossing[0] < 50 && crossing[2] < 50,
        "the gadget triangle painted over the gadget line: {crossing:?}"
    );
    // And the triangle is there around it.
    let (x, y) = pixel_of(0.0, 2.0);
    let face = rgba_at(&live, VIEWPORT_WIDTH, x, y);
    assert!(face[2] > face[1], "no gadget triangle: {face:?}");
}

// ---- Screen-space lines, Phase 3 (doc/design_screen_space_lines.md) --------

/// The red channel of every column summed down the image, in units of full
/// coverage: for a white line on black, the line's coverage per column.
/// `bgra` = a live render; otherwise an RGBA export.
fn column_coverage(pixels: &[u8], width: u32, height: u32, bgra: bool) -> Vec<f32> {
    let red = if bgra { 2 } else { 0 };
    (0..width)
        .map(|column| {
            (0..height)
                .map(|row| pixels[((row * width + column) * 4) as usize + red] as f32 / 255.0)
                .sum()
        })
        .collect()
}

/// A white horizontal line across the middle of a top-down orthographic view
/// from x = −12 to 12 (columns 40 to 280), in the content's wireframe.
fn render_horizontal_line(style: LineStyle) -> Option<(Vec<f32>, Vec<f32>)> {
    let mut renderer = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
    let mut line = LineMesh::new();
    line.add_segment(
        &Vec3::new(-12.0, 0.03, 0.0),
        &Vec3::new(12.0, 0.03, 0.0),
        &style,
    );
    upload(
        &mut renderer,
        &Mesh::new(),
        &line,
        &Mesh::new(),
        &LineMesh::new(),
    );
    top_down_orthographic(&mut renderer);

    let live = renderer.render(BLACK);
    // At the viewport's size, the export frames the same view at 2×.
    let export = renderer.render_image(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, BLACK);
    let dashed = style.dash.is_some();
    dump(&format!("line_live_dashed_{dashed}"), &live);
    dump(&format!("line_export_dashed_{dashed}"), &export);
    Some((
        column_coverage(&live, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, true),
        column_coverage(&export, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, false),
    ))
}

/// D2 end to end: a line authored as 2 px is 2 px in the live view and 2 px
/// in a 2× export after the downsample. Without the target's pixel scale the
/// export would show a 1 px line.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_two_pixel_line_is_two_pixels_wide_live_and_exported() {
    let Some((live, export)) =
        render_horizontal_line(LineStyle::solid([1.0, 1.0, 1.0]).with_width(2.0))
    else {
        return;
    };
    for (name, coverage) in [("live", &live), ("export", &export)] {
        for (column, sum) in coverage.iter().enumerate().take(260).skip(60) {
            assert!(
                (sum - 2.0).abs() < 0.06,
                "{name}, column {column}: coverage sums to {sum}"
            );
        }
    }
}

/// The columns, from `range`, at which `coverage / width` rises through 0.5:
/// where each dash starts.
fn dash_starts(coverage: &[f32], width: f32, range: std::ops::Range<usize>) -> Vec<usize> {
    range
        .filter(|&c| coverage[c - 1] / width < 0.5 && coverage[c] / width >= 0.5)
        .collect()
}

/// D5: a 6 / 4 px dash pattern repeats every 10 px, in the live view and in
/// a 2× export alike, and is on for 60 % of its length.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_dash_period_is_on_plus_off_pixels_live_and_exported() {
    let width = 2.0;
    let Some((live, export)) = render_horizontal_line(
        LineStyle::solid([1.0, 1.0, 1.0])
            .with_width(width)
            .dashed(6.0, 4.0),
    ) else {
        return;
    };
    for (name, coverage) in [("live", &live), ("export", &export)] {
        let starts = dash_starts(coverage, width, 60..260);
        assert!(
            starts.len() >= 18,
            "{name}: only {} dashes in 200 px",
            starts.len()
        );
        for pair in starts.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                (9..=11).contains(&gap),
                "{name}: dashes {gap} px apart at column {}",
                pair[0]
            );
        }
        let mean_period = (starts[starts.len() - 1] - starts[0]) as f32 / (starts.len() - 1) as f32;
        assert!(
            (mean_period - 10.0).abs() < 0.1,
            "{name}: mean period {mean_period}"
        );
        // Over whole periods the pattern is on for 6 px in 10.
        let span = starts[0]..starts[starts.len() - 1];
        let on = span.clone().map(|c| coverage[c] / width).sum::<f32>() / span.len() as f32;
        assert!((on - 0.6).abs() < 0.03, "{name}: on for {on} of the period");
    }
}

// ---- Screen-space lines, Phase 4 (doc/design_screen_space_lines.md D9) -----

use atomcad_renderer::line_lod::LodSlot;

/// A ground grid in `z = 0`, `±extent` units, lines 1 unit apart and every
/// tenth a major line, with a minor and a major LOD slot as the display
/// crate's grids register them (the major fading with the minor). White on the default backgrounds.
fn slotted_ground_grid(extent: i32) -> LineMesh {
    let mut grid = LineMesh::new();
    let slot = |spacing: f32, fades_with: u32| LodSlot {
        plane_point: Vec3::ZERO,
        plane_normal: Vec3::Z,
        spacing,
        fades_with,
    };
    let minor = grid.add_lod_slot(slot(1.0, 0));
    let major = grid.add_lod_slot(slot(10.0, minor));
    let white = LineStyle::solid([1.0, 1.0, 1.0]);
    let e = extent as f32;
    for i in -extent..=extent {
        let style = white.with_lod_slot(if i % 10 == 0 { major } else { minor });
        let t = i as f32;
        grid.add_segment(&Vec3::new(t, -e, 0.0), &Vec3::new(t, e, 0.0), &style);
        grid.add_segment(&Vec3::new(-e, t, 0.0), &Vec3::new(e, t, 0.0), &style);
    }
    grid
}

/// Lit pixels (any channel above a few levels) in the rectangle `columns ×
/// rows` of a `width`-wide image.
fn lit_in(
    pixels: &[u8],
    width: u32,
    columns: std::ops::Range<u32>,
    rows: std::ops::Range<u32>,
) -> usize {
    rows.flat_map(|row| columns.clone().map(move |column| (column, row)))
        .filter(|&(column, row)| {
            let i = ((row * width + column) * 4) as usize;
            pixels[i..i + 3].iter().any(|&c| c > 4)
        })
        .count()
}

/// Perspective, looking straight down at the origin from height `h`, +Y up.
fn top_down_perspective(renderer: &mut Renderer, h: f64) {
    renderer.set_orthographic_mode(false);
    renderer.move_camera(
        &DVec3::new(0.0, 0.0, h),
        &DVec3::ZERO,
        &DVec3::new(0.0, 1.0, 0.0),
    );
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_grid_zoomed_out_past_the_minor_threshold_lights_only_major_lines() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.update_background_mesh(&slotted_ground_grid(60));
    // Focal length in pixels: the minor slot drops above h = f / T_drop.
    let f = VIEWPORT_HEIGHT as f64 / 2.0 / (renderer.camera.fovy / 2.0).tan();
    let drop_h = f / atomcad_renderer::line_lod::T_DROP_PX as f64;

    // The open cell between the major lines x, y = 0 and x, y = 10, with a
    // margin for the major lines' width and fringe.
    let cell = |h: f64| {
        let px_per_unit = f / h;
        let column = |x: f64| (VIEWPORT_WIDTH as f64 / 2.0 + x * px_per_unit) as u32;
        let row = |y: f64| (VIEWPORT_HEIGHT as f64 / 2.0 - y * px_per_unit) as u32;
        (
            column(0.0) + 4..column(10.0) - 3,
            row(10.0) + 4..row(0.0) - 3,
        )
    };

    // Zoomed in, the minor lines light the cell: the check below can fail.
    let near = drop_h * 0.5;
    top_down_perspective(&mut renderer, near);
    let live = renderer.render(BLACK);
    dump("lod_near", &live);
    let (columns, rows) = cell(near);
    assert!(lit_in(&live, VIEWPORT_WIDTH, columns, rows) > 20);

    // Zoomed out past the threshold, the cell is empty but the major lines
    // around it are still drawn.
    let far = drop_h * 1.5;
    top_down_perspective(&mut renderer, far);
    let live = renderer.render(BLACK);
    dump("lod_far", &live);
    let (columns, rows) = cell(far);
    assert!(columns.len() > 5 && rows.len() > 5, "the cell is too small");
    assert_eq!(
        lit_in(&live, VIEWPORT_WIDTH, columns.clone(), rows.clone()),
        0,
        "minor lines drawn past the threshold"
    );
    let major_x0 = (VIEWPORT_WIDTH / 2 - 1)..(VIEWPORT_WIDTH / 2 + 2);
    assert!(lit_in(&live, VIEWPORT_WIDTH, major_x0, rows.clone()) > 0);

    // A 2× export makes the same choice (its own uniforms, plain threshold).
    let export = renderer.render_image(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, BLACK);
    dump("lod_far_export", &export);
    assert_eq!(
        lit_in(&export, VIEWPORT_WIDTH, columns.clone(), rows.clone()),
        0,
        "minor lines drawn in the export"
    );
    // …and leaves the live view's LOD alone.
    assert_eq!(renderer.render(BLACK), live);
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_low_perspective_grid_fades_towards_the_horizon() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.update_background_mesh(&slotted_ground_grid(200));
    renderer.set_orthographic_mode(false);
    // Eye 2 units up, looking almost level along +Y.
    renderer.move_camera(
        &DVec3::new(0.3, 0.0, 2.0),
        &DVec3::new(0.3, 100.0, 0.0),
        &DVec3::Z,
    );
    let live = renderer.render(BLACK);
    dump("lod_horizon", &live);

    // The mean brightness of a band of rows.
    let mean = |rows: std::ops::Range<u32>| {
        let count = rows.len() as f64 * VIEWPORT_WIDTH as f64;
        let sum: f64 = rows
            .flat_map(|row| (0..VIEWPORT_WIDTH).map(move |column| (column, row)))
            .map(|(column, row)| rgba_at(&live, VIEWPORT_WIDTH, column, row)[0] as f64)
            .sum();
        sum / count
    };
    // The horizon is a few rows above the middle; the band just below it is
    // tens of units away, the bottom rows only a few.
    let horizon = VIEWPORT_HEIGHT / 2 - 8;
    let near_horizon = mean(horizon + 2..horizon + 12);
    let bottom = mean(VIEWPORT_HEIGHT - 20..VIEWPORT_HEIGHT);
    assert!(
        bottom > 10.0,
        "the grid near the eye is not drawn: {bottom}"
    );
    assert!(
        near_horizon < 0.5 * bottom,
        "near the horizon {near_horizon}, near the bottom {bottom}"
    );

    // The grid ends as a whole: the major lines fade with the minor ones
    // rather than running on alone towards the horizon. In this view, with
    // their own fade distances, they would light the rows from about 10 to
    // 28 below the horizon, where the minor lines are already gone.
    let major_only = mean(horizon + 10..horizon + 28);
    assert!(
        major_only < 0.02 * bottom,
        "lines left near the horizon: {major_only} (bottom {bottom})"
    );
}

// ---- Viewport anti-aliasing, Phase 1 (doc/design_viewport_antialiasing.md) --

use atomcad_renderer::render_target::{LIVE_SAMPLE_COUNT, RenderQuality};

/// An opaque square of half-size `half` in `z = 0`, rotated by `angle`
/// radians about +Z, so none of its edges runs along a pixel row or column.
fn tilted_square(half: f32, angle: f32, albedo: Vec3) -> Mesh {
    let mut mesh = Mesh::new();
    let material = Material::new(&albedo, 0.5, 0.0);
    let (sin, cos) = angle.sin_cos();
    for (x, y) in [(-half, -half), (half, -half), (half, half), (-half, half)] {
        let corner = Vec3::new(x * cos - y * sin, x * sin + y * cos, 0.0);
        mesh.add_vertex(Vertex::new(&corner, &Vec3::Z, &material));
    }
    mesh.add_quad(0, 1, 2, 3);
    mesh
}

/// How many pixels of a live render of a uniformly coloured face on black are
/// strictly between the face's colour and the background: summed channels
/// between 15 % and 85 % of the face's, which is `interior`.
fn count_intermediate(pixels: &[u8], interior: [u8; 4]) -> usize {
    let full: u32 = interior[..3].iter().map(|&c| c as u32).sum();
    pixels
        .chunks_exact(4)
        .filter(|p| {
            let sum: u32 = p[..3].iter().map(|&c| c as u32).sum();
            sum * 100 > full * 15 && sum * 100 < full * 85
        })
        .count()
}

/// D1, D3: the edges of an opaque mesh in the live view are anti-aliased.
/// Mutation: with the live sample count forced to 1, no pixel is
/// intermediate and this fails.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_tilted_square_has_anti_aliased_edges_in_the_live_view() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    // The skip asks the adapter, not the live target, so a live target that
    // lost its samples fails here instead of skipping.
    if !renderer.msaa_supported() {
        println!("renderer_gpu: no 4x MSAA on this adapter, skipping");
        return;
    }
    upload(
        &mut renderer,
        &tilted_square(6.0, 0.4, Vec3::new(0.9, 0.9, 0.9)),
        &LineMesh::new(),
        &Mesh::new(),
        &LineMesh::new(),
    );
    top_down_orthographic(&mut renderer);
    let live = renderer.render(BLACK);
    dump("aa_mesh_edge", &live);

    let interior = rgba_at(
        &live,
        VIEWPORT_WIDTH,
        VIEWPORT_WIDTH / 2,
        VIEWPORT_HEIGHT / 2,
    );
    assert!(interior[0] > 50, "the square is not drawn: {interior:?}");
    // The perimeter is 4 × 120 px; most edge pixels are partly covered.
    let intermediate = count_intermediate(&live, interior);
    assert!(
        intermediate > 200,
        "the square's edges are not anti-aliased: {intermediate} intermediate pixels"
    );
}

/// D2, D8: anti-aliasing off renders the live view with one sample (the
/// pre-design image: compare the `live_aa_off` dump with a pre-phase `live`
/// dump), and turning it on again restores the anti-aliased image exactly.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn turning_anti_aliasing_off_and_on_restores_the_anti_aliased_image() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_fixed_scene(&mut renderer);
    let on = renderer.render(BACKGROUND_RGB);

    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        ..RenderQuality::default()
    });
    assert_eq!(renderer.live_sample_count(), 1);
    assert_eq!(
        renderer.get_viewport_size(),
        (VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
    );
    let off = renderer.render(BACKGROUND_RGB);
    dump("live_aa_off", &off);
    assert_eq!(off.len(), on.len());

    renderer.set_render_quality(RenderQuality::default());
    let on_again = renderer.render(BACKGROUND_RGB);
    assert_eq!(on, on_again, "anti-aliasing off and on changed the image");

    if renderer.msaa_supported() {
        assert_eq!(renderer.live_sample_count(), LIVE_SAMPLE_COUNT);
        assert_ne!(on, off, "anti-aliasing changed nothing");
    } else {
        println!("renderer_gpu: no 4x MSAA on this adapter, both images are 1-sample");
    }
}

// ---- Viewport anti-aliasing, Phase 2 (doc/design_viewport_antialiasing.md) --
//
// Per-sample impostor coverage (D4). Mutation: with the shaders' mask forced
// to 0b1111, the silhouette, no-seam, bond, ghost and gadget tests fail.

/// Uploads impostors only: `atoms`, `bonds` and `ghosts` as content,
/// `gadget_atoms` as the gadget pass's guide dots.
fn upload_impostors(
    renderer: &mut Renderer,
    atoms: &AtomImpostorMesh,
    bonds: &BondImpostorMesh,
    ghosts: &TransparentImpostorMesh,
    gadget_atoms: &AtomImpostorMesh,
) {
    renderer.update_all_gpu_meshes(
        &Mesh::new(),
        &LineMesh::new(),
        &Mesh::new(),
        &LineMesh::new(),
        &LineMesh::new(),
        atoms,
        bonds,
        ghosts,
        &TransparentSurfaceMesh::new(),
        &LabelMesh::new(),
        gadget_atoms,
        &BondImpostorMesh::new(),
        true,
        Some((1, "Main".to_string())),
    );
}

fn atom(centre: Vec3, radius: f32, albedo: [f32; 3]) -> AtomImpostorMesh {
    let mut atoms = AtomImpostorMesh::new();
    atoms.add_atom_quad(&centre, radius, &albedo, 0.5, 0.0, &[0.0; 4]);
    atoms
}

/// A renderer at the default size with a multisampled live target, or
/// `None` (with a note) when there is no adapter or no 4× MSAA on it. The
/// skip asks the adapter, not the live target, so a live target that lost
/// its samples fails instead of skipping.
fn try_msaa_renderer() -> Option<Renderer> {
    let renderer = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
    if !renderer.msaa_supported() {
        println!("renderer_gpu: no 4x MSAA on this adapter, skipping");
        return None;
    }
    assert_eq!(renderer.live_sample_count(), LIVE_SAMPLE_COUNT);
    Some(renderer)
}

/// Per-pixel coverage of the live view, from one render on black and one on
/// white. A pixel resolved from opaque content covering a fraction `c` of
/// its samples is `c·colour + (1 − c)·background`, so the two renders differ
/// by `(1 − c)·255`. For blended content `c` is coverage times alpha.
fn live_coverage(renderer: &mut Renderer, name: &str) -> Vec<f32> {
    let on_black = renderer.render(BLACK);
    let on_white = renderer.render([255, 255, 255]);
    dump(&format!("{name}_on_black"), &on_black);
    dump(&format!("{name}_on_white"), &on_white);
    on_black
        .chunks_exact(4)
        .zip(on_white.chunks_exact(4))
        .map(|(b, w)| {
            let diff: f32 = (0..3).map(|c| w[c] as f32 - b[c] as f32).sum::<f32>() / 3.0;
            1.0 - diff / 255.0
        })
        .collect()
}

/// The world position of the centre of pixel `index` in the top-down
/// orthographic view (`top_down_orthographic`).
fn world_of_pixel(index: usize) -> (f32, f32) {
    let column = (index as u32 % VIEWPORT_WIDTH) as f32;
    let row = (index as u32 / VIEWPORT_WIDTH) as f32;
    (
        (column + 0.5 - VIEWPORT_WIDTH as f32 / 2.0) / 10.0,
        (VIEWPORT_HEIGHT as f32 / 2.0 - row - 0.5) / 10.0,
    )
}

fn intermediate(coverage: &[f32], low: f32, high: f32) -> usize {
    coverage.iter().filter(|&&c| c > low && c < high).count()
}

/// D4: the summed coverage of a sphere of radius 20 px is its area, and its
/// outline has partly covered pixels.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_spheres_summed_coverage_is_its_area() {
    let Some(mut renderer) = try_msaa_renderer() else {
        return;
    };
    top_down_orthographic(&mut renderer);
    upload_impostors(
        &mut renderer,
        &atom(Vec3::ZERO, 2.0, [0.9, 0.9, 0.9]),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    let coverage = live_coverage(&mut renderer, "aa_sphere_20px");
    let area: f32 = coverage.iter().sum();
    let expected = std::f32::consts::PI * 20.0 * 20.0;
    assert!(
        (area / expected - 1.0).abs() < 0.02,
        "coverage {area} px² vs π r² = {expected}"
    );
    let edge = intermediate(&coverage, 0.1, 0.9);
    assert!(edge > 60, "the outline is not anti-aliased: {edge} pixels");
}

/// D4: a sub-pixel sphere (r = 0.7 px) rendered at 16 sub-pixel offsets
/// covers π r² on average, and its coverage changes with the offset (a
/// single sample would make each render all-or-nothing).
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_sub_pixel_spheres_mean_coverage_is_its_area() {
    let Some(mut renderer) = try_msaa_renderer() else {
        return;
    };
    top_down_orthographic(&mut renderer);
    let mut areas = Vec::new();
    for i in 0..4 {
        for j in 0..4 {
            // Quarter-pixel steps, in world units (10 px per unit).
            let centre = Vec3::new((i as f32 + 0.125) * 0.025, (j as f32 + 0.125) * 0.025, 0.0);
            upload_impostors(
                &mut renderer,
                &atom(centre, 0.07, [0.9, 0.9, 0.9]),
                &BondImpostorMesh::new(),
                &TransparentImpostorMesh::new(),
                &AtomImpostorMesh::new(),
            );
            let coverage = live_coverage(&mut renderer, &format!("aa_sphere_subpx_{i}_{j}"));
            areas.push(coverage.iter().sum::<f32>());
        }
    }
    let mean = areas.iter().sum::<f32>() / areas.len() as f32;
    let expected = std::f32::consts::PI * 0.7 * 0.7;
    assert!(
        (mean / expected - 1.0).abs() < 0.1,
        "mean coverage {mean} px² vs π r² = {expected} ({areas:?})"
    );
    let first = areas[0];
    assert!(
        areas.iter().any(|a| (a - first).abs() > 0.1),
        "every offset gives the same coverage: {areas:?}"
    );
}

/// D4: one opaque sphere in front of another with a depth gap. Along the
/// front sphere's silhouette over the back one the two colours mix, and the
/// background never shows through, whichever is drawn first.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_sphere_in_front_of_another_leaves_no_seam() {
    let Some(mut renderer) = try_msaa_renderer() else {
        return;
    };
    top_down_orthographic(&mut renderer);
    const BLUE: [u8; 3] = [0, 0, 255];
    // Red, 20 px, high above a green one of 25 px whose centre is 25 px to
    // the right: the front silhouette crosses the back disc.
    let front = (Vec3::new(0.0, 0.0, 10.0), 2.0, [1.0, 0.0, 0.0]);
    let back = (Vec3::new(2.5, 0.0, 0.0), 2.5, [0.0, 1.0, 0.0]);
    for (order, first, second) in [("front_first", front, back), ("back_first", back, front)] {
        let mut atoms = AtomImpostorMesh::new();
        for (centre, radius, albedo) in [first, second] {
            atoms.add_atom_quad(&centre, radius, &albedo, 0.5, 0.0, &[0.0; 4]);
        }
        upload_impostors(
            &mut renderer,
            &atoms,
            &BondImpostorMesh::new(),
            &TransparentImpostorMesh::new(),
            &AtomImpostorMesh::new(),
        );
        let live = renderer.render(BLUE);
        dump(&format!("aa_no_seam_{order}"), &live);

        let mut arc = 0;
        let mut mixed = 0;
        for (index, pixel) in live.chunks_exact(4).enumerate() {
            let (x, y) = world_of_pixel(index);
            let to_front = (x * x + y * y).sqrt();
            let to_back = ((x - 2.5).powi(2) + y * y).sqrt();
            // Within 1.5 px of the front silhouette, well inside the back disc.
            if (to_front - 2.0).abs() > 0.15 || to_back > 2.1 {
                continue;
            }
            arc += 1;
            let [b, g, r] = [pixel[0] as f32, pixel[1] as f32, pixel[2] as f32];
            // One background sample of four would be 64 levels of blue on
            // top of the white specular both spheres add to every channel.
            assert!(
                b - r.min(g) < 40.0,
                "{order}: background shows through at {x:.2},{y:.2}: rgb {r} {g} {b}"
            );
            let redness = r / (r + g).max(1.0);
            if r + g > 30.0 && redness > 0.2 && redness < 0.8 {
                mixed += 1;
            }
        }
        assert!(arc > 60, "{order}: arc too short to judge: {arc}");
        assert!(
            mixed > 15,
            "{order}: the silhouette over the back sphere is not anti-aliased: {mixed} of {arc}"
        );
    }
}

/// D4: a bond's long edges are anti-aliased, and its flat ends do not paint
/// past the clip planes.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_bonds_long_edges_are_anti_aliased_and_its_ends_stay_clipped() {
    let Some(mut renderer) = try_msaa_renderer() else {
        return;
    };
    top_down_orthographic(&mut renderer);
    let angle: f32 = 0.3;
    let axis = Vec3::new(angle.cos(), angle.sin(), 0.0);
    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_with_positions(&(axis * -6.0), &(axis * 6.0), 1.0, &[0.9, 0.9, 0.9]);
    upload_impostors(
        &mut renderer,
        &AtomImpostorMesh::new(),
        &bonds,
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    let coverage = live_coverage(&mut renderer, "aa_bond");

    let mut edge = 0;
    for (index, &c) in coverage.iter().enumerate() {
        let (x, y) = world_of_pixel(index);
        let along = x * axis.x + y * axis.y;
        let across = (-x * axis.y + y * axis.x).abs();
        if along.abs() > 6.15 {
            assert!(c < 0.02, "paints past the end at {x:.2},{y:.2}: {c}");
        } else if along.abs() < 5.85 && across < 0.85 {
            assert!(c > 0.98, "a hole inside the bond at {x:.2},{y:.2}: {c}");
        } else if along.abs() < 5.0 && c > 0.1 && c < 0.9 {
            edge += 1;
        }
    }
    // Two long edges of 100 px each.
    assert!(
        edge > 100,
        "the long edges are not anti-aliased: {edge} pixels"
    );
}

/// D4: a ghost (x-ray) atom's silhouette composites partly covered pixels,
/// and every pixel of the read-back image stays opaque.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_ghosts_silhouette_is_anti_aliased_and_the_image_stays_opaque() {
    let Some(mut renderer) = try_msaa_renderer() else {
        return;
    };
    top_down_orthographic(&mut renderer);
    let alpha = 0.5;
    let mut ghosts = TransparentImpostorMesh::new();
    ghosts.add_atom_quad(
        &Vec3::ZERO,
        2.0,
        &[0.9, 0.9, 0.9],
        0.5,
        0.0,
        &[0.0; 4],
        alpha,
    );
    upload_impostors(
        &mut renderer,
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
        &ghosts,
        &AtomImpostorMesh::new(),
    );
    for background in [BLACK, [255, 255, 255]] {
        let live = renderer.render(background);
        assert!(
            live.chunks_exact(4).all(|p| p[3] == 255),
            "a pixel is not opaque on {background:?}"
        );
    }
    // Coverage times alpha: `alpha` inside, 0 outside, in between on the edge.
    let coverage = live_coverage(&mut renderer, "aa_ghost");
    let inside = coverage[(VIEWPORT_HEIGHT / 2 * VIEWPORT_WIDTH + VIEWPORT_WIDTH / 2) as usize];
    assert!((inside - alpha).abs() < 0.02, "interior {inside}");
    let edge = intermediate(&coverage, 0.1 * alpha, 0.9 * alpha);
    assert!(
        edge > 60,
        "the ghost's outline is not anti-aliased: {edge} pixels"
    );
    let area: f32 = coverage.iter().sum::<f32>() / alpha;
    let expected = std::f32::consts::PI * 20.0 * 20.0;
    assert!((area / expected - 1.0).abs() < 0.03, "{area} vs {expected}");
}

/// D4: a guide dot drawn in the gadget pass, with the content atom pipeline,
/// is anti-aliased too.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_gadget_guide_dot_is_anti_aliased() {
    let Some(mut renderer) = try_msaa_renderer() else {
        return;
    };
    top_down_orthographic(&mut renderer);
    upload_impostors(
        &mut renderer,
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &atom(Vec3::ZERO, 1.0, [0.9, 0.9, 0.9]),
    );
    let coverage = live_coverage(&mut renderer, "aa_gadget_dot");
    let area: f32 = coverage.iter().sum();
    let expected = std::f32::consts::PI * 10.0 * 10.0;
    assert!((area / expected - 1.0).abs() < 0.03, "{area} vs {expected}");
    let edge = intermediate(&coverage, 0.1, 0.9);
    assert!(
        edge > 30,
        "the guide dot is not anti-aliased: {edge} pixels"
    );
}

// ---- Viewport anti-aliasing, Phase 3 (doc/design_viewport_antialiasing.md) --
//
// The still-camera refined frame (D5). Refined frames for pixel tests come
// from `render_still`; the tests of the decision set a zero delay and read
// `last_frame_kind`. Mutations: draw the refined frame with the normal
// pipelines (the junction and contact tests fail); in the transparent
// shader's per-sample entry point multiply the ghost's alpha by 4 (the
// refined-matches-normal test fails).

use atomcad_renderer::live_refinement::LiveFrameKind;
use std::time::Duration;

/// A multisampled renderer whose adapter can also shade per sample, or
/// `None` (with a note). Like `try_msaa_renderer`, the skip asks the
/// adapter, so a renderer that lost its refined pipelines fails instead.
fn try_refining_renderer() -> Option<Renderer> {
    let renderer = try_msaa_renderer()?;
    if !renderer.refinement_supported() {
        println!("renderer_gpu: no per-sample shading on this adapter, skipping");
        return None;
    }
    Some(renderer)
}

/// The indices of the pixels of a top-down orthographic live render within
/// `half_width` world units of the line through `point` along the unit
/// vector `direction`, and within `half_length` of `point` along it.
fn pixels_along(
    point: (f32, f32),
    direction: (f32, f32),
    half_length: f32,
    half_width: f32,
) -> Vec<usize> {
    (0..(VIEWPORT_WIDTH * VIEWPORT_HEIGHT) as usize)
        .filter(|&index| {
            let (x, y) = world_of_pixel(index);
            let (dx, dy) = (x - point.0, y - point.1);
            let along = dx * direction.0 + dy * direction.1;
            let across = -dx * direction.1 + dy * direction.0;
            along.abs() <= half_length && across.abs() <= half_width
        })
        .collect()
}

/// Checks a red-against-green boundary along `band` (pixel indices of a
/// live render): in the normal frame every pixel is one colour or the other
/// (the D4 staircase: green channel at most 40 on the red side, at least 110
/// on the green one, in these scenes), and the refined frame differs from
/// it by more than 24 levels on more than `min_changed` of them, its partly
/// covered pixels.
fn assert_refined_boundary(
    normal: &[u8],
    refined: &[u8],
    band: &[usize],
    min_changed: usize,
    what: &str,
) {
    let green = |pixels: &[u8], index: usize| pixels[index * 4 + 1];
    let between: Vec<u8> = band
        .iter()
        .map(|&index| green(normal, index))
        .filter(|&g| g > 40 && g < 110)
        .collect();
    assert!(
        between.is_empty(),
        "{what}: a normal frame already mixes the colours ({between:?}), so the test proves nothing"
    );
    let changed = band
        .iter()
        .filter(|&&index| {
            (0..3)
                .any(|c| (normal[index * 4 + c] as i32 - refined[index * 4 + c] as i32).abs() > 24)
        })
        .count();
    assert!(
        changed > min_changed,
        "{what}: the refined frame is not anti-aliased: {changed} of {} pixels changed",
        band.len()
    );
}

/// D5: a bond entering a differently coloured atom. Seen from above with the
/// bond's axis through the atom's centre, the two surfaces meet along a
/// straight segment, an intersection with no depth gap. The refined frame
/// mixes the colours along it; a normal frame (D4) decides each pixel
/// whole, which pins down that the refined frame is what fixes it.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_bond_atom_junction_is_anti_aliased_in_the_refined_frame() {
    let Some(mut renderer) = try_refining_renderer() else {
        return;
    };
    top_down_orthographic(&mut renderer);
    // A red atom of radius 4 and a green bond of radius 2 along `axis`: the
    // junction is the segment at distance sqrt(4^2 - 2^2) along the axis, 4
    // long, tilted so it crosses the pixel grid.
    let angle: f32 = 0.3;
    let axis = (angle.cos(), angle.sin());
    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_with_positions(
        &Vec3::ZERO,
        &Vec3::new(axis.0 * 9.0, axis.1 * 9.0, 0.0),
        2.0,
        &[0.0, 1.0, 0.0],
    );
    upload_impostors(
        &mut renderer,
        &atom(Vec3::ZERO, 4.0, [1.0, 0.0, 0.0]),
        &bonds,
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    let normal = renderer.render(BLACK);
    let refined = renderer.render_still(BLACK);
    dump("refined_junction_normal", &normal);
    dump("refined_junction", &refined);

    // Clear of the segment's ends, where it meets the silhouettes.
    let distance = (4.0f32 * 4.0 - 2.0 * 2.0).sqrt();
    let band = pixels_along(
        (axis.0 * distance, axis.1 * distance),
        (-axis.1, axis.0),
        1.7,
        0.15,
    );
    assert!(
        band.len() > 60,
        "junction too short to judge: {}",
        band.len()
    );
    assert_refined_boundary(&normal, &refined, &band, 15, "junction");
}

/// D5: two overlapping spheres of different colour, centres at the same
/// height: the contact is a straight segment, tilted across the pixel grid.
/// Mixed along it in the refined frame, not in a normal one.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_sphere_contact_is_anti_aliased_in_the_refined_frame() {
    let Some(mut renderer) = try_refining_renderer() else {
        return;
    };
    top_down_orthographic(&mut renderer);
    let angle: f32 = 0.3;
    let axis = (angle.cos(), angle.sin());
    let d = 1.5;
    let mut atoms = AtomImpostorMesh::new();
    for (sign, albedo) in [(-1.0, [1.0, 0.0, 0.0]), (1.0, [0.0, 1.0, 0.0])] {
        atoms.add_atom_quad(
            &Vec3::new(sign * axis.0 * d, sign * axis.1 * d, 0.0),
            3.0,
            &albedo,
            0.5,
            0.0,
            &[0.0; 4],
        );
    }
    upload_impostors(
        &mut renderer,
        &atoms,
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    let normal = renderer.render(BLACK);
    let refined = renderer.render_still(BLACK);
    dump("refined_contact_normal", &normal);
    dump("refined_contact", &refined);

    // The contact segment is 2 sqrt(3^2 - 1.5^2) = 5.2 long; stay clear of
    // its ends, where it meets the silhouettes.
    let band = pixels_along((0.0, 0.0), (-axis.1, axis.0), 2.2, 0.15);
    assert!(
        band.len() > 80,
        "contact too short to judge: {}",
        band.len()
    );
    assert_refined_boundary(&normal, &refined, &band, 20, "contact");
}

/// Whether the live pixel `index` is on an edge of the image: some
/// neighbour differs from it by more than 24 levels in a channel.
fn on_an_edge(pixels: &[u8], index: usize) -> bool {
    let (w, h) = (VIEWPORT_WIDTH as i64, VIEWPORT_HEIGHT as i64);
    let (x, y) = (index as i64 % w, index as i64 / w);
    let at = |x: i64, y: i64| &pixels[((y * w + x) * 4) as usize..][..3];
    let here = at(x, y);
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| {
            let (nx, ny) = (x + dx, y + dy);
            nx >= 0
                && ny >= 0
                && nx < w
                && ny < h
                && (0..3).any(|c| (at(nx, ny)[c] as i32 - here[c] as i32).abs() > 24)
        })
    })
}

/// Asserts that `refined` and `normal` agree within a few levels wherever
/// `normal` has no edge, and that `refined` is opaque.
fn assert_refined_matches_normal_away_from_edges(normal: &[u8], refined: &[u8], what: &str) {
    assert_eq!(normal.len(), refined.len());
    assert!(
        refined.chunks_exact(4).all(|p| p[3] == 255),
        "{what}: a refined pixel is not opaque"
    );
    let mut compared = 0;
    for (index, (n, r)) in normal
        .chunks_exact(4)
        .zip(refined.chunks_exact(4))
        .enumerate()
    {
        if on_an_edge(normal, index) {
            continue;
        }
        compared += 1;
        let worst = (0..3)
            .map(|c| (n[c] as i32 - r[c] as i32).abs())
            .max()
            .unwrap();
        assert!(
            worst <= 3,
            "{what}: pixel {index} differs by {worst} away from any edge: normal {n:?}, refined {r:?}"
        );
    }
    assert!(compared > 1000, "{what}: only {compared} pixels compared");
}

/// D5: away from impostor edges the refined frame is the normal one, for
/// the fixed scene and for a ghost over an opaque atom, and it stays
/// opaque. Catches a per-sample entry point that shades or blends
/// differently from `fs_main` (a wrong sample offset, a ghost weighted as
/// if it were the whole pixel), which the edge tests would not.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_refined_frame_matches_the_normal_one_away_from_edges() {
    let Some(mut renderer) = try_refining_renderer() else {
        return;
    };
    upload_fixed_scene(&mut renderer);
    let normal = renderer.render(BACKGROUND_RGB);
    let refined = renderer.render_still(BACKGROUND_RGB);
    dump("refined_fixed_scene", &refined);
    assert_refined_matches_normal_away_from_edges(&normal, &refined, "fixed scene");

    top_down_orthographic(&mut renderer);
    let mut ghosts = TransparentImpostorMesh::new();
    ghosts.add_atom_quad(
        &Vec3::new(1.0, 0.0, 5.0),
        3.0,
        &[0.2, 0.9, 0.3],
        0.5,
        0.0,
        &[0.0; 4],
        0.4,
    );
    upload_impostors(
        &mut renderer,
        &atom(Vec3::new(-1.0, 0.0, 0.0), 3.0, [0.9, 0.2, 0.2]),
        &BondImpostorMesh::new(),
        &ghosts,
        &AtomImpostorMesh::new(),
    );
    let normal = renderer.render(BACKGROUND_RGB);
    let refined = renderer.render_still(BACKGROUND_RGB);
    dump("refined_ghost_over_atom", &refined);
    assert_refined_matches_normal_away_from_edges(&normal, &refined, "ghost over atom");
}

/// D5's decision with a zero delay: after an upload the first frame is
/// normal and owes a refined one at once, the next is refined, the one after
/// that hands back the kept bytes, and a camera move starts over.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_resting_view_is_refined_once_and_then_kept() {
    let Some(mut renderer) = try_refining_renderer() else {
        return;
    };
    renderer.set_refine_delay(Some(Duration::ZERO));
    upload_fixed_scene(&mut renderer);

    let normal = renderer.render(BACKGROUND_RGB);
    assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Normal));
    assert_eq!(renderer.refine_in(), Some(Duration::ZERO));

    let refined = renderer.render(BACKGROUND_RGB);
    assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Refined));
    assert_eq!(renderer.refine_in(), None);
    assert!(normal != refined, "the refined frame changed nothing");

    let kept = renderer.render(BACKGROUND_RGB);
    assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Kept));
    assert!(kept == refined, "the kept frame is not the refined one");
    assert_eq!(renderer.refine_in(), None);

    // A different background is a change too.
    renderer.render(BLACK);
    assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Normal));
    renderer.render(BLACK);
    assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Refined));

    renderer.move_camera(
        &DVec3::new(10.0, -20.0, 30.0),
        &DVec3::ZERO,
        &DVec3::new(0.0, 0.0, 1.0),
    );
    renderer.render(BLACK);
    assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Normal));
}

/// D5: every mutating method of `Renderer` strictly increases the live
/// generation when it changes something, and the settings pushed on every
/// refresh do not when they are unchanged. The source-text check in
/// `live_refinement_test.rs` catches a setter missing from this list.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn every_setter_invalidates_the_refined_frame() {
    use atomcad_renderer::camera::CameraCanonicalView;
    use atomcad_renderer::transparent_surface_mesh::SurfaceTransparencyMode;
    use atomcad_util::transform::Transform;

    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let check = |what: &str, renderer: &mut Renderer, change: &dyn Fn(&mut Renderer)| {
        let before = renderer.live_generation();
        change(renderer);
        assert!(
            renderer.live_generation() > before,
            "{what} did not bump the live generation"
        );
    };
    check("update_all_gpu_meshes", &mut renderer, &upload_fixed_scene);
    check("update_background_mesh", &mut renderer, &|r| {
        r.update_background_mesh(&LineMesh::new())
    });
    check("move_camera", &mut renderer, &|r| {
        r.move_camera(
            &DVec3::new(1.0, 2.0, 30.0),
            &DVec3::ZERO,
            &DVec3::new(0.0, 1.0, 0.0),
        )
    });
    check("set_orthographic_mode", &mut renderer, &|r| {
        r.set_orthographic_mode(true)
    });
    check("set_ortho_half_height", &mut renderer, &|r| {
        r.set_ortho_half_height(7.0)
    });
    check("set_camera_canonical_view", &mut renderer, &|r| {
        r.set_camera_canonical_view(CameraCanonicalView::Front)
    });
    check("set_camera_transform", &mut renderer, &|r| {
        r.set_camera_transform(&Transform::new(
            DVec3::new(0.0, -30.0, 0.0),
            glam::f64::DQuat::IDENTITY,
        ))
    });
    check("a direct camera edit", &mut renderer, &|r| {
        r.camera.fovy *= 0.5;
        r.update_camera_buffer();
    });
    check("set_surface_transparency_mode", &mut renderer, &|r| {
        r.set_surface_transparency_mode(SurfaceTransparencyMode::SinglePass)
    });
    check("set_viewport_request", &mut renderer, &|r| {
        r.set_viewport_request((VIEWPORT_WIDTH + 10) as f64, VIEWPORT_HEIGHT as f64, 1.0)
    });
    check("set_viewport_request (ratio)", &mut renderer, &|r| {
        r.set_viewport_request((VIEWPORT_WIDTH + 10) as f64, VIEWPORT_HEIGHT as f64, 2.0)
    });
    if renderer.msaa_supported() {
        check("set_render_quality", &mut renderer, &|r| {
            r.set_render_quality(RenderQuality {
                anti_aliasing: false,
                ..RenderQuality::default()
            })
        });
    }
    check("set_render_quality (resolution)", &mut renderer, &|r| {
        r.set_render_quality(RenderQuality {
            full_display_resolution: false,
            ..r.render_quality()
        })
    });
    check("set_refine_delay", &mut renderer, &|r| {
        r.set_refine_delay(Some(Duration::ZERO))
    });

    // Pushed on every refresh: unchanged values must not bump, or every
    // refresh would throw the refined image away.
    let before = renderer.live_generation();
    renderer.set_surface_transparency_mode(SurfaceTransparencyMode::SinglePass);
    renderer.set_render_quality(renderer.render_quality());
    renderer.set_viewport_request((VIEWPORT_WIDTH + 10) as f64, VIEWPORT_HEIGHT as f64, 2.0);
    assert_eq!(
        renderer.live_generation(),
        before,
        "an unchanged setting bumped"
    );
}

/// D5, D8: with anti-aliasing off there is no refined frame, however long
/// the view rests, and none is owed.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn with_anti_aliasing_off_no_frame_is_refined() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_refine_delay(Some(Duration::ZERO));
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        ..RenderQuality::default()
    });
    upload_fixed_scene(&mut renderer);
    for _ in 0..4 {
        renderer.render(BACKGROUND_RGB);
        assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Normal));
        assert_eq!(renderer.refine_in(), None);
    }
}

// ---- Viewport anti-aliasing, Phase 4 (doc/design_viewport_antialiasing.md) --
//
// HiDPI (D7): the live target in device pixels, its pixel scale the device
// pixel ratio. Mutation: build the live target at the logical size (the
// device pixel ratio ignored in `TargetSpec::viewport`) and the HiDPI test
// fails.

/// The fixed test viewport's logical size, as a request.
const LOGICAL_W: f64 = VIEWPORT_WIDTH as f64;
const LOGICAL_H: f64 = VIEWPORT_HEIGHT as f64;

/// D7: at ratio 2 the live target is twice the logical size, and a line
/// authored as 2 (logical) px covers 4 render pixels per column, because the
/// target's pixel scale is the ratio.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn at_ratio_2_the_live_target_is_twice_the_logical_size_and_lines_scale() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_viewport_request(LOGICAL_W, LOGICAL_H, 2.0);
    let (width, height) = (VIEWPORT_WIDTH * 2, VIEWPORT_HEIGHT * 2);
    assert_eq!(renderer.get_viewport_size(), (width, height));

    let mut line = LineMesh::new();
    line.add_segment(
        &Vec3::new(-12.0, 0.03, 0.0),
        &Vec3::new(12.0, 0.03, 0.0),
        &LineStyle::solid([1.0, 1.0, 1.0]).with_width(2.0),
    );
    upload(
        &mut renderer,
        &Mesh::new(),
        &line,
        &Mesh::new(),
        &LineMesh::new(),
    );
    // 10 logical px per world unit, so 20 render px: the line spans render
    // columns 80 to 560.
    top_down_orthographic(&mut renderer);

    let live = renderer.render(BLACK);
    dump("hidpi_line_live", &live);
    assert_eq!(live.len(), (width * height * 4) as usize);
    let coverage = column_coverage(&live, width, height, true);
    for (column, sum) in coverage.iter().enumerate().take(520).skip(120) {
        assert!(
            (sum - 4.0).abs() < 0.12,
            "column {column}: coverage sums to {sum}, expected 4 render px"
        );
    }
    // Nothing outside the segment's ends.
    for (column, sum) in coverage.iter().enumerate().take(60) {
        assert!(*sum < 0.01, "column {column} lit beyond the line: {sum}");
    }
}

/// D8: with *Full display resolution* off the live target is the logical
/// size whatever the ratio, and turning it on again restores the full-size
/// image exactly.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn turning_full_display_resolution_off_and_on_restores_the_full_size_image() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_viewport_request(LOGICAL_W, LOGICAL_H, 2.0);
    upload_fixed_scene(&mut renderer);
    let full = renderer.render(BACKGROUND_RGB);
    dump("hidpi_fixed_scene", &full);
    assert_eq!(full.len(), (VIEWPORT_WIDTH * VIEWPORT_HEIGHT * 16) as usize);
    assert!(count_non_background(&full, BACKGROUND_RGB, true) > 4000);

    renderer.set_render_quality(RenderQuality {
        full_display_resolution: false,
        ..RenderQuality::default()
    });
    assert_eq!(
        renderer.get_viewport_size(),
        (VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
    );
    let logical = renderer.render(BACKGROUND_RGB);
    dump("hidpi_off_fixed_scene", &logical);
    assert_eq!(
        logical.len(),
        (VIEWPORT_WIDTH * VIEWPORT_HEIGHT * 4) as usize
    );
    assert!(count_non_background(&logical, BACKGROUND_RGB, true) > 1000);
    // Anti-aliasing is independent of the resolution.
    if renderer.msaa_supported() {
        assert_eq!(renderer.live_sample_count(), LIVE_SAMPLE_COUNT);
    }

    renderer.set_render_quality(RenderQuality::default());
    assert_eq!(
        renderer.get_viewport_size(),
        (VIEWPORT_WIDTH * 2, VIEWPORT_HEIGHT * 2)
    );
    let full_again = renderer.render(BACKGROUND_RGB);
    assert_eq!(
        full, full_again,
        "full display resolution off and on changed the image"
    );
}

/// D7: the logical size is what the export dialog's default and the gadget
/// pick context read; the device size is what is handed to Flutter.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_logical_viewport_size_is_the_requested_one_at_any_ratio() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_viewport_request(LOGICAL_W, LOGICAL_H, 2.0);
    assert_eq!(renderer.get_viewport_logical_size(), (LOGICAL_W, LOGICAL_H));
    assert_eq!(
        renderer.get_viewport_size(),
        (VIEWPORT_WIDTH * 2, VIEWPORT_HEIGHT * 2)
    );

    // At a fractional ratio the device size is rounded, so the logical size
    // read back is within one device pixel of the request.
    renderer.set_viewport_request(LOGICAL_W + 0.3, LOGICAL_H, 1.25);
    let (w, h) = renderer.get_viewport_logical_size();
    assert!((w - (LOGICAL_W + 0.3)).abs() <= 1.0 / 1.25, "{w}");
    assert!((h - LOGICAL_H).abs() <= 1.0 / 1.25, "{h}");
    assert_eq!(renderer.get_viewport_size(), (400, 250));

    // Resolution off: the logical size is the target's size.
    renderer.set_render_quality(RenderQuality {
        full_display_resolution: false,
        ..RenderQuality::default()
    });
    renderer.set_viewport_request(LOGICAL_W, LOGICAL_H, 2.0);
    assert_eq!(renderer.get_viewport_logical_size(), (LOGICAL_W, LOGICAL_H));
    assert_eq!(
        renderer.get_viewport_size(),
        (VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
    );
}

/// D7: the CLI's sized capture (save the request, render at the requested
/// pixels at ratio 1, put the request back) renders exactly `w × h` and
/// leaves the live target at the device size, still multisampled.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_sized_capture_at_ratio_2_renders_its_size_and_restores_the_live_target() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_viewport_request(LOGICAL_W, LOGICAL_H, 2.0);
    upload_fixed_scene(&mut renderer);
    let before = renderer.render(BACKGROUND_RGB);
    let samples = renderer.live_sample_count();

    let (w, h) = (256u32, 192u32);
    let saved = renderer.viewport_request();
    renderer.set_viewport_request(w as f64, h as f64, 1.0);
    let capture = renderer.render_still(BACKGROUND_RGB);
    dump("hidpi_sized_capture", &capture);
    assert_eq!(capture.len(), (w * h * 4) as usize);
    assert!(count_non_background(&capture, BACKGROUND_RGB, true) > 1000);
    renderer.set_viewport_request(
        saved.logical_width,
        saved.logical_height,
        saved.device_pixel_ratio,
    );

    assert_eq!(renderer.viewport_request(), saved);
    assert_eq!(
        renderer.get_viewport_size(),
        (VIEWPORT_WIDTH * 2, VIEWPORT_HEIGHT * 2)
    );
    assert_eq!(renderer.live_sample_count(), samples);
    if renderer.msaa_supported() {
        assert_eq!(samples, LIVE_SAMPLE_COUNT);
    }
    let after = renderer.render(BACKGROUND_RGB);
    assert_eq!(before, after, "the sized capture disturbed the live view");
}

// ---- Depth perception, Phase 1 (doc/design_depth_perception.md) -----------
//
// The lit-pipelines scene (Testing layer 4): every lit pipeline, one image
// per pipeline and variant, so a later phase that changes one of them on
// purpose can exempt that image without losing the comparison on the rest.
// It exists for `ATOMCAD_GPU_COMPARE_DIR`: the shading refactor of Phase 1
// must leave every one of these images unchanged.

use atomcad_renderer::transparent_surface_mesh::SurfaceComponentRange;

const LIGHT_BACKGROUND: [u8; 3] = [235, 235, 235];
const DARK_BACKGROUND: [u8; 3] = [30, 30, 30];

/// The content of one lit-pipelines image.
struct LitScene {
    main: Mesh,
    atoms: AtomImpostorMesh,
    bonds: BondImpostorMesh,
    ghosts: TransparentImpostorMesh,
    surfaces: TransparentSurfaceMesh,
    gadget_atoms: AtomImpostorMesh,
}

impl LitScene {
    fn empty() -> Self {
        Self {
            main: Mesh::new(),
            atoms: AtomImpostorMesh::new(),
            bonds: BondImpostorMesh::new(),
            ghosts: TransparentImpostorMesh::new(),
            surfaces: TransparentSurfaceMesh::new(),
            gadget_atoms: AtomImpostorMesh::new(),
        }
    }

    fn upload(&self, renderer: &mut Renderer) {
        renderer.update_all_gpu_meshes(
            &Mesh::new(),
            &LineMesh::new(),
            &self.main,
            &LineMesh::new(),
            &LineMesh::new(),
            &self.atoms,
            &self.bonds,
            &self.ghosts,
            &self.surfaces,
            &LabelMesh::new(),
            &self.gadget_atoms,
            &BondImpostorMesh::new(),
            true,
            Some((1, "Main".to_string())),
        );
    }
}

/// An axis-aligned cube of half-size `half` around the origin, one flat
/// normal per face, every vertex at opacity `alpha`.
fn cube(half: f32, albedo: Vec3, alpha: f32) -> Mesh {
    let mut mesh = Mesh::new();
    let material = Material::new(&albedo, 0.5, 0.0);
    for normal in [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ] {
        // `u × v = normal`, so the corners run counter-clockwise seen from
        // outside, the front face the triangle pipeline keeps.
        let u = normal.any_orthonormal_vector();
        let v = normal.cross(u);
        let centre = normal * half;
        let corners = [
            centre + (-u - v) * half,
            centre + (u - v) * half,
            centre + (u + v) * half,
            centre + (-u + v) * half,
        ];
        let first = mesh.vertices.len() as u32;
        for corner in &corners {
            mesh.add_vertex(Vertex::new_translucent(corner, &normal, &material, alpha));
        }
        mesh.add_quad(first, first + 1, first + 2, first + 3);
    }
    mesh
}

/// Every lit pipeline, by name, each alone in its scene: the opaque
/// triangle mesh, the atom impostor (plain, metallic, selected and frozen,
/// the last two with their rims), the bond impostor, the transparent atom
/// and bond impostors, the transparent isosurface and a gadget-pass atom.
fn lit_pipeline_scenes() -> Vec<(&'static str, LitScene)> {
    let atom = |roughness: f32, metallic: f32, rim: [f32; 4]| {
        let mut atoms = AtomImpostorMesh::new();
        atoms.add_atom_quad(
            &Vec3::ZERO,
            1.5,
            &[0.2, 0.5, 0.9],
            roughness,
            metallic,
            &rim,
        );
        LitScene {
            atoms,
            ..LitScene::empty()
        }
    };
    let mut scenes = vec![
        (
            "mesh",
            LitScene {
                main: cube(1.2, Vec3::new(0.8, 0.3, 0.2), 1.0),
                ..LitScene::empty()
            },
        ),
        ("atom", atom(0.5, 0.0, [0.0; 4])),
        ("atom_metallic", atom(0.3, 1.0, [0.0; 4])),
        ("atom_selected", atom(0.5, 0.0, [1.0, 0.5, 0.0, 1.0])),
        ("atom_frozen", atom(0.5, 0.0, [0.0, 0.8, 1.0, 1.0])),
    ];

    let bond_start = Vec3::new(-2.0, -0.5, 0.0);
    let bond_end = Vec3::new(2.0, 0.5, 0.0);
    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_with_positions(&bond_start, &bond_end, 0.5, &[0.7, 0.7, 0.7]);
    scenes.push((
        "bond",
        LitScene {
            bonds,
            ..LitScene::empty()
        },
    ));

    let ghost_colour = [0.9, 0.6, 0.2];
    let mut ghost_atom = TransparentImpostorMesh::new();
    ghost_atom.add_atom_quad(&Vec3::ZERO, 1.5, &ghost_colour, 0.5, 0.0, &[0.0; 4], 0.5);
    scenes.push((
        "ghost_atom",
        LitScene {
            ghosts: ghost_atom,
            ..LitScene::empty()
        },
    ));

    let mut ghost_bond = TransparentImpostorMesh::new();
    ghost_bond.add_bond_quad(&bond_start, &bond_end, 0.5, &ghost_colour, 0.5);
    scenes.push((
        "ghost_bond",
        LitScene {
            ghosts: ghost_bond,
            ..LitScene::empty()
        },
    ));

    let surface = cube(1.2, Vec3::new(0.3, 0.8, 0.4), 0.5);
    let index_count = surface.indices.len() as u32;
    scenes.push((
        "surface",
        LitScene {
            surfaces: TransparentSurfaceMesh {
                mesh: surface,
                components: vec![SurfaceComponentRange {
                    first_index: 0,
                    index_count,
                    centroid: Vec3::ZERO,
                }],
            },
            ..LitScene::empty()
        },
    ));

    let mut gadget_atoms = AtomImpostorMesh::new();
    gadget_atoms.add_atom_quad(&Vec3::ZERO, 1.0, &[0.9, 0.9, 0.2], 0.5, 0.0, &[0.0; 4]);
    scenes.push((
        "gadget_atom",
        LitScene {
            gadget_atoms,
            ..LitScene::empty()
        },
    ));
    scenes
}

/// Every lit pipeline, in perspective and orthographic, with MSAA off, on,
/// and on with the refined frame's per-sample entry points, on a light and a
/// dark background: one dumped image each, named
/// `lit_<pipeline>_<projection>_<sampling>_<background>`. Variants the
/// adapter cannot draw are skipped. Each image must show its content.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn lit_pipelines_scene() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let mut images = 0;
    for (pipeline, scene) in lit_pipeline_scenes() {
        scene.upload(&mut renderer);
        for orthographic in [false, true] {
            renderer.set_orthographic_mode(orthographic);
            renderer.set_ortho_half_height(3.5);
            // Oblique, so the cube shows three faces and every sphere a range
            // of normals.
            renderer.move_camera(
                &DVec3::new(5.0, -10.0, 8.0),
                &DVec3::ZERO,
                &DVec3::new(0.0, 0.0, 1.0),
            );
            let projection = if orthographic { "ortho" } else { "persp" };
            for sampling in ["msaa_off", "msaa_on", "refined"] {
                let anti_aliasing = sampling != "msaa_off";
                if anti_aliasing && !renderer.msaa_supported()
                    || sampling == "refined" && !renderer.refinement_supported()
                {
                    continue;
                }
                renderer.set_render_quality(RenderQuality {
                    anti_aliasing,
                    full_display_resolution: true,
                });
                for (background_name, background) in
                    [("light", LIGHT_BACKGROUND), ("dark", DARK_BACKGROUND)]
                {
                    let pixels = if sampling == "refined" {
                        renderer.render_still(background)
                    } else {
                        renderer.render(background)
                    };
                    let name = format!("lit_{pipeline}_{projection}_{sampling}_{background_name}");
                    dump(&name, &pixels);
                    let lit = count_non_background(&pixels, background, true);
                    assert!(lit > 500, "{name}: (almost) all background: {lit}");
                    images += 1;
                }
            }
        }
    }
    println!("renderer_gpu: lit-pipelines scene rendered {images} images");
}

// ---- Depth perception, Phase 2: the studio rig and the rim ----------------
//
// `doc/design_depth_perception.md` D3, D4. Expected colours come from the
// Rust twin (`shading.rs`); the twin-agreement test is what ties it to
// `lighting.wgsl`.

use atomcad_renderer::shading::{RIM_WIDTH_PX, light_surface, to_display, view_vector};
use atomcad_renderer::shading_uniform::{
    LightingPreset, ShadingPass, ShadingSettings, ShadingUniform,
};

fn shading_of(preset: LightingPreset) -> ShadingSettings {
    ShadingSettings {
        preset,
        ..ShadingSettings::default()
    }
}

/// Looking straight down -Z at the origin, +Y up, at 10 render pixels per
/// world unit in a viewport `height` pixels high; perspective from
/// `distance` away, or orthographic.
fn look_down(renderer: &mut Renderer, orthographic: bool, distance: f64, height: u32) {
    renderer.set_orthographic_mode(orthographic);
    renderer.set_ortho_half_height(height as f64 / 20.0);
    renderer.move_camera(
        &DVec3::new(0.0, 0.0, distance),
        &DVec3::ZERO,
        &DVec3::new(0.0, 1.0, 0.0),
    );
}

/// The view ray through the centre of pixel `(column, row)` of a
/// `width × height` target: origin and unit direction, world space.
fn ray_through_pixel(
    camera: &atomcad_renderer::camera::Camera,
    width: u32,
    height: u32,
    column: u32,
    row: u32,
) -> (DVec3, DVec3) {
    let x = (column as f64 + 0.5) / width as f64 * 2.0 - 1.0;
    let y = 1.0 - (row as f64 + 0.5) / height as f64 * 2.0;
    let inverse = camera.build_view_projection_matrix().inverse();
    let near = inverse.project_point3(DVec3::new(x, y, 0.1));
    let far = inverse.project_point3(DVec3::new(x, y, 0.9));
    (near, (far - near).normalize())
}

/// Where a ray meets the sphere `(centre, radius)` first, if it does.
fn ray_sphere(origin: DVec3, dir: DVec3, centre: DVec3, radius: f64) -> Option<DVec3> {
    let oc = centre - origin;
    let tca = oc.dot(dir);
    let d2 = oc.length_squared() - tca * tca;
    let r2 = radius * radius;
    (d2 <= r2).then(|| origin + dir * (tca - (r2 - d2).sqrt()))
}

/// D1, Testing layer 3: under each preset, in both projections, pixels at
/// known normals of a lit sphere (its centre, and four points about 60° from
/// the view axis) are what the Rust twin computes, within 2/255. Mutation:
/// change one constant in `shading.rs` only (the hemisphere's `0.5 + 0.5·`
/// to `0.5 + 0.4·` was tried); this test fails.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_twin_agrees_with_the_gpu_on_a_lit_sphere() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    let (radius, albedo) = (8.0f32, [0.2f32, 0.5, 0.9]);
    upload_impostors(
        &mut renderer,
        &atom(Vec3::ZERO, radius, albedo),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    let mut worst = 0u8;
    for preset in [LightingPreset::Headlight, LightingPreset::Studio] {
        renderer.set_shading(shading_of(preset));
        for orthographic in [false, true] {
            look_down(&mut renderer, orthographic, 60.0, VIEWPORT_HEIGHT);
            let pixels = renderer.render(DARK_BACKGROUND);
            dump(
                &format!("twin_{preset:?}_{orthographic}").to_lowercase(),
                &pixels,
            );
            let camera = renderer.camera.clone();
            let uniform = ShadingUniform::new(
                &camera,
                renderer.content_bounds(),
                DARK_BACKGROUND,
                &shading_of(preset),
                ShadingPass::Content,
                None,
            );
            // The pixels over the sphere's centre and over four points 60°
            // from the view axis: up, down, left and right. The expectation
            // is computed for the pixel's own centre, so landing a little off
            // those points does not matter.
            let view_proj = camera.build_view_projection_matrix();
            let pixel_over = |normal: DVec3| {
                let ndc = view_proj.project_point3(normal * radius as f64);
                (
                    ((ndc.x * 0.5 + 0.5) * VIEWPORT_WIDTH as f64) as u32,
                    ((0.5 - ndc.y * 0.5) * VIEWPORT_HEIGHT as f64) as u32,
                )
            };
            let (s, c) = 60f64.to_radians().sin_cos();
            for (column, row) in [
                DVec3::Z,
                DVec3::new(0.0, s, c),
                DVec3::new(0.0, -s, c),
                DVec3::new(-s, 0.0, c),
                DVec3::new(s, 0.0, c),
            ]
            .map(pixel_over)
            {
                let (origin, dir) =
                    ray_through_pixel(&camera, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, column, row);
                let hit = ray_sphere(origin, dir, DVec3::ZERO, radius as f64)
                    .expect("the sample pixel is on the sphere");
                let normal = hit.normalize().as_vec3();
                let expected = to_display(light_surface(
                    &uniform,
                    view_vector(&camera, hit),
                    normal,
                    Vec3::from(albedo),
                    0.5,
                    0.0,
                )) * 255.0;
                let got = rgba_at(&pixels, VIEWPORT_WIDTH, column, row);
                for c in 0..3 {
                    let diff = (got[c] as f32 - expected[c]).abs();
                    worst = worst.max(diff.round() as u8);
                    assert!(
                        diff <= 2.0,
                        "{preset:?}, orthographic {orthographic}, pixel ({column}, {row}), \
                         normal {normal}: GPU {got:?}, twin {expected}"
                    );
                }
            }
        }
    }
    println!("renderer_gpu: twin agreement, largest difference {worst}/255");
}

/// The mean luminance (0–255) of the pixels of a live (BGRA) render inside
/// the disc `(cx, cy, r)` (pixels) that `keep(dx, dy)` selects.
fn mean_in_disc(
    pixels: &[u8],
    width: u32,
    (cx, cy, r): (f32, f32, f32),
    keep: impl Fn(f32, f32) -> bool,
) -> f32 {
    let (mut sum, mut n) = (0.0, 0);
    for (i, p) in pixels.chunks_exact(4).enumerate() {
        let dx = (i as u32 % width) as f32 + 0.5 - cx;
        let dy = (i as u32 / width) as f32 + 0.5 - cy;
        if dx * dx + dy * dy < r * r && keep(dx, dy) {
            sum += 0.0722 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.2126 * p[2] as f32;
            n += 1;
        }
    }
    sum / n as f32
}

/// D3: the *Studio* key comes from the upper left, so a lit sphere is
/// brighter in its upper-left quadrant than in its lower-right one. Under
/// *Headlight* the two halves left and right of the centre are lit alike.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_sphere_under_studio_is_brighter_upper_left_than_lower_right() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_impostors(
        &mut renderer,
        &atom(Vec3::ZERO, 8.0, [0.7, 0.7, 0.7]),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let disc = (
        VIEWPORT_WIDTH as f32 / 2.0,
        VIEWPORT_HEIGHT as f32 / 2.0,
        78.0,
    );
    // Screen y grows downwards.
    let upper_left = |dx: f32, dy: f32| dx < 0.0 && dy < 0.0;
    let lower_right = |dx: f32, dy: f32| dx > 0.0 && dy > 0.0;

    renderer.set_shading(shading_of(LightingPreset::Studio));
    let studio = renderer.render(DARK_BACKGROUND);
    dump("studio_quadrants", &studio);
    let (ul, lr) = (
        mean_in_disc(&studio, VIEWPORT_WIDTH, disc, upper_left),
        mean_in_disc(&studio, VIEWPORT_WIDTH, disc, lower_right),
    );
    println!("renderer_gpu: Studio upper left {ul}, lower right {lr}");
    assert!(ul > lr + 20.0, "upper left {ul}, lower right {lr}");

    // The headlight is straight above the view axis: left and right alike.
    renderer.set_shading(shading_of(LightingPreset::Headlight));
    let headlight = renderer.render(DARK_BACKGROUND);
    let left = mean_in_disc(&headlight, VIEWPORT_WIDTH, disc, |dx, _| dx < 0.0);
    let right = mean_in_disc(&headlight, VIEWPORT_WIDTH, disc, |dx, _| dx > 0.0);
    assert!((left - right).abs() < 1.0, "left {left}, right {right}");
}

/// The rim test sphere's colours: a green albedo and a magenta rim, far apart
/// in every channel the weight is read from.
const RIM_ALBEDO: [f32; 3] = [0.2, 0.8, 0.2];
const RIM_COLOUR: [f32; 4] = [1.0, 0.0, 1.0, 1.0];

fn rim_atom(radius: f32, rim: [f32; 4]) -> AtomImpostorMesh {
    let mut atoms = AtomImpostorMesh::new();
    atoms.add_atom_quad(&Vec3::ZERO, radius, &RIM_ALBEDO, 0.5, 0.0, &rim);
    atoms
}

/// Renders a sphere of `radius` with and without the rim, live or exported
/// (`export`, through `render_image` at the viewport's size), and returns
/// each output pixel's rim weight: how far it moved from the plain colour
/// towards the rim colour, in the channel where those two are furthest
/// apart. 0 off the sphere.
fn rim_weights(renderer: &mut Renderer, radius: f32, export: bool, name: &str) -> Vec<f32> {
    let mut render = |rim: [f32; 4]| {
        upload_impostors(
            renderer,
            &rim_atom(radius, rim),
            &BondImpostorMesh::new(),
            &TransparentImpostorMesh::new(),
            &AtomImpostorMesh::new(),
        );
        if export {
            let (width, height) = renderer.get_viewport_size();
            renderer.render_image(width, height, BLACK)
        } else {
            // BGRA to RGBA, as the export is.
            let mut pixels = renderer.render(BLACK);
            for p in pixels.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
            pixels
        }
    };
    let plain = render([0.0; 4]);
    let rimmed = render(RIM_COLOUR);
    dump(&format!("rim_{name}_plain"), &plain);
    dump(&format!("rim_{name}"), &rimmed);
    plain
        .chunks_exact(4)
        .zip(rimmed.chunks_exact(4))
        .map(|(p, r)| {
            (0..3)
                .map(|c| {
                    let span = RIM_COLOUR[c] * 255.0 - p[c] as f32;
                    (span.abs(), (r[c] as f32 - p[c] as f32) / span)
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .filter(|(span, _)| *span > 40.0)
                .map_or(0.0, |(_, weight)| weight.clamp(0.0, 1.0))
        })
        .collect()
}

/// D4: the rim is a ring `RIM_WIDTH_PX` pixels wide along the silhouette,
/// whatever the sphere's size: its summed weight on a 200-px and on a 40-px
/// sphere (diameters) is the ring's area within 15 %. The old band
/// (everything beyond ~35° from the view axis) covers about 11× that on the
/// large sphere and 2.4× on the small one: mutation.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_rim_is_a_ring_of_fixed_pixel_width_on_any_sphere() {
    let (width, height) = (320, 240);
    let Some(mut renderer) = try_renderer(width, height) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, height);
    for radius_px in [100.0f32, 20.0] {
        // 10 render pixels per world unit.
        let name = format!("ring_{radius_px}");
        let weights = rim_weights(&mut renderer, radius_px / 10.0, false, &name);
        let area: f32 = weights.iter().sum();
        let w = RIM_WIDTH_PX.min(0.35 * radius_px);
        let ring = std::f32::consts::PI * (radius_px.powi(2) - (radius_px - w).powi(2));
        println!(
            "renderer_gpu: rim on a {radius_px}-px-radius sphere: {area} px², ring {ring} px²"
        );
        assert!(
            (area / ring - 1.0).abs() < 0.15,
            "{radius_px}-px-radius sphere: rim area {area}, ring {ring}"
        );
    }
}

/// D4: the rim is mixed in after lighting, so it is the rim colour all
/// around the outline: on the key side and the fill side of a sphere under
/// *Studio* alike, within 2/255. Lighting the rim (mixing it into the albedo,
/// as before) fails this: mutation.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_rim_is_the_same_colour_on_the_key_and_the_fill_side() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    renderer.set_shading(shading_of(LightingPreset::Studio));
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let radius_px = 80.0f32;
    upload_impostors(
        &mut renderer,
        &rim_atom(radius_px / 10.0, RIM_COLOUR),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    let pixels = renderer.render(BLACK);
    dump("rim_key_fill", &pixels);
    // 1.5 px inside the silhouette, upper left (key) and lower right (fill).
    let at = |angle_deg: f32| {
        let (s, c) = angle_deg.to_radians().sin_cos();
        let r = radius_px - 1.5;
        let column = (VIEWPORT_WIDTH as f32 / 2.0 + c * r) as u32;
        let row = (VIEWPORT_HEIGHT as f32 / 2.0 - s * r) as u32;
        rgba_at(&pixels, VIEWPORT_WIDTH, column, row)
    };
    let (key_side, fill_side) = (at(135.0), at(-45.0));
    for c in 0..3 {
        assert!(
            key_side[c].abs_diff(fill_side[c]) <= 2,
            "key side {key_side:?}, fill side {fill_side:?}"
        );
        let rim = (RIM_COLOUR[c] * 255.0) as u8;
        assert!(key_side[c].abs_diff(rim) <= 2, "key side {key_side:?}");
    }
}

/// D4: the rim is as wide in output pixels in a 2× supersampled export as in
/// the live view, within one pixel, measured as the summed rim weight along
/// the row through the sphere's centre (both sides, halved). Without the
/// target's `pixel_scale` in the width the export's rim would be half as
/// wide: mutation.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_rim_is_as_wide_exported_as_live() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    assert_eq!(IMAGE_EXPORT_SUPERSAMPLING, 2);
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let row = (VIEWPORT_HEIGHT / 2) as usize;
    let row_width = |weights: &[f32]| {
        let start = row * VIEWPORT_WIDTH as usize;
        weights[start..start + VIEWPORT_WIDTH as usize]
            .iter()
            .sum::<f32>()
            / 2.0
    };
    let live = row_width(&rim_weights(&mut renderer, 6.0, false, "width_live"));
    let export = row_width(&rim_weights(&mut renderer, 6.0, true, "width_export"));
    println!("renderer_gpu: rim width live {live} px, exported {export} px");
    assert!((live - RIM_WIDTH_PX).abs() <= 1.0, "live rim {live} px");
    assert!(
        (live - export).abs() <= 1.0,
        "live rim {live} px, exported {export} px"
    );
}

/// D1: `mesh.wgsl` builds `V` with the shared `camera_view_vector`, so in
/// orthographic a flat quad facing the viewer is lit the same across its
/// width. The old per-pixel `V` (towards the eye position) shaded it
/// unevenly, the more so the closer the eye: mutation.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn an_orthographic_flat_quad_is_evenly_lit() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload(
        &mut renderer,
        &square(8.0, 0.0, Vec3::new(0.8, 0.3, 0.2)),
        &LineMesh::new(),
        &Mesh::new(),
        &LineMesh::new(),
    );
    // The eye close to the quad: that changes nothing in orthographic, but
    // would swing an eye-position `V` by ~40° across it.
    look_down(&mut renderer, true, 10.0, VIEWPORT_HEIGHT);
    let pixels = renderer.render(BLACK);
    dump("ortho_flat_quad", &pixels);
    let row = VIEWPORT_HEIGHT / 2;
    let samples: Vec<[u8; 4]> = (VIEWPORT_WIDTH / 2 - 75..VIEWPORT_WIDTH / 2 + 75)
        .step_by(5)
        .map(|column| rgba_at(&pixels, VIEWPORT_WIDTH, column, row))
        .collect();
    for c in 0..3 {
        let (lo, hi) = samples
            .iter()
            .fold((255u8, 0u8), |(lo, hi), p| (lo.min(p[c]), hi.max(p[c])));
        assert!(hi - lo <= 1, "channel {c}: {lo}..{hi} across the quad");
    }
}

// ---- Depth cue (doc/design_depth_perception.md D5, Phase 3) ----------------

use atomcad_renderer::camera::Camera;
use atomcad_renderer::shading::apply_depth_cue;
use atomcad_renderer::thumbnail::ThumbnailBackground;

const FOG_ALBEDO: [f32; 3] = [0.2, 0.5, 0.9];
const FOG_RADIUS: f32 = 3.0;
/// Two identical spheres, one at the front of the content and one 30 Å
/// behind it, side by side as seen from above.
const FOG_FRONT: Vec3 = Vec3::new(-7.0, 0.0, 0.0);
const FOG_BACK: Vec3 = Vec3::new(7.0, 0.0, -30.0);

fn depth_cue_of(strength: f32) -> ShadingSettings {
    ShadingSettings {
        depth_cue_strength: strength,
        ..ShadingSettings::default()
    }
}

/// The two spheres of the fog scene, each with the rim colour `rim`.
fn fog_pair(albedo: [f32; 3], rim: [f32; 4]) -> AtomImpostorMesh {
    let mut atoms = AtomImpostorMesh::new();
    for centre in [FOG_FRONT, FOG_BACK] {
        atoms.add_atom_quad(&centre, FOG_RADIUS, &albedo, 0.5, 0.0, &rim);
    }
    atoms
}

fn upload_fog_pair(renderer: &mut Renderer, albedo: [f32; 3], rim: [f32; 4]) {
    upload_impostors(
        renderer,
        &fog_pair(albedo, rim),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
}

/// The pixel of a `width × height` render of `camera` over the world point
/// `point`.
fn pixel_over(camera: &Camera, width: u32, height: u32, point: DVec3) -> (u32, u32) {
    let ndc = camera.build_view_projection_matrix().project_point3(point);
    (
        ((ndc.x * 0.5 + 0.5) * width as f64) as u32,
        ((0.5 - ndc.y * 0.5) * height as f64) as u32,
    )
}

/// What the Rust twin says the pixel `(column, row)` over a fog-scene sphere
/// at `centre` is, 0–255, under `uniform`: lit, mapped to the display, then
/// fogged at the point the pixel's ray meets the sphere.
fn twin_pixel(
    camera: &Camera,
    uniform: &ShadingUniform,
    (width, height): (u32, u32),
    (column, row): (u32, u32),
    centre: Vec3,
    albedo: [f32; 3],
) -> Vec3 {
    let (origin, dir) = ray_through_pixel(camera, width, height, column, row);
    let hit = ray_sphere(origin, dir, centre.as_dvec3(), FOG_RADIUS as f64)
        .expect("the sample pixel is on the sphere");
    let normal = (hit - centre.as_dvec3()).normalize().as_vec3();
    let lit = to_display(light_surface(
        uniform,
        view_vector(camera, hit),
        normal,
        Vec3::from(albedo),
        0.5,
        0.0,
    ));
    apply_depth_cue(uniform, camera, lit, hit) * 255.0
}

fn assert_near(got: [u8; 4], expected: Vec3, tolerance: f32, what: &str) {
    for c in 0..3 {
        assert!(
            (got[c] as f32 - expected[c]).abs() <= tolerance,
            "{what}: GPU {got:?}, expected {expected}"
        );
    }
}

fn rgb_of(pixel: [u8; 3]) -> Vec3 {
    Vec3::new(pixel[0] as f32, pixel[1] as f32, pixel[2] as f32)
}

/// How far a pixel is from a background colour: the sum over the channels.
fn distance_to(pixel: [u8; 4], background: [u8; 3]) -> i32 {
    (0..3)
        .map(|c| (pixel[c] as i32 - background[c] as i32).abs())
        .sum()
}

/// D5, and the twin agreement of Testing layer 3 extended to the cue: of two
/// identical spheres, the one at the back is measurably closer to the
/// background, on a light and on a dark background, in both projections;
/// the one at the front is the cue-off pixel. Both centre pixels are what
/// the twin's `apply_depth_cue` computes, within 2/255.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_back_sphere_is_fogged_towards_the_background_and_the_front_one_is_not() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    upload_fog_pair(&mut renderer, FOG_ALBEDO, [0.0; 4]);
    let cue = depth_cue_of(0.5);
    let mut worst = 0.0f32;
    for orthographic in [false, true] {
        look_down(&mut renderer, orthographic, 60.0, VIEWPORT_HEIGHT);
        let camera = renderer.camera.clone();
        for background in [LIGHT_BACKGROUND, DARK_BACKGROUND] {
            renderer.set_shading(ShadingSettings::default());
            let off = renderer.render(background);
            renderer.set_shading(cue);
            let on = renderer.render(background);
            let name = format!("fog_pair_{orthographic}_{}", background[0]);
            dump(&format!("{name}_off"), &off);
            dump(&name, &on);
            let uniform = ShadingUniform::new(
                &camera,
                renderer.content_bounds(),
                background,
                &cue,
                ShadingPass::Content,
                None,
            );
            for centre in [FOG_FRONT, FOG_BACK] {
                let pixel = pixel_over(&camera, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, centre.as_dvec3());
                let expected = twin_pixel(
                    &camera,
                    &uniform,
                    (VIEWPORT_WIDTH, VIEWPORT_HEIGHT),
                    pixel,
                    centre,
                    FOG_ALBEDO,
                );
                let got = rgba_at(&on, VIEWPORT_WIDTH, pixel.0, pixel.1);
                let without = rgba_at(&off, VIEWPORT_WIDTH, pixel.0, pixel.1);
                let what = format!("{name}, sphere at {centre}");
                assert_near(got, expected, 2.0, &what);
                for c in 0..3 {
                    worst = worst.max((got[c] as f32 - expected[c]).abs());
                }
                if centre == FOG_FRONT {
                    let plain = rgb_of([without[0], without[1], without[2]]);
                    assert_near(got, plain, 1.0, &what);
                } else {
                    let (fogged, plain) = (
                        distance_to(got, background),
                        distance_to(without, background),
                    );
                    assert!(
                        fogged + 30 < plain,
                        "{what}: {fogged} from the background, {plain} without the cue"
                    );
                }
            }
        }
    }
    println!("renderer_gpu: depth-cue twin agreement, largest difference {worst}/255");
}

/// D2, D5: a thumbnail draws the view twice in a row, on a light and on a
/// dark background, and keeps one. The kept image is fogged towards its own
/// background, which is what writing the `ShadingUniform` before every draw
/// guarantees. A bright and a dark scene make the thumbnail keep each
/// background once. Mutation: building the uniform with a fixed background.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_thumbnail_is_fogged_towards_its_own_background() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let cue = depth_cue_of(1.0);
    renderer.set_shading(cue);
    look_down(&mut renderer, false, 60.0, VIEWPORT_HEIGHT);
    let mut kept = Vec::new();
    for (scene, albedo) in [("bright", [0.95, 0.95, 0.95]), ("dark", [0.05, 0.05, 0.08])] {
        upload_fog_pair(&mut renderer, albedo, [0.0; 4]);
        let bounds = renderer.content_bounds().expect("content has bounds");
        let mut camera = fit_camera_to_bounds(&renderer.camera, bounds);
        camera.aspect = 1.0;
        let thumbnail = renderer.render_thumbnail(&camera);
        dump(&format!("fog_thumbnail_{scene}"), &thumbnail);
        let corner = [thumbnail[0], thumbnail[1], thumbnail[2]];
        let background = [ThumbnailBackground::Light, ThumbnailBackground::Dark]
            .into_iter()
            .find(|b| b.rgb() == corner)
            .expect("the corner is one of the two backgrounds");
        kept.push(background);
        let uniform = ShadingUniform::new(
            &camera,
            Some(bounds),
            background.rgb(),
            &cue,
            ShadingPass::Content,
            None,
        );
        let size = (THUMBNAIL_SIZE, THUMBNAIL_SIZE);
        let pixel = pixel_over(&camera, size.0, size.1, FOG_BACK.as_dvec3());
        let expected = twin_pixel(&camera, &uniform, size, pixel, FOG_BACK, albedo);
        let i = ((pixel.1 * THUMBNAIL_SIZE + pixel.0) * 4) as usize;
        let got = [thumbnail[i], thumbnail[i + 1], thumbnail[i + 2], 255];
        assert_near(
            got,
            expected,
            3.0,
            &format!("{scene} scene on the {background:?} background"),
        );
    }
    assert_ne!(kept[0], kept[1], "both scenes kept the same background");
}

/// D5: the cue mixes in display space towards the pass's clear colour, so at
/// strength 1 a sphere a span behind the front of the content is the
/// background pixel, within 2/255, on a light and on a dark background. A
/// linear-space mix converges on `to_display(background)` instead: mutation.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn at_full_strength_the_far_end_is_the_background() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    // Small spheres 200 Å apart along the view axis, so the span (the
    // bounding sphere's diameter) is barely more than their distance: the
    // back one's nearest point is 99.5 % of the way into it.
    let (front, back) = (Vec3::ZERO, Vec3::new(0.0, 2.0, -200.0));
    let mut atoms = AtomImpostorMesh::new();
    for centre in [front, back] {
        atoms.add_atom_quad(&centre, 0.5, &FOG_ALBEDO, 0.5, 0.0, &[0.0; 4]);
    }
    upload_impostors(
        &mut renderer,
        &atoms,
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    renderer.set_shading(depth_cue_of(1.0));
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    for background in [LIGHT_BACKGROUND, DARK_BACKGROUND] {
        let pixels = renderer.render(background);
        dump(&format!("fog_far_end_{}", background[0]), &pixels);
        let (column, row) = pixel_of(back.x, back.y);
        let got = rgba_at(&pixels, VIEWPORT_WIDTH, column, row);
        assert_near(got, rgb_of(background), 2.0, "the far sphere");
        let (column, row) = pixel_of(front.x, front.y);
        let near = rgba_at(&pixels, VIEWPORT_WIDTH, column, row);
        assert!(
            distance_to(near, background) > 60,
            "the near sphere {near:?} is lost in the background {background:?}"
        );
    }
}

/// D4, D5: the rim is mixed in after the cue, so a selected sphere at the
/// back of the content keeps its rim colour: within 2/255 of the same sphere
/// at the front, on the light background where fog would show most.
/// Fogging after `apply_rim` fails this: mutation.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_selected_sphere_at_the_back_keeps_its_rim_colour() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    upload_fog_pair(&mut renderer, FOG_ALBEDO, RIM_COLOUR);
    renderer.set_shading(depth_cue_of(1.0));
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let pixels = renderer.render(LIGHT_BACKGROUND);
    dump("fog_rim", &pixels);
    let radius_px = FOG_RADIUS * 10.0;
    let rim = Vec3::new(RIM_COLOUR[0], RIM_COLOUR[1], RIM_COLOUR[2]) * 255.0;
    for angle in [90.0f32, 200.0, 315.0] {
        // 1.5 px inside the silhouette, at the same angle on both spheres.
        let at = |centre: Vec3| {
            let (column, row) = pixel_of(centre.x, centre.y);
            let (s, c) = angle.to_radians().sin_cos();
            let r = radius_px - 1.5;
            let column = (column as f32 + c * r) as u32;
            let row = (row as f32 - s * r) as u32;
            rgba_at(&pixels, VIEWPORT_WIDTH, column, row)
        };
        let (front, back) = (at(FOG_FRONT), at(FOG_BACK));
        assert_near(front, rim, 2.0, &format!("front rim at {angle}°"));
        for c in 0..3 {
            assert!(
                front[c].abs_diff(back[c]) <= 2,
                "rim at {angle}°: front {front:?}, back {back:?}"
            );
        }
    }
}

/// D2, D5: the gadget pass has no depth cue. A guide dot as deep as the back
/// sphere is drawn exactly as with the cue off, while the back sphere is
/// fogged. Writing the content variant for the gadget pass fails this:
/// mutation.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_gadget_sphere_is_not_fogged() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    let gadget_centre = Vec3::new(0.0, 6.0, -30.0);
    upload_impostors(
        &mut renderer,
        &fog_pair(FOG_ALBEDO, [0.0; 4]),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &atom(gadget_centre, 2.0, [0.9, 0.6, 0.1]),
    );
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    renderer.set_shading(ShadingSettings::default());
    let off = renderer.render(LIGHT_BACKGROUND);
    renderer.set_shading(depth_cue_of(1.0));
    let on = renderer.render(LIGHT_BACKGROUND);
    dump("fog_gadget_off", &off);
    dump("fog_gadget", &on);
    let (column, row) = pixel_of(gadget_centre.x, gadget_centre.y);
    assert_eq!(
        rgba_at(&on, VIEWPORT_WIDTH, column, row),
        rgba_at(&off, VIEWPORT_WIDTH, column, row),
        "the guide dot is fogged"
    );
    let (column, row) = pixel_of(FOG_BACK.x, FOG_BACK.y);
    assert_ne!(
        rgba_at(&on, VIEWPORT_WIDTH, column, row),
        rgba_at(&off, VIEWPORT_WIDTH, column, row),
        "the back sphere is not fogged"
    );
}

// ---- Baked ambient occlusion (`doc/design_depth_perception.md` D7, D10,
// Phase 4). These set `ao_strength` with `set_shading`, since the renderer
// default is 0. Expected colours come from the twin, `light_surface_exposed`.

use atomcad_renderer::atom_impostor_mesh::FULLY_EXPOSED;
use atomcad_renderer::shading::{decode_exposure, light_surface_exposed};

const AO_ALBEDO: [f32; 3] = [0.7, 0.7, 0.7];
const AO_RADIUS: f32 = 5.0;

fn ao_of(preset: LightingPreset, strength: f32) -> ShadingSettings {
    ShadingSettings {
        preset,
        ao_strength: strength,
        ..ShadingSettings::default()
    }
}

/// Spheres of `AO_RADIUS` at the given centres, each with its packed
/// exposure.
fn exposed_atoms(spheres: &[(Vec3, [i8; 4])]) -> AtomImpostorMesh {
    let mut atoms = AtomImpostorMesh::new();
    for (centre, exposure) in spheres {
        atoms.add_atom_quad_with_exposure(
            centre, AO_RADIUS, &AO_ALBEDO, 0.5, 0.0, &[0.0; 4], *exposure,
        );
    }
    atoms
}

/// The pixel over the point of the sphere at `centre` whose normal is
/// `normal`, and what the twin computes there, 0–255.
fn ao_twin_pixel(
    renderer: &Renderer,
    uniform: &ShadingUniform,
    centre: Vec3,
    exposure: [i8; 4],
    normal: DVec3,
) -> ((u32, u32), Vec3) {
    let camera = &renderer.camera;
    let (column, row) = pixel_over(
        camera,
        VIEWPORT_WIDTH,
        VIEWPORT_HEIGHT,
        centre.as_dvec3() + normal * AO_RADIUS as f64,
    );
    let (origin, dir) = ray_through_pixel(camera, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, column, row);
    let hit = ray_sphere(origin, dir, centre.as_dvec3(), AO_RADIUS as f64)
        .expect("the sample pixel is on the sphere");
    let n = (hit - centre.as_dvec3()).normalize().as_vec3();
    let lit = to_display(light_surface_exposed(
        uniform,
        view_vector(camera, hit),
        n,
        Vec3::from(AO_ALBEDO),
        0.5,
        0.0,
        decode_exposure(exposure),
        1.0,
    ));
    ((column, row), lit * 255.0)
}

fn ao_uniform(renderer: &Renderer, settings: &ShadingSettings) -> ShadingUniform {
    ShadingUniform::new(
        &renderer.camera,
        renderer.content_bounds(),
        DARK_BACKGROUND,
        settings,
        ShadingPass::Content,
        None,
    )
}

fn luminance_of(p: [u8; 4]) -> f32 {
    0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32
}

/// D7, and the twin agreement of Testing layer 3 extended to the AO mapping:
/// of two spheres with exposure 0 and 1, the buried one is darker, and both
/// are what the twin computes at their centre and at four points 60° from
/// the view axis, within 2/255, under both presets and projections. At
/// `ao_strength` 0 the two are drawn alike.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_buried_sphere_is_darker_by_the_mapping_and_alike_at_strength_0() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    let open = (Vec3::new(-7.0, 0.0, 0.0), FULLY_EXPOSED);
    let buried = (Vec3::new(7.0, 0.0, 0.0), [0, 0, 0, 0]);
    upload_impostors(
        &mut renderer,
        &exposed_atoms(&[open, buried]),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    let (s, c) = 60f64.to_radians().sin_cos();
    let normals = [
        DVec3::Z,
        DVec3::new(0.0, s, c),
        DVec3::new(0.0, -s, c),
        DVec3::new(-s, 0.0, c),
        DVec3::new(s, 0.0, c),
    ];
    let mut worst = 0.0f32;
    for preset in [LightingPreset::Headlight, LightingPreset::Studio] {
        for orthographic in [false, true] {
            look_down(&mut renderer, orthographic, 60.0, VIEWPORT_HEIGHT);
            let settings = ao_of(preset, 1.0);
            renderer.set_shading(settings);
            let pixels = renderer.render(DARK_BACKGROUND);
            dump(
                &format!("ao_pair_{preset:?}_{orthographic}").to_lowercase(),
                &pixels,
            );
            let uniform = ao_uniform(&renderer, &settings);
            for (centre, exposure) in [open, buried] {
                for normal in normals {
                    let ((column, row), expected) =
                        ao_twin_pixel(&renderer, &uniform, centre, exposure, normal);
                    let got = rgba_at(&pixels, VIEWPORT_WIDTH, column, row);
                    for ch in 0..3 {
                        worst = worst.max((got[ch] as f32 - expected[ch]).abs());
                    }
                    assert_near(
                        got,
                        expected,
                        2.0,
                        &format!("{preset:?}, ortho {orthographic}, {exposure:?}, normal {normal}"),
                    );
                }
            }
            let centre_luminance = |centre: Vec3| {
                let (column, row) = pixel_over(
                    &renderer.camera,
                    VIEWPORT_WIDTH,
                    VIEWPORT_HEIGHT,
                    centre.as_dvec3() + DVec3::Z * AO_RADIUS as f64,
                );
                luminance_of(rgba_at(&pixels, VIEWPORT_WIDTH, column, row))
            };
            let (dark, bright) = (centre_luminance(buried.0), centre_luminance(open.0));
            assert!(dark < bright - 20.0, "buried {dark}, open {bright}");
        }
    }
    println!("renderer_gpu: AO twin agreement, largest difference {worst}/255");

    // Strength 0: the buried sphere is drawn like the open one.
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    renderer.set_shading(ao_of(LightingPreset::Studio, 0.0));
    let pixels = renderer.render(DARK_BACKGROUND);
    dump("ao_pair_strength_0", &pixels);
    let (left, _) = pixel_of(open.0.x, 0.0);
    let (right, _) = pixel_of(buried.0.x, 0.0);
    let half = (AO_RADIUS * 10.0) as u32 + 2;
    let mut differing = 0;
    for row in VIEWPORT_HEIGHT / 2 - half..VIEWPORT_HEIGHT / 2 + half {
        for dx in 0..2 * half {
            let a = rgba_at(&pixels, VIEWPORT_WIDTH, left - half + dx, row);
            let b = rgba_at(&pixels, VIEWPORT_WIDTH, right - half + dx, row);
            if (0..3).any(|ch| a[ch].abs_diff(b[ch]) > 1) {
                differing += 1;
            }
        }
    }
    assert_eq!(differing, 0, "the two spheres differ at strength 0");
}

/// D10: a sphere with `E = ½` and its bent vector pointing to screen-right is
/// brighter on its right half than on its left, as the twin says at 60° to
/// either side. With `b = 0` the two halves are alike. Dropping the
/// `dot(b, N)` term from `ao_visibility` fails the first assertion
/// (mutation).
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_bent_vector_lights_the_side_it_points_to() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    // The headlight is symmetric left to right, so only `b` tells the halves
    // apart.
    let settings = ao_of(LightingPreset::Headlight, 1.0);
    renderer.set_shading(settings);
    let disc = (
        VIEWPORT_WIDTH as f32 / 2.0,
        VIEWPORT_HEIGHT as f32 / 2.0,
        AO_RADIUS * 10.0 - 2.0,
    );
    let (s, c) = 60f64.to_radians().sin_cos();
    for (exposure, name) in [
        ([127, 0, 0, 64], "ao_bent_right"),
        ([0, 0, 0, 64], "ao_bent_none"),
    ] {
        upload_impostors(
            &mut renderer,
            &exposed_atoms(&[(Vec3::ZERO, exposure)]),
            &BondImpostorMesh::new(),
            &TransparentImpostorMesh::new(),
            &AtomImpostorMesh::new(),
        );
        let pixels = renderer.render(DARK_BACKGROUND);
        dump(name, &pixels);
        let left = mean_in_disc(&pixels, VIEWPORT_WIDTH, disc, |dx, _| dx < 0.0);
        let right = mean_in_disc(&pixels, VIEWPORT_WIDTH, disc, |dx, _| dx > 0.0);
        println!("renderer_gpu: {name}: left {left}, right {right}");
        if exposure[0] == 0 {
            assert!((left - right).abs() < 1.0, "left {left}, right {right}");
            continue;
        }
        assert!(right > left + 10.0, "left {left}, right {right}");
        let uniform = ao_uniform(&renderer, &settings);
        for normal in [DVec3::new(-s, 0.0, c), DVec3::new(s, 0.0, c), DVec3::Z] {
            let ((column, row), expected) =
                ao_twin_pixel(&renderer, &uniform, Vec3::ZERO, exposure, normal);
            assert_near(
                rgba_at(&pixels, VIEWPORT_WIDTH, column, row),
                expected,
                2.0,
                &format!("normal {normal}"),
            );
        }
    }
}

/// D7: a bond whose start is buried and whose end is open brightens
/// monotonically from start to end.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_bond_brightens_from_its_buried_end_to_its_open_end() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_quad_with_exposure(
        &Vec3::new(-10.0, 0.0, 0.0),
        &Vec3::new(10.0, 0.0, 0.0),
        1.5,
        &AO_ALBEDO,
        [[0, 0, 0, 0], FULLY_EXPOSED],
    );
    upload_impostors(
        &mut renderer,
        &AtomImpostorMesh::new(),
        &bonds,
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    renderer.set_shading(ao_of(LightingPreset::Headlight, 1.0));
    let pixels = renderer.render(DARK_BACKGROUND);
    dump("ao_bond", &pixels);
    let row = VIEWPORT_HEIGHT / 2;
    let (start, _) = pixel_of(-9.5, 0.0);
    let (end, _) = pixel_of(9.5, 0.0);
    let profile: Vec<f32> = (start..=end)
        .map(|column| luminance_of(rgba_at(&pixels, VIEWPORT_WIDTH, column, row)))
        .collect();
    for pair in profile.windows(2) {
        assert!(pair[1] >= pair[0], "not monotonic: {profile:?}");
    }
    let (first, last) = (profile[0], profile[profile.len() - 1]);
    assert!(last > first + 20.0, "start {first}, end {last}");
}

// ---- Shadows, Phase 1 (doc/design_shadows.md) ------------------------------
//
// Caster passes, the single-cascade fit and the receiver. Expected colours
// come from the twin at `key_visibility` 0 (full shadow, strength 1) and 1
// (lit); the shadow lookup itself has no twin (Testing layer 3). The
// renderer's default is shadows off, so every image above is unchanged
// (the anchor, against a dump of the phase before).

use atomcad_renderer::shading::FULLY_EXPOSED as SHADING_FULLY_EXPOSED;
use atomcad_renderer::shadow_fit::{ShadowSetup, shadow_setup};
use atomcad_renderer::tessellator::tessellator::tessellate_sphere;
use glam::f64::DVec2;

const SHADOW_GREY: [f32; 3] = [0.7, 0.7, 0.7];

/// *Studio* with shadows at `strength`, nothing else.
fn shadows_of(strength: f32) -> ShadingSettings {
    ShadingSettings {
        preset: LightingPreset::Studio,
        shadow_strength: strength,
        ..ShadingSettings::default()
    }
}

/// The content of one shadow scene: every kind of caster and receiver, plus
/// a label and a background grid to take group 2 before the transparent
/// draws (D8).
#[derive(Default)]
struct ShadowScene {
    main: Mesh,
    atoms: Option<AtomImpostorMesh>,
    bonds: Option<BondImpostorMesh>,
    ghosts: Option<TransparentImpostorMesh>,
    surfaces: TransparentSurfaceMesh,
    labels: Option<LabelMesh>,
    gadget_atoms: Option<AtomImpostorMesh>,
    grid: bool,
}

impl ShadowScene {
    fn atoms(&mut self) -> &mut AtomImpostorMesh {
        self.atoms.get_or_insert_with(AtomImpostorMesh::new)
    }

    fn upload(&self, renderer: &mut Renderer) {
        let empty_atoms = AtomImpostorMesh::new();
        let empty_bonds = BondImpostorMesh::new();
        let empty_ghosts = TransparentImpostorMesh::new();
        let empty_labels = LabelMesh::new();
        renderer.update_all_gpu_meshes(
            &Mesh::new(),
            &LineMesh::new(),
            &self.main,
            &LineMesh::new(),
            &LineMesh::new(),
            self.atoms.as_ref().unwrap_or(&empty_atoms),
            self.bonds.as_ref().unwrap_or(&empty_bonds),
            self.ghosts.as_ref().unwrap_or(&empty_ghosts),
            &self.surfaces,
            self.labels.as_ref().unwrap_or(&empty_labels),
            self.gadget_atoms.as_ref().unwrap_or(&empty_atoms),
            &BondImpostorMesh::new(),
            true,
            Some((1, "Main".to_string())),
        );
        let mut grid = LineMesh::new();
        if self.grid {
            for i in -4..=4 {
                let t = i as f32 * 4.0;
                grid.add_line_with_uniform_color(
                    &Vec3::new(t, -20.0, -3.0),
                    &Vec3::new(t, 20.0, -3.0),
                    &[0.5, 0.5, 0.5],
                );
            }
        }
        renderer.update_background_mesh(&grid);
    }
}

/// A square grid of touching spheres of `radius`, `2n + 1` on a side,
/// centred on the origin in the plane `z`.
fn sphere_layer(atoms: &mut AtomImpostorMesh, n: i32, radius: f32, z: f32) {
    for i in -n..=n {
        for j in -n..=n {
            let centre = Vec3::new(i as f32 * 2.0 * radius, j as f32 * 2.0 * radius, z);
            atoms.add_atom_quad(&centre, radius, &SHADOW_GREY, 0.5, 0.0, &[0.0; 4]);
        }
    }
}

fn add_atom(atoms: &mut AtomImpostorMesh, centre: DVec3, radius: f32, albedo: [f32; 3]) {
    atoms.add_atom_quad(&centre.as_vec3(), radius, &albedo, 0.5, 0.0, &[0.0; 4]);
}

/// `mesh` with every vertex moved by `by`.
fn translated(mut mesh: Mesh, by: Vec3) -> Mesh {
    for v in &mut mesh.vertices {
        v.position = (Vec3::from(v.position) + by).to_array();
    }
    mesh
}

/// The live view's shadow setup, as `draw_to_target` computes it.
fn live_setup(renderer: &Renderer) -> Option<ShadowSetup> {
    shadow_setup(
        &renderer.camera,
        renderer.caster_bounds(),
        &renderer.shading(),
        &renderer.live_target_spec(),
    )
}

/// The live target at the harness's `VIEWPORT_WIDTH × VIEWPORT_HEIGHT`
/// render pixels with `ratio` of them per output pixel, as on a display of
/// that device pixel ratio. The scene is drawn alike in render pixels, but
/// the cascade criterion (D6) is per *output* pixel, so a ratio of 4 lets
/// cascade 0's texel grow four times coarser. 1 restores the default.
fn set_output_ratio(renderer: &mut Renderer, ratio: f64) {
    renderer.set_viewport_request(
        VIEWPORT_WIDTH as f64 / ratio,
        VIEWPORT_HEIGHT as f64 / ratio,
        ratio,
    );
    assert_eq!(
        renderer.get_viewport_size(),
        (VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
    );
}

/// The live view with shadows at strength 1, then off (both *Studio*),
/// dumped as `shadow_<name>_on` / `_off`. `still` draws the refined frame.
fn shadow_pair(renderer: &mut Renderer, name: &str, still: bool) -> (Vec<u8>, Vec<u8>) {
    let mut render = |strength: f32| {
        renderer.set_shading(shadows_of(strength));
        if still {
            renderer.render_still(DARK_BACKGROUND)
        } else {
            renderer.render(DARK_BACKGROUND)
        }
    };
    let on = render(1.0);
    let off = render(0.0);
    dump(&format!("shadow_{name}_on"), &on);
    dump(&format!("shadow_{name}_off"), &off);
    (on, off)
}

/// Where `point` lies on the live target, in pixels, y down.
fn screen_of(renderer: &Renderer, point: DVec3) -> DVec2 {
    let (width, height) = renderer.get_viewport_size();
    let ndc = renderer
        .camera
        .build_view_projection_matrix()
        .project_point3(point);
    DVec2::new(
        (ndc.x * 0.5 + 0.5) * width as f64,
        (0.5 - ndc.y * 0.5) * height as f64,
    )
}

fn pixel_at(pixels: &[u8], renderer: &Renderer, at: DVec2) -> [u8; 4] {
    let (width, _) = renderer.get_viewport_size();
    rgba_at(pixels, width, at.x as u32, at.y as u32)
}

/// `point` moved along the light's travel direction onto the plane `z`.
fn along_light_onto(setup: &ShadowSetup, point: DVec3, z: f64) -> DVec3 {
    point + setup.light_dir * ((z - point.z) / setup.light_dir.z)
}

/// The light's travel direction on screen, unit, in pixels (y down).
fn light_on_screen(renderer: &Renderer, setup: &ShadowSetup) -> DVec2 {
    let camera = &renderer.camera;
    let forward = (camera.target - camera.eye).normalize();
    let right = forward.cross(camera.up).normalize();
    let up = right.cross(forward);
    DVec2::new(setup.light_dir.dot(right), -setup.light_dir.dot(up)).normalize()
}

/// The twin's colour, 0–255, of a surface at `hit` with `normal` and
/// `albedo` (roughness 0.5, not metallic, fully exposed) under `settings`,
/// with the key at `key_visibility`.
fn shadow_twin(
    renderer: &Renderer,
    settings: &ShadingSettings,
    hit: DVec3,
    normal: DVec3,
    albedo: [f32; 3],
    key_visibility: f32,
) -> Vec3 {
    let camera = &renderer.camera;
    let setup = shadow_setup(
        camera,
        renderer.caster_bounds(),
        settings,
        &renderer.live_target_spec(),
    );
    let uniform = ShadingUniform::new(
        camera,
        renderer.content_bounds(),
        DARK_BACKGROUND,
        settings,
        ShadingPass::Content,
        setup.as_ref(),
    );
    to_display(light_surface_exposed(
        &uniform,
        view_vector(camera, hit),
        normal.normalize().as_vec3(),
        Vec3::from(albedo),
        0.5,
        0.0,
        SHADING_FULLY_EXPOSED,
        key_visibility,
    )) * 255.0
}

/// How much a full shadow takes from a surface, in luminance (0–255): the
/// twin lit minus the twin in full shadow at strength 1.
fn full_shadow_drop(renderer: &Renderer, hit: DVec3, normal: DVec3, albedo: [f32; 3]) -> f32 {
    let lum = |c: Vec3| 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
    let settings = shadows_of(1.0);
    lum(shadow_twin(renderer, &settings, hit, normal, albedo, 1.0))
        - lum(shadow_twin(renderer, &settings, hit, normal, albedo, 0.0))
}

/// A far, tiny caster straight behind the view's target, `depth` Å further
/// along the view axis: hidden behind the scene and shadowing nothing in
/// it, it stretches the casters' view-depth extent, so cascade 0 grows as
/// deep as the cascade criterion lets it (D6) and its texel is as coarse as
/// `κ` output pixels. With [`set_output_ratio`] that is coarse.
fn far_caster(renderer: &Renderer, depth: f64) -> DVec3 {
    let camera = &renderer.camera;
    camera.target + (camera.target - camera.eye).normalize() * depth
}

/// D4, D8, D9: with shadows on under *Studio*, the live, the thumbnail and
/// the export path draw without a validation error, each recording one
/// caster pass (one cascade) unless the map already holds its setup (D9);
/// the map exists exactly while shadows are on.
/// Under *Headlight* a strength of 1 draws nothing and is byte-identical to
/// strength 0 (D1).
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn shadows_draw_through_every_path_and_count_their_passes() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_fixed_scene(&mut renderer);
    look_down(&mut renderer, false, 40.0, VIEWPORT_HEIGHT);
    assert!(
        !renderer.shadow_map_allocated(),
        "the default is shadows off"
    );

    let headlight = |strength| ShadingSettings {
        shadow_strength: strength,
        ..ShadingSettings::default()
    };
    renderer.set_shading(headlight(0.0));
    let plain = renderer.render(DARK_BACKGROUND);
    renderer.set_shading(headlight(1.0));
    assert!(!renderer.shadow_map_allocated(), "no map under Headlight");
    let headlight_on = renderer.render(DARK_BACKGROUND);
    assert_eq!(plain, headlight_on, "Headlight draws no shadow");
    assert_eq!(renderer.shadow_passes_drawn(), 0);

    renderer.set_shading(shadows_of(0.7));
    assert!(renderer.shadow_map_allocated());
    let before = renderer.shadow_passes_drawn();
    let live = renderer.render(DARK_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), before + 1);
    dump("shadow_paths_live", &live);
    // The offscreen draws record a pass only where their setup differs from
    // the one the map holds (D9). This compact scene is wholly in every
    // view, so the footprint is the casters' square whatever the aspect,
    // and every path may well share the live setup.
    let live_fit = live_setup(&renderer).expect("a setup");
    let camera = renderer.camera.clone();
    let mut square = camera.clone();
    square.aspect = 1.0;
    let thumbnail_fit = setup_for(
        &renderer,
        &square,
        &TargetSpec::image(THUMBNAIL_SIZE, THUMBNAIL_SIZE, THUMBNAIL_SUPERSAMPLING),
    );
    let mut wide = camera.clone();
    wide.aspect = 200.0 / 120.0;
    let export_fit = setup_for(
        &renderer,
        &wide,
        &TargetSpec::image(200, 120, IMAGE_EXPORT_SUPERSAMPLING),
    );
    let after_thumbnail = before + 1 + passes_after(&live_fit, &thumbnail_fit);
    let after_export = after_thumbnail + passes_after(&thumbnail_fit, &export_fit);
    let thumbnail = renderer.render_thumbnail(&camera);
    // Two backgrounds, one setup: the second draw finds the map cached.
    assert_eq!(renderer.shadow_passes_drawn(), after_thumbnail);
    dump("shadow_paths_thumbnail", &thumbnail);
    let image = renderer.render_image(200, 120, DARK_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), after_export);
    dump("shadow_paths_export", &image);
    // The atom over the quad shadows it: the live frame differs from today's.
    assert_ne!(live, plain);

    renderer.set_shading(shadows_of(0.0));
    assert!(
        !renderer.shadow_map_allocated(),
        "turning shadows off frees the map"
    );
    renderer.render(DARK_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), after_export);
}

/// The acne detector's lone objects, by name: (name, is a mesh).
const ACNE_OBJECTS: [(&str, bool); 4] = [
    ("sphere", false),
    ("bond", false),
    ("cube", true),
    ("tessellated_sphere", true),
];

/// One lone object for the acne detector, at the origin, with a far caster
/// at `far` when the texel is to be coarse.
fn acne_scene(name: &str, far: Option<DVec3>) -> ShadowScene {
    let mut scene = ShadowScene::default();
    match name {
        "sphere" => add_atom(scene.atoms(), DVec3::ZERO, 3.0, [0.2, 0.5, 0.9]),
        "bond" => {
            let mut bonds = BondImpostorMesh::new();
            bonds.add_bond_with_positions(
                &Vec3::new(-4.0, -1.0, 0.5),
                &Vec3::new(4.0, 1.0, -0.5),
                0.6,
                &[0.7, 0.7, 0.7],
            );
            scene.bonds = Some(bonds);
        }
        "cube" => scene.main = cube(2.5, Vec3::new(0.8, 0.3, 0.2), 1.0),
        _ => tessellate_sphere(
            &mut scene.main,
            &DVec3::ZERO,
            3.0,
            24,
            12,
            &Material::new(&Vec3::new(0.3, 0.7, 0.4), 0.5, 0.0),
        ),
    }
    if let Some(far) = far {
        add_atom(scene.atoms(), far, 0.5, [0.5, 0.5, 0.5]);
    }
    scene
}

/// Testing layer 4, the acne detector (a bias too small): a lone object
/// has nothing to shadow it, so with shadows on at strength 1 it must look
/// as with them off. Impostors: every pixel within 2/255. Meshes: at most
/// 0.5 % of the object's pixels further off than that. At the finest
/// texel (about 0.01 Å) and a coarse one (at least 0.3 Å, from a far caster
/// that stretches the casters' depth and an output ratio of 4, which lets
/// cascade 0 be that coarse), in both projections, with MSAA off and on
/// and in the refined frame. Mutation: without the normal offset and the
/// receiver-plane depth, acne appears.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_lone_object_does_not_shadow_itself() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    let mut worst = (0u8, String::new());
    for (name, is_mesh) in ACNE_OBJECTS {
        for orthographic in [false, true] {
            renderer.set_orthographic_mode(orthographic);
            renderer.set_ortho_half_height(7.0);
            renderer.move_camera(
                &DVec3::new(10.0, -20.0, 16.0),
                &DVec3::ZERO,
                &DVec3::new(0.0, 0.0, 1.0),
            );
            for coarse in [false, true] {
                set_output_ratio(&mut renderer, if coarse { 4.0 } else { 1.0 });
                let far = coarse.then(|| far_caster(&renderer, 1500.0));
                acne_scene(name, far).upload(&mut renderer);
                renderer.set_shading(shadows_of(1.0));
                let texel = live_setup(&renderer).expect("a setup").cascades[0].texel_world;
                if coarse {
                    assert!(texel >= 0.3, "{name}: coarse texel {texel}");
                } else {
                    assert!(texel <= 0.012, "{name}: fine texel {texel}");
                }
                for sampling in ["msaa_off", "msaa_on", "refined"] {
                    let anti_aliasing = sampling != "msaa_off";
                    if anti_aliasing && !renderer.msaa_supported()
                        || sampling == "refined" && !renderer.refinement_supported()
                    {
                        continue;
                    }
                    renderer.set_render_quality(RenderQuality {
                        anti_aliasing,
                        full_display_resolution: true,
                    });
                    let projection = if orthographic { "ortho" } else { "persp" };
                    let texels = if coarse { "coarse" } else { "fine" };
                    let label = format!("acne_{name}_{projection}_{texels}_{sampling}");
                    let (on, off) = shadow_pair(&mut renderer, &label, sampling == "refined");
                    let object = count_non_background(&off, DARK_BACKGROUND, true);
                    assert!(object > 1000, "{label}: the object is {object} pixels");
                    let mut acne = 0;
                    for (a, b) in on.chunks_exact(4).zip(off.chunks_exact(4)) {
                        let diff = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
                        if diff > worst.0 {
                            worst = (diff, label.clone());
                        }
                        acne += (diff > 2) as usize;
                    }
                    if is_mesh {
                        assert!(
                            acne as f64 <= 0.005 * object as f64,
                            "{label}: {acne} of {object} pixels darkened (texel {texel:.4} Å)"
                        );
                    } else {
                        assert_eq!(
                            acne, 0,
                            "{label}: {acne} pixels darkened (texel {texel:.4} Å)"
                        );
                    }
                }
            }
        }
    }
    println!(
        "renderer_gpu: acne detector, largest difference {}/255 ({})",
        worst.0, worst.1
    );
}

/// Testing layer 4, the contact detector (a bias too large): a sphere
/// resting on a CSG quad, and a sphere resting in the hollow of four
/// spheres of a layer.
/// The receiver just outside the resting sphere's silhouette, on the shadow
/// side, is darker than the same receiver far from it by at least half the
/// full-shadow difference (the twin at `v = 0` and `v = 1`). Top-down,
/// orthographic, at the fine and the coarse texel (a far caster and an
/// output ratio of 2).
///
/// Seen from above, that receiver lies under a long chord of the sphere: the
/// map compares it with the sphere's surface nearest the light, several Å
/// away, so this bounds the bias only loosely (`CONST_BIAS_TEXELS` × 10
/// passes it). The tight bound is `a_bond_lying_on_a_quad_shadows_its_contact`.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_resting_sphere_shadows_its_contact() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    // (name, resting sphere centre and radius, the layer's sphere radius or
    // `None` for the quad at z = 0). In the layer, spheres of 1.5 Å touch on
    // a 3 Å grid, and one of 1.5 Å resting in the hollow at (1.5, 1.5)
    // touches four of them: its centre is 3/√2 Å above the layer's.
    let cases: [(&str, DVec3, f64, Option<f64>); 2] = [
        ("quad", DVec3::new(0.0, 0.0, 2.0), 2.0, None),
        (
            "layer",
            DVec3::new(1.5, 1.5, 3.0 / 2f64.sqrt()),
            1.5,
            Some(1.5),
        ),
    ];
    for (name, centre, radius, receiver) in cases {
        for coarse in [false, true] {
            let mut scene = ShadowScene::default();
            add_atom(scene.atoms(), centre, radius as f32, [0.9, 0.4, 0.2]);
            match receiver {
                None => scene.main = square(15.0, 0.0, Vec3::from(SHADOW_GREY)),
                Some(r) => sphere_layer(scene.atoms(), 5, r as f32, 0.0),
            }
            set_output_ratio(&mut renderer, if coarse { 2.0 } else { 1.0 });
            if coarse {
                let far = far_caster(&renderer, 1200.0);
                add_atom(scene.atoms(), far, 0.5, [0.5, 0.5, 0.5]);
            }
            scene.upload(&mut renderer);
            renderer.set_shading(shadows_of(1.0));
            let setup = live_setup(&renderer).expect("a setup");
            let texel = setup.cascades[0].texel_world;
            let texels = if coarse { "coarse" } else { "fine" };
            if coarse {
                assert!(texel >= 0.3, "{name}: coarse texel {texel}");
            }
            let (on, _) = shadow_pair(&mut renderer, &format!("contact_{name}_{texels}"), false);

            // Just outside the resting sphere, along the light on screen,
            // and the same receiver point far from it.
            let towards = light_on_screen(&renderer, &setup);
            let radius_px = radius * 10.0;
            let near_px = screen_of(&renderer, centre) + towards * (radius_px + 3.0);
            let offset_a = (near_px - screen_of(&renderer, DVec3::ZERO)) / 10.0;
            let (near_hit, far_hit, normal) = match receiver {
                None => {
                    let hit = DVec3::new(offset_a.x, -offset_a.y, 0.0);
                    (hit, hit + DVec3::new(-9.0, 6.0, 0.0), DVec3::Z)
                }
                Some(r) => {
                    // The layer sphere under that pixel.
                    let at = DVec2::new(offset_a.x, -offset_a.y);
                    let c = (at / (2.0 * r)).round() * (2.0 * r);
                    let (x, y) = (at.x - c.x, at.y - c.y);
                    let z = (r * r - x * x - y * y).sqrt();
                    let c = DVec3::new(c.x, c.y, 0.0);
                    let hit = c + DVec3::new(x, y, z);
                    // The same point of a layer sphere far to the upper left.
                    let other = DVec3::new(-6.0 * r, 4.0 * r, 0.0);
                    (hit, hit + other, (hit - c).normalize())
                }
            };
            let near = luminance_of(pixel_at(&on, &renderer, near_px));
            let far = luminance_of(pixel_at(&on, &renderer, screen_of(&renderer, far_hit)));
            let drop = full_shadow_drop(&renderer, near_hit, normal, SHADOW_GREY);
            println!(
                "renderer_gpu: contact {name} {texels} (texel {texel:.3} Å): near {near:.1}, \
                 far {far:.1}, full shadow drop {drop:.1}"
            );
            assert!(
                drop > 10.0,
                "{name}: the sampled receiver faces away from the key"
            );
            assert!(
                far - near >= 0.5 * drop,
                "{name} {texels}: beside the contact {near:.1}, far {far:.1}, \
                 a full shadow takes {drop:.1}"
            );
        }
    }
}

/// Testing layer 4, the contact detector where it can see the bias: a bond
/// lying on a CSG quad. A shadow map compares a receiver with the caster's
/// surface nearest the light, so beside a resting sphere the gap is the
/// sphere's whole chord (about 5 Å on the shadow side, seen from above) and
/// no plausible bias leaks there; a stick is thin. Its shadow is a strip
/// 0.3–0.75 Å from its axis, and the light-facing surface is under 0.9 Å
/// above the strip's middle along the light. At a texel of about 0.18 Å the
/// PCF (0.27 Å) still resolves the strip, and the bias (about 0.2 Å) is
/// well under that gap. The strip's middle is darker than the open quad by
/// at least half the full-shadow difference. Mutation: `CONST_BIAS_TEXELS`
/// × 10 (about 0.95 Å here); the strip loses its shadow.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_bond_lying_on_a_quad_shadows_its_contact() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let (_, _, _, probe) = caster_over_layer(&mut renderer, true, false);
    let light = probe.light_dir;
    let radius = 0.3;
    // Across the light's horizontal direction, so the strip lies along it.
    let along = light.cross(DVec3::Z).normalize() * 6.0;
    let axis = DVec3::new(0.0, 0.0, radius);
    let mut scene = ShadowScene {
        main: square(15.0, 0.0, Vec3::from(SHADOW_GREY)),
        ..ShadowScene::default()
    };
    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_with_positions(
        &(axis - along).as_vec3(),
        &(axis + along).as_vec3(),
        radius as f32,
        &[0.7, 0.7, 0.7],
    );
    scene.bonds = Some(bonds);
    let far = far_caster(&renderer, 610.0);
    add_atom(scene.atoms(), far, 0.5, [0.5, 0.5, 0.5]);
    scene.upload(&mut renderer);
    renderer.set_shading(shadows_of(1.0));
    let setup = live_setup(&renderer).expect("a setup");
    let texel = setup.cascades[0].texel_world;
    assert!((0.15..=0.22).contains(&texel), "texel {texel}");
    let (on, _) = shadow_pair(&mut renderer, "contact_bond", false);
    let horizontal = DVec3::new(light.x, light.y, 0.0).normalize();
    let strip = DVec3::new(0.0, 0.0, 0.0) + horizontal * 0.5;
    let open = strip - horizontal * 4.0;
    let near = luminance_of(pixel_at(&on, &renderer, screen_of(&renderer, strip)));
    let far = luminance_of(pixel_at(&on, &renderer, screen_of(&renderer, open)));
    let drop = full_shadow_drop(&renderer, strip, DVec3::Z, SHADOW_GREY);
    println!(
        "renderer_gpu: contact bond (texel {texel:.3} Å): strip {near:.1}, open {far:.1}, \
         full shadow drop {drop:.1}"
    );
    assert!(drop > 10.0);
    assert!(
        far - near >= 0.5 * drop,
        "the strip beside the bond {near:.1}, the open quad {far:.1}, a full shadow takes {drop:.1}"
    );
}

/// The scene of the direction and position checks: a sphere 4 Å above a
/// receiver (a square layer of spheres or a flat quad), placed so that its
/// shadow's centre falls on the layer sphere at the origin. With `coarse`,
/// a far caster and an output ratio of 4 make the texel coarse, and the
/// sphere is larger (4 Å, 8 Å up), so it still covers many texels and its
/// shadow stays clear of it on screen; without, the ratio is 1.
fn caster_over_layer(
    renderer: &mut Renderer,
    flat: bool,
    coarse: bool,
) -> (ShadowScene, DVec3, f64, ShadowSetup) {
    let (radius, gap) = if coarse { (4.0, 8.0) } else { (1.5, 4.0) };
    let top = 1.0;
    set_output_ratio(renderer, if coarse { 4.0 } else { 1.0 });
    // The light direction depends only on the camera, so a first setup
    // (any casters) gives it.
    renderer.set_shading(shadows_of(1.0));
    let mut probe = ShadowScene::default();
    add_atom(probe.atoms(), DVec3::ZERO, 1.0, SHADOW_GREY);
    probe.upload(renderer);
    let light = live_setup(renderer).expect("a setup").light_dir;
    let height = top + gap + radius;
    let centre = DVec3::new(0.0, 0.0, top) - light * ((height - top) / -light.z);
    let mut scene = ShadowScene::default();
    add_atom(scene.atoms(), centre, radius as f32, [0.9, 0.4, 0.2]);
    if flat {
        scene.main = square(18.0, top as f32, Vec3::from(SHADOW_GREY));
    } else {
        sphere_layer(scene.atoms(), 8, 1.0, 0.0);
    }
    if coarse {
        let far = far_caster(renderer, 1600.0);
        add_atom(scene.atoms(), far, 0.5, [0.5, 0.5, 0.5]);
    }
    scene.upload(renderer);
    let setup = live_setup(renderer).expect("a setup");
    (scene, centre, radius, setup)
}

/// The luminance drop shadows cause at each pixel: off − on, ≥ 0.
fn darkening(on: &[u8], off: &[u8]) -> Vec<f32> {
    on.chunks_exact(4)
        .zip(off.chunks_exact(4))
        .map(|(a, b)| {
            let lum = |p: &[u8]| 0.0722 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.2126 * p[2] as f32;
            (lum(b) - lum(a)).max(0.0)
        })
        .collect()
}

/// D1, D4, D5, D7: the shadow lands where the light puts it. A sphere 4 Å
/// above a layer of spheres, looked at from above under *Studio* at strength
/// 1, in both projections: the layer is darker below and to the right of
/// the sphere's screen position, not above and to the left; the layer
/// sphere at the shadow's centre is the twin at `v = 0` there within 2/255,
/// one far from it the twin at `v = 1`. Mutation: flip `L` in the light
/// camera.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_shadow_falls_below_and_right_of_its_caster_and_is_full_in_its_core() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    for orthographic in [false, true] {
        look_down(&mut renderer, orthographic, 60.0, VIEWPORT_HEIGHT);
        let (_, centre, _, _) = caster_over_layer(&mut renderer, false, false);
        let projection = if orthographic { "ortho" } else { "persp" };
        let (on, off) = shadow_pair(&mut renderer, &format!("direction_{projection}"), false);
        let drop = darkening(&on, &off);
        let caster = screen_of(&renderer, centre);
        let shadow = screen_of(&renderer, DVec3::new(0.0, 0.0, 1.0));
        assert!(
            shadow.x > caster.x && shadow.y > caster.y,
            "{projection}: the shadow's centre {shadow} is not below and right of the caster {caster}"
        );
        let mirrored = caster - (shadow - caster);
        let mean_drop = |at: DVec2| {
            let (mut sum, mut n) = (0.0, 0);
            for (i, d) in drop.iter().enumerate() {
                let p = DVec2::new(
                    (i as u32 % VIEWPORT_WIDTH) as f64,
                    (i as u32 / VIEWPORT_WIDTH) as f64,
                );
                if (p - at).length() < 8.0 {
                    sum += d;
                    n += 1;
                }
            }
            sum / n as f32
        };
        let (below_right, above_left) = (mean_drop(shadow), mean_drop(mirrored));
        assert!(
            below_right > 20.0 && above_left < 1.0,
            "{projection}: darkening below right {below_right}, above left {above_left}"
        );

        // The layer sphere at the shadow's core, and one far from it, against
        // the twin.
        let camera = renderer.camera.clone();
        for (sphere, key_visibility) in [(DVec3::ZERO, 0.0), (DVec3::new(-12.0, 6.0, 0.0), 1.0)] {
            let (column, row) =
                pixel_over(&camera, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, sphere + DVec3::Z);
            let (origin, dir) =
                ray_through_pixel(&camera, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, column, row);
            let hit = ray_sphere(origin, dir, sphere, 1.0).expect("the pixel is on the sphere");
            let expected = shadow_twin(
                &renderer,
                &shadows_of(1.0),
                hit,
                hit - sphere,
                SHADOW_GREY,
                key_visibility,
            );
            assert_near(
                rgba_at(&on, VIEWPORT_WIDTH, column, row),
                expected,
                2.0,
                &format!("{projection}: layer sphere at {sphere}, v = {key_visibility}"),
            );
        }
    }
}

/// Testing layer 4, the position check: the darkness-weighted centroid of a
/// sphere's shadow on a flat layer lies within two output pixels of the CPU
/// projection of the sphere's centre along `L` onto the layer, in both
/// projections, at the fine and at a coarse texel. (A flat layer, not the
/// sphere layer: on bumps the shadow's centroid moves with the surface's
/// height.) The expectation includes the receiver's normal offset (D7),
/// which moves a shadow by up to a texel. Mutation: offset the receiver's
/// light camera (not the caster pass's) by half a texel; at the coarse
/// texel that is a few pixels. (Offsetting the light camera itself moves
/// the caster pass and the receiver together and changes nothing.)
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_shadow_is_centred_on_the_casters_projection_along_the_light() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    for orthographic in [false, true] {
        for coarse in [false, true] {
            look_down(&mut renderer, orthographic, 60.0, VIEWPORT_HEIGHT);
            let (_, centre, _, setup) = caster_over_layer(&mut renderer, true, coarse);
            let texel = setup.cascades[0].texel_world;
            let projection = if orthographic { "ortho" } else { "persp" };
            let texels = if coarse { "coarse" } else { "fine" };
            let (on, off) = shadow_pair(
                &mut renderer,
                &format!("position_{projection}_{texels}"),
                false,
            );
            let drop = darkening(&on, &off);
            let (mut sum, mut weight) = (DVec2::ZERO, 0.0);
            for (i, d) in drop.iter().enumerate() {
                if *d > 2.0 {
                    let p = DVec2::new(
                        (i as u32 % VIEWPORT_WIDTH) as f64 + 0.5,
                        (i as u32 / VIEWPORT_WIDTH) as f64 + 0.5,
                    );
                    sum += p * *d as f64;
                    weight += *d as f64;
                }
            }
            assert!(weight > 0.0, "{projection} {texels}: no shadow");
            let centroid = sum / weight;
            // The receiver looks the map up one texel off the surface along
            // its normal (`NORMAL_OFFSET_TEXELS · t · sin θ`, D7), which
            // moves the shadow by as much against it.
            let sin_theta = (1.0 - setup.light_dir.z * setup.light_dir.z).sqrt();
            let lookup = DVec3::Z * (texel * sin_theta);
            let expected = screen_of(&renderer, along_light_onto(&setup, centre - lookup, 1.0));
            let off_by = (centroid - expected).length();
            println!(
                "renderer_gpu: shadow position {projection} {texels} (texel {texel:.3} Å): \
                 centroid {centroid}, expected {expected}, off by {off_by:.2} px"
            );
            assert!(
                off_by <= 2.0,
                "{projection} {texels}: the shadow's centroid {centroid} is {off_by:.2} px \
                 from the caster's projection {expected}"
            );
            if coarse {
                assert!(texel >= 0.4, "{projection}: coarse texel {texel}");
            }
        }
    }
}

/// D5: a caster above the top edge of the view still shadows the visible
/// layer: the depth range and the footprint are the casters' sphere's, not
/// the view's. Mutation: fit the depth range to the view slice's corners
/// instead; the caster falls out of the map.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_caster_off_screen_still_casts() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    for orthographic in [false, true] {
        look_down(&mut renderer, orthographic, 60.0, VIEWPORT_HEIGHT);
        let (_, _, _, probe) = caster_over_layer(&mut renderer, true, false);
        let light = probe.light_dir;
        // The shadow's centre 6 Å above the view's centre, on the layer; the
        // caster 20 Å above the layer, back along the light: high enough to
        // be nearer the light than any corner of the visible slice.
        let shadow_at = DVec3::new(0.0, 6.0, 0.0);
        let radius = 1.5;
        let centre = shadow_at - light * (20.0 / -light.z);
        let caster_px = screen_of(&renderer, centre);
        assert!(
            caster_px.y + radius * 10.0 < 0.0,
            "the caster is on screen at {caster_px}"
        );
        let mut scene = ShadowScene {
            main: square(60.0, 0.0, Vec3::from(SHADOW_GREY)),
            ..ShadowScene::default()
        };
        add_atom(scene.atoms(), centre, radius as f32, [0.9, 0.4, 0.2]);
        scene.upload(&mut renderer);
        let projection = if orthographic { "ortho" } else { "persp" };
        let (on, off) = shadow_pair(&mut renderer, &format!("offscreen_{projection}"), false);
        let at = screen_of(&renderer, shadow_at);
        let got = luminance_of(pixel_at(&off, &renderer, at))
            - luminance_of(pixel_at(&on, &renderer, at));
        let drop = full_shadow_drop(&renderer, shadow_at, DVec3::Z, SHADOW_GREY);
        assert!(
            got >= 0.5 * drop,
            "{projection}: the off-screen caster's shadow takes {got:.1} of {drop:.1}"
        );
    }
}

/// D7: a receiver outside the cascade's square is lit. One caster sphere,
/// mostly beyond the upper left of the view, so that the view's footprint
/// cuts it and it reaches the square's border,
/// over a large ghost layer deeper than every caster: ghost pixels whose
/// light-space position is outside the square and far from the sphere
/// equal shadows off. Mutation: without the explicit `uv` bounds test the
/// clamped lookups read the sphere at the border and darken them.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_receiver_outside_the_square_is_lit() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let (radius, sphere) = (3.0, DVec3::new(-17.5, 7.0, 5.0));
    let ghost_z = -25.0;
    let mut scene = ShadowScene::default();
    add_atom(scene.atoms(), sphere, radius as f32, [0.9, 0.4, 0.2]);
    let mut ghosts = TransparentImpostorMesh::new();
    for i in -12..=12 {
        for j in -8..=8 {
            let centre = Vec3::new(i as f32 * 1.6, j as f32 * 1.6, ghost_z as f32);
            ghosts.add_atom_quad(&centre, 0.8, &[0.6, 0.8, 0.9], 0.5, 0.0, &[0.0; 4], 0.8);
        }
    }
    scene.ghosts = Some(ghosts);
    scene.upload(&mut renderer);
    renderer.set_shading(shadows_of(1.0));
    let setup = live_setup(&renderer).expect("a setup");
    let cascade = setup.cascades[0];
    let (on, off) = shadow_pair(&mut renderer, "outside_square", false);

    // The ghost layer's top under each pixel (orthographic: straight down),
    // in light space.
    let half = 0.5 * cascade.side;
    let sphere_l = setup.to_light_space(sphere).truncate();
    let sphere_px = screen_of(&renderer, sphere);
    let mut checked = 0;
    for row in 0..VIEWPORT_HEIGHT {
        for column in 0..VIEWPORT_WIDTH {
            let (x, y) = world_of_pixel((row * VIEWPORT_WIDTH + column) as usize);
            let point = DVec3::new(x as f64, y as f64, ghost_z + 0.8);
            let l = setup.to_light_space(point).truncate() - cascade.centre_l;
            let outside = l.x.abs().max(l.y.abs()) > half + 2.0;
            let far = (setup.to_light_space(point).truncate() - sphere_l).length() > radius + 3.0;
            let i = ((row * VIEWPORT_WIDTH + column) * 4) as usize;
            // A ghost, not the caster in front of it.
            let on_caster =
                (DVec2::new(column as f64, row as f64) - sphere_px).length() < radius * 10.0 + 2.0;
            let is_ghost =
                !on_caster && count_non_background(&off[i..i + 4], DARK_BACKGROUND, true) == 1;
            if outside && far && is_ghost {
                for c in 0..3 {
                    assert!(
                        on[i + c].abs_diff(off[i + c]) <= 1,
                        "pixel ({column}, {row}) outside the square darkened: {:?} vs {:?}",
                        &on[i..i + 3],
                        &off[i..i + 3]
                    );
                }
                checked += 1;
            }
        }
    }
    println!("renderer_gpu: {checked} ghost pixels outside the square checked");
    assert!(checked > 200, "only {checked} pixels outside the square");
}

/// D3: casters and receivers of every kind. A bond above a flat layer
/// casts; a CSG cube above a layer of spheres casts; the flat CSG layer
/// under an atom receives (the position check's scene, here at the
/// shadow's core).
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn bonds_and_meshes_cast_and_meshes_receive() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let (_, atom_centre, _, setup) = caster_over_layer(&mut renderer, true, false);
    let light = setup.light_dir;
    let core = DVec3::new(0.0, 0.0, 1.0);
    let (on, off) = shadow_pair(&mut renderer, "kinds_atom_on_quad", false);
    let at = screen_of(&renderer, core);
    let got =
        luminance_of(pixel_at(&off, &renderer, at)) - luminance_of(pixel_at(&on, &renderer, at));
    let drop = full_shadow_drop(&renderer, core, DVec3::Z, SHADOW_GREY);
    assert!(
        got >= 0.8 * drop,
        "the quad under an atom: {got:.1} of {drop:.1}"
    );

    // A bond in the atom's place, across the light's path to the core.
    let mut scene = ShadowScene {
        main: square(18.0, 1.0, Vec3::from(SHADOW_GREY)),
        ..ShadowScene::default()
    };
    let across = light.cross(DVec3::Z).normalize() * 3.0;
    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_with_positions(
        &(atom_centre - across).as_vec3(),
        &(atom_centre + across).as_vec3(),
        0.8,
        &[0.7, 0.7, 0.7],
    );
    scene.bonds = Some(bonds);
    scene.upload(&mut renderer);
    let (on, off) = shadow_pair(&mut renderer, "kinds_bond_on_quad", false);
    let got =
        luminance_of(pixel_at(&off, &renderer, at)) - luminance_of(pixel_at(&on, &renderer, at));
    assert!(got >= 0.8 * drop, "a bond casts: {got:.1} of {drop:.1}");

    // A cube in the atom's place, over the layer of spheres.
    let mut scene = ShadowScene {
        main: translated(
            cube(1.5, Vec3::new(0.8, 0.3, 0.2), 1.0),
            atom_centre.as_vec3(),
        ),
        ..ShadowScene::default()
    };
    sphere_layer(scene.atoms(), 8, 1.0, 0.0);
    scene.upload(&mut renderer);
    let (on, off) = shadow_pair(&mut renderer, "kinds_cube_on_layer", false);
    let got =
        luminance_of(pixel_at(&off, &renderer, at)) - luminance_of(pixel_at(&on, &renderer, at));
    let drop = full_shadow_drop(&renderer, core, DVec3::Z, SHADOW_GREY);
    assert!(
        got >= 0.8 * drop,
        "a cube casts on spheres: {got:.1} of {drop:.1}"
    );
}

/// D3, D8: transparent content does not cast but does receive. A ghost
/// sphere and a transparent isosurface above a flat layer leave the image
/// as with shadows off. In a second scene a ghost inside an atom's shadow
/// is darker than with shadows off, and one outside it is not. Both scenes
/// hold a label and a grid, so the transparent draws come after both have
/// taken group 2: this is what exercises the re-bind (D8). Mutation: skip
/// the re-bind before the transparent impostors; the draw fails validation.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn transparent_content_receives_but_does_not_cast() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let mut labels = LabelMesh::new();
    labels.add_glyph_quad(
        &Vec3::new(8.0, 6.0, 2.0),
        [-0.5, -0.5],
        [0.5, 0.5],
        [0.0, 0.0],
        [0.1, 0.1],
        0.0,
    );
    let surface = translated(
        cube(1.5, Vec3::new(0.3, 0.8, 0.4), 0.5),
        Vec3::new(6.0, -2.0, 5.0),
    );
    let index_count = surface.indices.len() as u32;
    let mut ghosts = TransparentImpostorMesh::new();
    ghosts.add_atom_quad(
        &Vec3::new(-6.0, 2.0, 5.0),
        1.5,
        &[0.9, 0.6, 0.2],
        0.5,
        0.0,
        &[0.0; 4],
        0.5,
    );
    let scene = ShadowScene {
        main: square(18.0, 0.0, Vec3::from(SHADOW_GREY)),
        ghosts: Some(ghosts),
        surfaces: TransparentSurfaceMesh {
            mesh: surface,
            components: vec![SurfaceComponentRange {
                first_index: 0,
                index_count,
                centroid: Vec3::new(6.0, -2.0, 5.0),
            }],
        },
        labels: Some(labels.clone()),
        grid: true,
        ..ShadowScene::default()
    };
    scene.upload(&mut renderer);
    let (on, off) = shadow_pair(&mut renderer, "transparent_casts_nothing", false);
    let worst = on
        .iter()
        .zip(&off)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(worst <= 2, "transparent content cast a shadow: {worst}/255");

    // A ghost in an atom's shadow, and one outside it.
    let (_, _, _, probe) = caster_over_layer(&mut renderer, true, false);
    let light = probe.light_dir;
    let ghost_in = DVec3::new(-4.0, 0.0, 3.0);
    let ghost_out = DVec3::new(6.0, 4.0, 3.0);
    let caster = ghost_in - light * (5.0 / -light.z);
    let mut ghosts = TransparentImpostorMesh::new();
    for centre in [ghost_in, ghost_out] {
        ghosts.add_atom_quad(
            &centre.as_vec3(),
            1.2,
            &[0.9, 0.6, 0.2],
            0.5,
            0.0,
            &[0.0; 4],
            0.6,
        );
    }
    let mut scene = ShadowScene {
        main: square(18.0, 0.0, Vec3::from(SHADOW_GREY)),
        ghosts: Some(ghosts),
        labels: Some(labels),
        grid: true,
        ..ShadowScene::default()
    };
    add_atom(scene.atoms(), caster, 2.0, [0.9, 0.4, 0.2]);
    scene.upload(&mut renderer);
    let (on, off) = shadow_pair(&mut renderer, "ghost_receives", false);
    let lum_drop = |p: DVec3| {
        let at = screen_of(&renderer, p + DVec3::new(0.0, 0.0, 1.2));
        luminance_of(pixel_at(&off, &renderer, at)) - luminance_of(pixel_at(&on, &renderer, at))
    };
    let (inside, outside) = (lum_drop(ghost_in), lum_drop(ghost_out));
    assert!(
        inside > 10.0,
        "the ghost in the shadow darkened by {inside}"
    );
    assert!(
        outside.abs() <= 1.0,
        "the ghost outside the shadow changed by {outside}"
    );
}

/// D3, D8: the gadget pass receives nothing (its strength is 0): a guide
/// dot inside a shadow is drawn as with shadows off. And a selected atom
/// in shadow keeps its rim colour within 2/255: the rim is mixed after the
/// lighting (DP D4).
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn gadgets_and_rims_are_not_shadowed() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    let (_, caster, caster_radius, setup) = caster_over_layer(&mut renderer, false, false);
    // The same scene, with the layer sphere at the shadow's core selected,
    // and a guide dot between the caster and the layer, on the light's path
    // to that core.
    let mut scene = ShadowScene::default();
    add_atom(scene.atoms(), caster, caster_radius as f32, [0.9, 0.4, 0.2]);
    for i in -8..=8 {
        for j in -8..=8 {
            let rim = if i == 0 && j == 0 {
                RIM_COLOUR
            } else {
                [0.0; 4]
            };
            let centre = Vec3::new(i as f32 * 2.0, j as f32 * 2.0, 0.0);
            scene
                .atoms()
                .add_atom_quad(&centre, 1.0, &SHADOW_GREY, 0.5, 0.0, &rim);
        }
    }
    let dot = DVec3::new(0.0, 0.0, 1.0) - setup.light_dir * (1.5 / -setup.light_dir.z);
    let mut gadget_atoms = AtomImpostorMesh::new();
    add_atom(&mut gadget_atoms, dot, 0.4, [0.9, 0.9, 0.2]);
    scene.gadget_atoms = Some(gadget_atoms);
    scene.upload(&mut renderer);
    let (on, off) = shadow_pair(&mut renderer, "gadget_and_rim", false);
    let at = screen_of(&renderer, dot);
    assert_eq!(
        pixel_at(&on, &renderer, at),
        pixel_at(&off, &renderer, at),
        "the guide dot is shadowed"
    );
    // The selected atom is in the cast shadow, or this tests nothing: its
    // inside darkens.
    let rimmed = DVec3::ZERO;
    let inside = screen_of(&renderer, rimmed) + DVec2::new(3.0, 3.0);
    let darkened = luminance_of(pixel_at(&off, &renderer, inside))
        - luminance_of(pixel_at(&on, &renderer, inside));
    assert!(
        darkened > 10.0,
        "the selected atom is not in the shadow: {darkened}"
    );
    let rim = Vec3::new(RIM_COLOUR[0], RIM_COLOUR[1], RIM_COLOUR[2]) * 255.0;
    for angle in [0.0f64, 120.0, 240.0] {
        let (s, c) = angle.to_radians().sin_cos();
        let at = screen_of(&renderer, rimmed) + DVec2::new(c, -s) * (10.0 - 1.5);
        assert_near(
            pixel_at(&on, &renderer, at),
            rim,
            2.0,
            &format!("rim at {angle}°"),
        );
        let (a, b) = (pixel_at(&on, &renderer, at), pixel_at(&off, &renderer, at));
        for ch in 0..3 {
            assert!(
                a[ch].abs_diff(b[ch]) <= 2,
                "rim at {angle}°: {a:?} vs {b:?}"
            );
        }
    }
}

// ---- Shadows, Phase 2: adaptive cascades (doc/design_shadows.md D6) --------
//
// A perspective close-up on a slab hundreds of Å deep, at a grazing angle,
// gets several cascades; the debug tint shows which receiver reads which.

/// The grazing camera of the Phase 2 scenes: perspective, the eye 14 Å above
/// the slab's top face (z = 0) and just beyond its near edge, looking across
/// it and slightly down.
fn look_across_slab(renderer: &mut Renderer) {
    renderer.set_orthographic_mode(false);
    renderer.move_camera(
        &DVec3::new(0.0, -215.0, 14.0),
        &DVec3::new(0.0, -150.0, 0.0),
        &DVec3::Z,
    );
}

/// An axis-aligned box from `min` to `max`, its faces outward.
fn box_mesh(min: Vec3, max: Vec3, albedo: Vec3) -> Mesh {
    let mut mesh = cube(0.5, albedo, 1.0);
    for v in &mut mesh.vertices {
        let unit = Vec3::from(v.position) + Vec3::splat(0.5);
        v.position = (min + unit * (max - min)).to_array();
    }
    mesh
}

/// The grazing slab as a box, 400 × 400 × 20 Å, its top face at z = 0.
fn slab_box() -> Mesh {
    box_mesh(
        Vec3::new(-200.0, -200.0, -20.0),
        Vec3::new(200.0, 200.0, 0.0),
        Vec3::from(SHADOW_GREY),
    )
}

/// `b` appended to `a`.
fn merged(mut a: Mesh, b: Mesh) -> Mesh {
    let first = a.vertices.len() as u32;
    a.vertices.extend(b.vertices);
    a.indices.extend(b.indices.iter().map(|i| i + first));
    a
}

/// Which cascade's debug tint (D9) turned `plain` into `tinted`: red,
/// green, blue, yellow for 0–3, from the sign of each channel's change, or
/// `None` if the change matches none of them.
fn tint_cascade(plain: [u8; 4], tinted: [u8; 4]) -> Option<usize> {
    let d = |c: usize| tinted[c] as i32 - plain[c] as i32;
    let (up, down) = (|c| d(c) > 3, |c| d(c) < -3);
    match (up(0), up(1), up(2), down(0), down(1), down(2)) {
        (true, false, false, false, true, true) => Some(0),
        (false, true, false, true, false, true) => Some(1),
        (false, false, true, true, true, false) => Some(2),
        (true, true, false, false, false, true) => Some(3),
        _ => None,
    }
}

/// The live view at strength 1 with the cascade tint off, then on, dumped as
/// `shadow_<name>_plain` / `_tinted`.
fn tint_pair(renderer: &mut Renderer, name: &str) -> (Vec<u8>, Vec<u8>) {
    renderer.set_shading(shadows_of(1.0));
    renderer.set_shadow_cascade_debug(false);
    let plain = renderer.render(DARK_BACKGROUND);
    renderer.set_shadow_cascade_debug(true);
    let tinted = renderer.render(DARK_BACKGROUND);
    renderer.set_shadow_cascade_debug(false);
    dump(&format!("shadow_{name}_plain"), &plain);
    dump(&format!("shadow_{name}_tinted"), &tinted);
    (plain, tinted)
}

/// D6, D9: the debug tint on the grazing slab, built as an impostor lattice
/// of about 40 000 spheres. The pixels nearest the camera carry cascade 0's
/// tint, the farthest a higher one, and the tint changes nothing with the
/// debug flag off (or under a pass without shadows). Mutation: select the
/// cascade by where it starts instead of where it ends.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_debug_tint_shows_the_near_and_far_cascades_on_a_grazing_slab() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_across_slab(&mut renderer);
    let mut scene = ShadowScene::default();
    sphere_layer(scene.atoms(), 100, 1.0, -1.0);
    scene.upload(&mut renderer);
    renderer.set_shading(shadows_of(1.0));
    let setup = live_setup(&renderer).expect("a setup");
    println!(
        "renderer_gpu: tinted slab, {} cascades, splits {:?}",
        setup.cascades.len(),
        setup
            .cascades
            .iter()
            .map(|c| c.split_far)
            .collect::<Vec<_>>()
    );
    assert!(setup.cascades.len() >= 2, "one cascade: {setup:?}");
    let (plain, tinted) = tint_pair(&mut renderer, "tint_slab");

    // The nearest pixels: the bottom rows. The farthest: the topmost row the
    // slab covers.
    let (width, height) = (VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    let at = |pixels: &[u8], column: u32, row: u32| rgba_at(pixels, width, column, row);
    let top = (0..height)
        .find(|&row| {
            count_non_background(
                &plain[(row * width * 4) as usize..((row + 1) * width * 4) as usize],
                DARK_BACKGROUND,
                true,
            ) > width as usize / 2
        })
        .expect("the slab reaches the horizon");
    let mut near = [0usize; 5];
    let mut far = [0usize; 5];
    for column in (20..width - 20).step_by(7) {
        for row in height - 4..height {
            near[tint_cascade(at(&plain, column, row), at(&tinted, column, row)).unwrap_or(4)] += 1;
        }
        for row in top + 1..top + 3 {
            far[tint_cascade(at(&plain, column, row), at(&tinted, column, row)).unwrap_or(4)] += 1;
        }
    }
    println!("renderer_gpu: tint near {near:?}, far {far:?} (cascades 0-3, unmatched)");
    let near_total: usize = near.iter().sum();
    assert_eq!(
        near[0], near_total,
        "the nearest pixels are not all cascade 0's: {near:?}"
    );
    let far_total: usize = far.iter().sum();
    assert!(
        far[1..4].iter().sum::<usize>() >= far_total * 9 / 10,
        "the farthest pixels are not a later cascade's: {far:?}"
    );

    // The flag off draws the untinted picture.
    renderer.set_shadow_cascade_debug(false);
    assert_eq!(renderer.render(DARK_BACKGROUND), plain);
}

/// Where the darkening `drop` of row `row` first reaches `level`, scanning
/// columns from `from` in the direction `step` (±1), interpolated between
/// pixel centres.
fn crossing_in_row(drop: &[f32], row: u32, from: u32, step: i32, level: f32) -> Option<f64> {
    let width = VIEWPORT_WIDTH as i32;
    let d = |column: i32| drop[(row as i32 * width + column) as usize];
    let mut column = from as i32;
    while column + step >= 0 && column + step < width {
        let (a, b) = (d(column), d(column + step));
        if a < level && b >= level {
            let f = (level - a) / (b - a);
            return Some(column as f64 + 0.5 + step as f64 * f as f64);
        }
        column += step;
    }
    None
}

/// The steeper view of the seam and position checks: perspective, the eye
/// 50 Å above a large slab, looking 14° down across it. Its cascade 0 ends
/// about 217 Å away, mid-screen, once [`near_caster`] fixes the casters'
/// front close to the eye.
fn look_down_the_slab(renderer: &mut Renderer) {
    renderer.set_orthographic_mode(false);
    renderer.move_camera(
        &DVec3::new(0.0, -400.0, 50.0),
        &DVec3::new(0.0, -200.0, 0.0),
        &DVec3::Z,
    );
}

/// An 800 × 800 × 20 Å slab, its top face at z = 0.
fn large_slab_box() -> Mesh {
    box_mesh(
        Vec3::new(-400.0, -400.0, -20.0),
        Vec3::new(400.0, 400.0, 0.0),
        Vec3::from(SHADOW_GREY),
    )
}

/// A tiny caster 3 Å in front of the eye, off screen to the right, its
/// shadow off screen too. It puts the casters' front near the eye, so
/// cascade 0 starts where an output pixel is far below the floor and is as
/// deep as the floor allows (D6): about 200 Å of footprint.
fn near_caster(renderer: &Renderer) -> DVec3 {
    let camera = &renderer.camera;
    let forward = (camera.target - camera.eye).normalize();
    let right = forward.cross(camera.up).normalize();
    camera.eye + forward * 3.0 + right * 6.0 + DVec3::Z * 2.0
}

/// D7: no seam. A wall standing on a large slab along the view casts a
/// straight shadow edge that crosses the split between cascade 0 (texel
/// 0.09 Å) and cascade 1 (0.84 Å, from a far hidden caster). Unblended, the
/// two cascades put the edge about a pixel apart, and it jumps where the
/// split falls. Blended, it moves across the band gradually: between any
/// two neighbouring rows near the split and its band, by at most one output
/// pixel. Mutation: `blend_fraction` 0 (the jump measured 1.3 px).
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_shadow_edge_across_a_split_has_no_seam() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down_the_slab(&mut renderer);
    let wall = box_mesh(
        Vec3::new(-1.0, -390.0, 0.0),
        Vec3::new(0.0, 390.0, 8.0),
        Vec3::new(0.9, 0.4, 0.2),
    );
    let mut scene = ShadowScene {
        main: merged(large_slab_box(), wall),
        ..ShadowScene::default()
    };
    add_atom(
        scene.atoms(),
        far_caster(&renderer, 2000.0),
        0.5,
        SHADOW_GREY,
    );
    add_atom(scene.atoms(), near_caster(&renderer), 0.05, SHADOW_GREY);
    scene.upload(&mut renderer);
    renderer.set_shading(shadows_of(1.0));
    let setup = live_setup(&renderer).expect("a setup");
    let (first, second) = (setup.cascades[0], setup.cascades[1]);
    assert!(
        second.texel_world > 5.0 * first.texel_world,
        "the cascades are too alike to show a seam: {:?}",
        setup.cascades
    );
    let (on, off) = shadow_pair(&mut renderer, "seam", false);
    let drop = darkening(&on, &off);
    let core = drop.iter().copied().fold(0.0f32, f32::max);

    // The rows of the split and of the band's start: the slab's top face at
    // those view depths, under the eye.
    let camera = renderer.camera.clone();
    let forward = (camera.target - camera.eye).normalize();
    let foot = DVec3::new(camera.eye.x, camera.eye.y, 0.0);
    let row_at_depth = |depth: f64| {
        let along = (depth - (foot - camera.eye).dot(forward)) / DVec3::Y.dot(forward);
        screen_of(&renderer, foot + DVec3::Y * along).y
    };
    let band = SHADOW_BLEND_FRACTION_GPU * (first.split_far - first.split_near);
    let split_row = row_at_depth(first.split_far);
    let band_row = row_at_depth(first.split_far - band);
    assert!(
        band_row - split_row > 6.0 && split_row > 40.0,
        "the band (rows {split_row:.1}–{band_row:.1}) is not well inside the view"
    );

    // The shadow's far edge in every row from 15 above the split to 15
    // below the band, scanning in from the right.
    let rows = (split_row - 15.0) as u32..(band_row + 15.0) as u32;
    let edges: Vec<f64> = rows
        .clone()
        .map(|row| {
            crossing_in_row(&drop, row, VIEWPORT_WIDTH - 1, -1, 0.5 * core)
                .unwrap_or_else(|| panic!("no shadow edge in row {row}"))
        })
        .collect();
    let (step, at) = edges
        .windows(2)
        .zip(rows)
        .map(|(pair, row)| ((pair[1] - pair[0]).abs(), row))
        .fold((0.0, 0), |worst, s| if s.0 > worst.0 { s } else { worst });
    println!(
        "renderer_gpu: seam, split at row {split_row:.1}, band to row {band_row:.1}, \
         largest step of the edge {step:.2} px (rows {at}–{})",
        at + 1
    );
    assert!(
        step <= 1.0,
        "the shadow edge jumps by {step:.2} px between rows {at} and {}",
        at + 1
    );
}

/// The penumbra's width, in pixels, along the middle row of `drop`: the
/// distance between where the darkening reaches 10 % and 90 % of `core`,
/// scanning in from the right.
fn penumbra_px(drop: &[f32], core: f32) -> f64 {
    let row = VIEWPORT_HEIGHT / 2;
    let outer = crossing_in_row(drop, row, VIEWPORT_WIDTH - 1, -1, 0.1 * core).expect("10 %");
    let inner = crossing_in_row(drop, row, VIEWPORT_WIDTH - 1, -1, 0.9 * core).expect("90 %");
    outer - inner
}

/// D7: the softness is in Å, not texels. A plate's straight shadow edge on
/// a flat quad, top-down and orthographic at 50 render pixels per Å, in a
/// view whose (one) cascade has a texel of about 0.02 Å and in one of about
/// 0.06 Å (a far hidden caster at two depths): the penumbra is as wide in
/// both, within 30 %. Mutation: the PCF radius as `1.5 · t` alone, in
/// texels.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_penumbra_is_as_wide_in_a_fine_and_a_coarse_cascade() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    let pixels_per_a = 50.0;
    // The plate's right edge is at x = 0 and its top 3 Å above the quad: its
    // shadow's edge is at `x = 3 · L_x / −L_z`. The camera looks down on that.
    look_down(&mut renderer, true, 60.0, VIEWPORT_HEIGHT);
    renderer.set_shading(shadows_of(1.0));
    let mut probe = ShadowScene::default();
    add_atom(probe.atoms(), DVec3::ZERO, 1.0, SHADOW_GREY);
    probe.upload(&mut renderer);
    let light = live_setup(&renderer).expect("a setup").light_dir;
    let edge_x = 3.0 * light.x / -light.z;
    assert!(
        edge_x > 1.0,
        "the light does not throw the edge clear of the plate"
    );
    renderer.set_ortho_half_height(VIEWPORT_HEIGHT as f64 / (2.0 * pixels_per_a));
    renderer.move_camera(
        &DVec3::new(edge_x, 0.0, 60.0),
        &DVec3::new(edge_x, 0.0, 0.0),
        &DVec3::Y,
    );
    let mut widths = Vec::new();
    for far in [50.0, 170.0] {
        let plate = box_mesh(
            Vec3::new(-20.0, -20.0, 2.8),
            Vec3::new(0.0, 20.0, 3.0),
            Vec3::new(0.9, 0.4, 0.2),
        );
        let mut scene = ShadowScene {
            main: merged(square(20.0, 0.0, Vec3::from(SHADOW_GREY)), plate),
            ..ShadowScene::default()
        };
        add_atom(scene.atoms(), far_caster(&renderer, far), 0.5, SHADOW_GREY);
        scene.upload(&mut renderer);
        renderer.set_shading(shadows_of(1.0));
        let setup = live_setup(&renderer).expect("a setup");
        assert_eq!(setup.cascades.len(), 1);
        let texel = setup.cascades[0].texel_world;
        let (on, off) = shadow_pair(&mut renderer, &format!("penumbra_{far}"), false);
        let drop = darkening(&on, &off);
        let core = drop.iter().copied().fold(0.0f32, f32::max);
        let width = penumbra_px(&drop, core) / pixels_per_a;
        println!("renderer_gpu: penumbra at texel {texel:.4} Å: {width:.3} Å");
        widths.push((texel, width));
    }
    let ((fine_t, fine), (coarse_t, coarse)) = (widths[0], widths[1]);
    assert!(
        (0.015..=0.025).contains(&fine_t) && (0.05..=0.07).contains(&coarse_t),
        "texels {fine_t} and {coarse_t}"
    );
    assert!(
        (coarse / fine - 1.0).abs() <= 0.3,
        "the penumbra is {fine:.3} Å at a texel of {fine_t:.3} Å but {coarse:.3} Å at {coarse_t:.3} Å"
    );
}

/// Testing layer 4, the acne detector on several cascades: the grazing slab
/// (a box, so nothing on it can shadow anything else) drawn with shadows on
/// at strength 1 looks as with them off, in every cascade, with MSAA off
/// and on: at most 0.5 % of its pixels differ by more than 2/255.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_grazing_slab_does_not_shadow_itself_in_any_cascade() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    look_across_slab(&mut renderer);
    ShadowScene {
        main: slab_box(),
        ..ShadowScene::default()
    }
    .upload(&mut renderer);
    renderer.set_shading(shadows_of(1.0));
    let setup = live_setup(&renderer).expect("a setup");
    assert!(setup.cascades.len() >= 2, "{:?}", setup.cascades);
    for anti_aliasing in [false, true] {
        if anti_aliasing && !renderer.msaa_supported() {
            continue;
        }
        renderer.set_render_quality(RenderQuality {
            anti_aliasing,
            full_display_resolution: true,
        });
        let label = format!("acne_grazing_slab_msaa_{anti_aliasing}");
        let (on, off) = shadow_pair(&mut renderer, &label, false);
        let object = count_non_background(&off, DARK_BACKGROUND, true);
        assert!(object > 10_000, "{label}: the slab is {object} pixels");
        let acne = on
            .chunks_exact(4)
            .zip(off.chunks_exact(4))
            .filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 2))
            .count();
        println!("renderer_gpu: {label}: {acne} of {object} pixels darkened");
        assert!(
            acne as f64 <= 0.005 * object as f64,
            "{label}: {acne} of {object} pixels darkened"
        );
    }
}

/// Testing layer 4, the position check in a later cascade: a sphere 8 Å
/// above the large slab, placed so that its shadow's centre falls at a view
/// depth of 320 Å, well inside cascade 1 (the debug tint confirms it). The
/// darkness-weighted centroid of its shadow lies within two output pixels
/// of the CPU projection of its centre along `L` onto the slab, the
/// receiver's normal offset included. Mutation: index `world_to_clip` with
/// `k − 1`.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_shadow_in_a_later_cascade_is_centred_on_the_casters_projection() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    look_down_the_slab(&mut renderer);
    renderer.set_shading(shadows_of(1.0));
    let camera = renderer.camera.clone();
    let forward = (camera.target - camera.eye).normalize();
    // The shadow's centre: the slab's top face 320 Å away along the view,
    // projected straight down onto it.
    let mut core = camera.eye + forward * 320.0;
    core.z = 0.0;
    let build = |renderer: &mut Renderer, light: DVec3| {
        let (radius, height) = (4.0, 8.0 + 4.0);
        let centre = core - light * (height / -light.z);
        let mut scene = ShadowScene {
            main: large_slab_box(),
            ..ShadowScene::default()
        };
        add_atom(scene.atoms(), centre, radius as f32, [0.9, 0.4, 0.2]);
        add_atom(
            scene.atoms(),
            far_caster(renderer, 2000.0),
            0.5,
            SHADOW_GREY,
        );
        add_atom(scene.atoms(), near_caster(renderer), 0.05, SHADOW_GREY);
        scene.upload(renderer);
        centre
    };
    // The light depends only on the camera; a first scene gives it.
    build(&mut renderer, -DVec3::Z);
    let light = live_setup(&renderer).expect("a setup").light_dir;
    let centre = build(&mut renderer, light);
    let setup = live_setup(&renderer).expect("a setup");
    let depth = (core - camera.eye).dot(forward);
    let k = setup
        .cascades
        .iter()
        .position(|c| depth <= c.split_far)
        .expect("a cascade");
    let cascade = setup.cascades[k];
    let band = SHADOW_BLEND_FRACTION_GPU * (cascade.split_far - cascade.split_near);
    assert!(
        k >= 1 && depth > cascade.split_near + 1.0 && depth < cascade.split_far - band,
        "the shadow's centre at depth {depth} is not inside cascade 1 or later: {:?}",
        setup.cascades
    );

    let (on, off) = shadow_pair(&mut renderer, "position_cascade_1", false);
    let (plain, tinted) = tint_pair(&mut renderer, "position_cascade_1");
    let expected_px = screen_of(&renderer, core);
    assert_eq!(
        tint_cascade(
            pixel_at(&plain, &renderer, expected_px),
            pixel_at(&tinted, &renderer, expected_px)
        ),
        Some(k),
        "the debug tint disagrees about the shadow's cascade"
    );
    // The caster must not hide its own shadow: the two discs, each about
    // the caster's radius on screen, are apart.
    let caster_px = screen_of(&renderer, centre);
    let radius_px = 4.0 * VIEWPORT_HEIGHT as f64 / (2.0 * depth * (0.5 * camera.fovy).tan());
    assert!(
        (caster_px - expected_px).length() > 2.5 * radius_px,
        "the caster at {caster_px} covers its shadow at {expected_px}"
    );

    let drop = darkening(&on, &off);
    let (mut sum, mut weight) = (DVec2::ZERO, 0.0);
    for (i, d) in drop.iter().enumerate() {
        let p = DVec2::new(
            (i as u32 % VIEWPORT_WIDTH) as f64 + 0.5,
            (i as u32 / VIEWPORT_WIDTH) as f64 + 0.5,
        );
        if *d > 2.0 && (p - expected_px).length() < 25.0 {
            sum += p * *d as f64;
            weight += *d as f64;
        }
    }
    assert!(weight > 0.0, "no shadow");
    let centroid = sum / weight;
    // The receiver looks the map up a texel off the surface (D7), which
    // moves the shadow by as much against its normal.
    let sin_theta = (1.0 - light.z * light.z).sqrt();
    let lookup = DVec3::Z * (cascade.texel_world * sin_theta);
    let expected = screen_of(&renderer, along_light_onto(&setup, centre - lookup, 0.0));
    let off_by = (centroid - expected).length();
    println!(
        "renderer_gpu: shadow position in cascade {k} (texel {:.3} Å): centroid {centroid}, \
         expected {expected}, off by {off_by:.2} px",
        cascade.texel_world
    );
    assert!(
        off_by <= 2.0,
        "the shadow's centroid {centroid} is {off_by:.2} px from the caster's projection {expected}"
    );
}

/// `shadow_fit::SHADOW_BLEND_FRACTION`, for the band's rows.
const SHADOW_BLEND_FRACTION_GPU: f64 = atomcad_renderer::shadow_fit::SHADOW_BLEND_FRACTION;

// ---- Shadows, Phase 3: the cache (doc/design_shadows.md D9) ----------------
//
// The map remembers the setup it was last rendered for: a draw records
// caster passes only when its setup differs, and the upload that replaces
// the casters, or a new map, forgets it. Asserted on `shadow_passes_drawn`,
// never on timing. Every image the earlier phases draw is byte-identical to
// Phase 2's (the anchor): the cache changes what is drawn, never how.

use atomcad_renderer::render_target::TargetSpec;
use atomcad_renderer::thumbnail::THUMBNAIL_SUPERSAMPLING;

/// The cache scene's caster: 1.5 Å above the layer's tops.
const CACHE_CASTER: DVec3 = DVec3::new(0.0, 0.0, 4.0);
/// The same caster moved inside the casters' bounds, which stay the same.
const CACHE_CASTER_MOVED: DVec3 = DVec3::new(3.0, -2.0, 4.0);

/// A sphere at `caster` over a 50 Å layer of spheres, looked down on in
/// perspective with shadows on at strength 1. The layer runs past every side
/// of the view, so the view's frustum cuts the footprint (D5) and an
/// offscreen target of another aspect gets another setup.
fn upload_cache_scene(renderer: &mut Renderer, caster: DVec3) {
    let mut scene = ShadowScene::default();
    sphere_layer(scene.atoms(), 12, 1.0, 0.0);
    add_atom(scene.atoms(), caster, 1.5, [0.9, 0.4, 0.2]);
    scene.upload(renderer);
    look_down(renderer, false, 40.0, VIEWPORT_HEIGHT);
    renderer.set_shading(shadows_of(1.0));
}

/// The setup a draw of `camera` into a target of `spec` computes.
fn setup_for(renderer: &Renderer, camera: &Camera, spec: &TargetSpec) -> ShadowSetup {
    shadow_setup(camera, renderer.caster_bounds(), &renderer.shading(), spec).expect("a setup")
}

/// How many caster passes a draw with `setup` records: one per cascade.
fn passes_of(setup: &ShadowSetup) -> u64 {
    setup.cascades.len() as u64
}

/// How many caster passes a draw with `setup` records after one with
/// `remembered`, nothing having been uploaded between: none when the two
/// are equal (the map is cached), one per cascade otherwise.
fn passes_after(remembered: &ShadowSetup, setup: &ShadowSetup) -> u64 {
    if remembered == setup {
        0
    } else {
        passes_of(setup)
    }
}

/// The live view drawn as a normal frame, asserted to be one: a kept frame
/// would record no pass whatever the cache did.
fn render_normal(renderer: &mut Renderer, background: [u8; 3]) -> Vec<u8> {
    let pixels = renderer.render(background);
    assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Normal));
    pixels
}

/// D9: a second draw of an unchanged view records no shadow pass, on the
/// same background or another, and draws the same shadows.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn an_unchanged_view_records_its_shadow_passes_once() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_cache_scene(&mut renderer, CACHE_CASTER);
    let passes = passes_of(&live_setup(&renderer).expect("a setup"));
    let before = renderer.shadow_passes_drawn();
    let first = render_normal(&mut renderer, DARK_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), before + passes);
    let second = render_normal(&mut renderer, DARK_BACKGROUND);
    assert_eq!(
        renderer.shadow_passes_drawn(),
        before + passes,
        "an unchanged view re-rendered the shadow map"
    );
    assert!(first == second, "the cached map drew other shadows");
    dump("shadow_cache_unchanged", &second);
    // The background is not part of the setup.
    render_normal(&mut renderer, LIGHT_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), before + passes);
}

/// D9: the refined frame drawn once the view rests records no shadow pass,
/// and draws what a freshly rendered map gives.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn the_refined_frame_after_a_rest_records_no_shadow_pass() {
    let Some(mut renderer) = try_refining_renderer() else {
        return;
    };
    renderer.set_refine_delay(Some(Duration::ZERO));
    upload_cache_scene(&mut renderer, CACHE_CASTER);
    let passes = passes_of(&live_setup(&renderer).expect("a setup"));
    let before = renderer.shadow_passes_drawn();
    render_normal(&mut renderer, DARK_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), before + passes);
    let refined = renderer.render(DARK_BACKGROUND);
    assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Refined));
    assert_eq!(
        renderer.shadow_passes_drawn(),
        before + passes,
        "the refined frame re-rendered the shadow map"
    );
    dump("shadow_cache_refined", &refined);

    // Uploading the same content again forgets the map; the refined frame
    // drawn over the re-rendered map is the same.
    upload_cache_scene(&mut renderer, CACHE_CASTER);
    let fresh = renderer.render_still(DARK_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), before + 2 * passes);
    assert!(
        refined == fresh,
        "the cached refined frame differs from a fresh one"
    );
}

/// D9: a camera move records the new setup's passes; a content upload
/// records them even when the setup is unchanged, and the shadows then show
/// the new casters; a lightweight upload records none. Mutation: skip
/// `invalidate` in the upload; the moved caster's shadow is then stale.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn camera_moves_and_content_uploads_re_render_the_map_and_lightweight_ones_do_not() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_cache_scene(&mut renderer, CACHE_CASTER);
    render_normal(&mut renderer, DARK_BACKGROUND);
    let setup = live_setup(&renderer).expect("a setup");

    // A camera move.
    let before = renderer.shadow_passes_drawn();
    renderer.move_camera(
        &DVec3::new(2.0, 1.0, 40.0),
        &DVec3::new(2.0, 1.0, 0.0),
        &DVec3::new(0.0, 1.0, 0.0),
    );
    let moved = live_setup(&renderer).expect("a setup");
    assert_ne!(moved, setup, "the camera move did not change the setup");
    render_normal(&mut renderer, DARK_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), before + passes_of(&moved));
    look_down(&mut renderer, false, 40.0, VIEWPORT_HEIGHT);
    render_normal(&mut renderer, DARK_BACKGROUND);

    // A lightweight upload leaves the casters, and so the map, alone.
    let before = renderer.shadow_passes_drawn();
    let unchanged = render_normal(&mut renderer, DARK_BACKGROUND);
    assert_eq!(renderer.shadow_passes_drawn(), before);
    let mut gizmo = Mesh::new();
    tessellate_sphere(
        &mut gizmo,
        &DVec3::new(-8.0, 5.0, 6.0),
        1.0,
        12,
        12,
        &Material::new(&Vec3::new(0.9, 0.9, 0.2), 0.5, 0.0),
    );
    renderer.update_all_gpu_meshes(
        &gizmo,
        &LineMesh::new(),
        &Mesh::new(),
        &LineMesh::new(),
        &LineMesh::new(),
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &TransparentSurfaceMesh::new(),
        &LabelMesh::new(),
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
        false,
        None,
    );
    let with_gizmo = render_normal(&mut renderer, DARK_BACKGROUND);
    assert_eq!(
        renderer.shadow_passes_drawn(),
        before,
        "a lightweight upload re-rendered the shadow map"
    );
    assert!(
        with_gizmo != unchanged,
        "the lightweight mesh was not drawn"
    );

    // A content upload whose caster moves inside the same bounds: the setup
    // is unchanged, so only the upload's `invalidate` re-renders the map.
    let bounds = renderer.caster_bounds();
    upload_cache_scene(&mut renderer, CACHE_CASTER_MOVED);
    assert_eq!(renderer.caster_bounds(), bounds);
    assert_eq!(live_setup(&renderer).expect("a setup"), setup);
    let after_upload = render_normal(&mut renderer, DARK_BACKGROUND);
    dump("shadow_cache_after_upload", &after_upload);
    let Some(mut fresh) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_cache_scene(&mut fresh, CACHE_CASTER_MOVED);
    let reference = render_normal(&mut fresh, DARK_BACKGROUND);
    // The picture first, so the mutation shows as the stale shadow it is.
    assert!(
        after_upload == reference,
        "after the upload the map still shows the old casters"
    );
    assert_eq!(renderer.shadow_passes_drawn(), before + passes_of(&setup));
}

/// D9: a thumbnail, drawn on two backgrounds, records one set of passes.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn a_thumbnail_records_one_set_of_shadow_passes() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_cache_scene(&mut renderer, CACHE_CASTER);
    render_normal(&mut renderer, DARK_BACKGROUND);
    let mut camera = renderer.camera.clone();
    camera.aspect = 1.0;
    let thumbnail = setup_for(
        &renderer,
        &camera,
        &TargetSpec::image(THUMBNAIL_SIZE, THUMBNAIL_SIZE, THUMBNAIL_SUPERSAMPLING),
    );
    assert_ne!(
        Some(&thumbnail),
        live_setup(&renderer).as_ref(),
        "the thumbnail could reuse the live map, so this proves nothing"
    );
    let before = renderer.shadow_passes_drawn();
    let camera = renderer.camera.clone();
    let pixels = renderer.render_thumbnail(&camera);
    dump("shadow_cache_thumbnail", &pixels);
    assert_eq!(
        renderer.shadow_passes_drawn(),
        before + passes_of(&thumbnail)
    );
}

/// D9: an export, then a live frame, each record their own passes, and the
/// live frame shows its own shadows: it equals the live frame drawn before
/// the export. Mutation: leave the remembered setup unchanged on an
/// offscreen draw; the live frame then reads the export's map.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn an_export_and_the_next_live_frame_each_draw_their_own_shadows() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_cache_scene(&mut renderer, CACHE_CASTER);
    let live_before = render_normal(&mut renderer, DARK_BACKGROUND);
    let live = live_setup(&renderer).expect("a setup");
    let (width, height) = (400, 100);
    let mut camera = renderer.camera.clone();
    camera.aspect = width as f64 / height as f64;
    let export = setup_for(
        &renderer,
        &camera,
        &TargetSpec::image(width, height, IMAGE_EXPORT_SUPERSAMPLING),
    );
    assert_ne!(export, live, "the export could reuse the live map");

    let before = renderer.shadow_passes_drawn();
    let image = renderer.render_image(width, height, DARK_BACKGROUND);
    dump("shadow_cache_export", &image);
    assert_eq!(renderer.shadow_passes_drawn(), before + passes_of(&export));
    let live_after = render_normal(&mut renderer, DARK_BACKGROUND);
    assert!(
        live_after == live_before,
        "the live frame after an export shows other shadows"
    );
    assert_eq!(
        renderer.shadow_passes_drawn(),
        before + passes_of(&export) + passes_of(&live)
    );
}

/// D9: turning shadows off frees the map; turning them on again draws the
/// right shadows on the next frame, because a new map remembers no setup.
/// Mutation: keep the remembered setup when the map is replaced.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn turning_shadows_off_frees_the_map_and_on_again_redraws_it() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    upload_cache_scene(&mut renderer, CACHE_CASTER);
    let lit = render_normal(&mut renderer, DARK_BACKGROUND);
    let passes = passes_of(&live_setup(&renderer).expect("a setup"));

    renderer.set_shading(shadows_of(0.0));
    assert!(
        !renderer.shadow_map_allocated(),
        "shadows off keeps the map"
    );
    let off = render_normal(&mut renderer, DARK_BACKGROUND);
    assert!(off != lit, "the scene has no visible shadow");

    renderer.set_shading(shadows_of(1.0));
    assert!(renderer.shadow_map_allocated());
    let before = renderer.shadow_passes_drawn();
    let again = render_normal(&mut renderer, DARK_BACKGROUND);
    assert!(again == lit, "the new map drew other shadows");
    assert_eq!(renderer.shadow_passes_drawn(), before + passes);
}

// ---- Shadows, Phase 4: AO cedes the key (doc/design_shadows.md D10) --------

use atomcad_renderer::shading::{AO_KEY, AO_KEY_WITH_SHADOWS};

/// D10: AO's share of the key follows the shadow strength *drawn*, so the
/// coupling is inert wherever no shadow is drawn. With AO on and shadows
/// off, the AO scene is what Phase 3 drew (the `ao_pair_*` images of
/// [`a_buried_sphere_is_darker_by_the_mapping_and_alike_at_strength_0`]
/// against the dump); under *Headlight* a shadow strength of 1 is
/// byte-identical to 0 with AO on, too. Under *Studio* with shadows on, the
/// buried sphere's crown, which nothing shadows, is what the twin computes
/// with `AO_KEY_WITH_SHADOWS`. Mutation: compute `ao_key` from the setting
/// alone; the *Headlight* comparison fails.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn ao_cedes_the_key_only_where_a_shadow_is_drawn() {
    let Some(mut renderer) = try_renderer(VIEWPORT_WIDTH, VIEWPORT_HEIGHT) else {
        return;
    };
    renderer.set_render_quality(RenderQuality {
        anti_aliasing: false,
        full_display_resolution: true,
    });
    let open = (Vec3::new(-7.0, 0.0, 0.0), FULLY_EXPOSED);
    let buried = (Vec3::new(7.0, 0.0, 0.0), [0, 0, 0, 0]);
    upload_impostors(
        &mut renderer,
        &exposed_atoms(&[open, buried]),
        &BondImpostorMesh::new(),
        &TransparentImpostorMesh::new(),
        &AtomImpostorMesh::new(),
    );
    for orthographic in [false, true] {
        look_down(&mut renderer, orthographic, 60.0, VIEWPORT_HEIGHT);
        let with = |preset, shadow_strength| ShadingSettings {
            shadow_strength,
            ..ao_of(preset, 1.0)
        };
        renderer.set_shading(with(LightingPreset::Headlight, 0.0));
        let passes = renderer.shadow_passes_drawn();
        let off = renderer.render(DARK_BACKGROUND);
        renderer.set_shading(with(LightingPreset::Headlight, 1.0));
        let on = renderer.render(DARK_BACKGROUND);
        dump(&format!("ao_headlight_shadows_on_{orthographic}"), &on);
        assert!(off == on, "Headlight with AO, ortho {orthographic}");
        assert_eq!(renderer.shadow_passes_drawn(), passes);

        let settings = with(LightingPreset::Studio, 1.0);
        renderer.set_shading(settings);
        let pixels = renderer.render(DARK_BACKGROUND);
        dump(&format!("ao_studio_shadows_on_{orthographic}"), &pixels);
        let setup = live_setup(&renderer).expect("a shadow setup");
        let uniform = ShadingUniform::new(
            &renderer.camera,
            renderer.content_bounds(),
            DARK_BACKGROUND,
            &settings,
            ShadingPass::Content,
            Some(&setup),
        );
        assert_eq!(uniform.ao_key, AO_KEY_WITH_SHADOWS);
        let ((column, row), expected) =
            ao_twin_pixel(&renderer, &uniform, buried.0, buried.1, DVec3::Z);
        assert_near(
            rgba_at(&pixels, VIEWPORT_WIDTH, column, row),
            expected,
            2.0,
            &format!("buried crown, Studio with shadows, ortho {orthographic}"),
        );
        // ... and not what it would be with AO's full share of the key.
        let full_share = ShadingUniform {
            ao_key: AO_KEY,
            ..uniform
        };
        let (_, unshared) = ao_twin_pixel(&renderer, &full_share, buried.0, buried.1, DVec3::Z);
        assert!(
            (expected - unshared).abs().max_element() > 3.0,
            "the coupling is visible on the crown: {expected} vs {unshared}"
        );
    }
}
