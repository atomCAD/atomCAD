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

/// Writes `pixels` to `$ATOMCAD_GPU_DUMP_DIR/<name>.raw` when the variable is
/// set; does nothing otherwise.
fn dump(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("ATOMCAD_GPU_DUMP_DIR") {
        let path = std::path::Path::new(&dir).join(format!("{name}.raw"));
        std::fs::write(&path, pixels).expect("write GPU dump");
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
    renderer.set_viewport_size(width, height);
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
    check("set_viewport_size", &mut renderer, &|r| {
        r.set_viewport_size(VIEWPORT_WIDTH + 10, VIEWPORT_HEIGHT)
    });
    if renderer.msaa_supported() {
        check("set_render_quality", &mut renderer, &|r| {
            r.set_render_quality(RenderQuality {
                anti_aliasing: false,
            })
        });
    }
    check("set_refine_delay", &mut renderer, &|r| {
        r.set_refine_delay(Some(Duration::ZERO))
    });

    // Pushed on every refresh: unchanged values must not bump, or every
    // refresh would throw the refined image away.
    let before = renderer.live_generation();
    renderer.set_surface_transparency_mode(SurfaceTransparencyMode::SinglePass);
    renderer.set_render_quality(renderer.render_quality());
    renderer.set_viewport_size(VIEWPORT_WIDTH + 10, VIEWPORT_HEIGHT);
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
    });
    upload_fixed_scene(&mut renderer);
    for _ in 0..4 {
        renderer.render(BACKGROUND_RGB);
        assert_eq!(renderer.last_frame_kind(), Some(LiveFrameKind::Normal));
        assert_eq!(renderer.refine_in(), None);
    }
}
