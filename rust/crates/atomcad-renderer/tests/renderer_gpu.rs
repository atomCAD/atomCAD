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
fn try_renderer(width: u32, height: u32) -> Option<Renderer> {
    let adapter = block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    );
    if adapter.is_none() {
        println!("renderer_gpu: no wgpu adapter found, skipping");
        return None;
    }
    Some(block_on(Renderer::new(width, height)))
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
