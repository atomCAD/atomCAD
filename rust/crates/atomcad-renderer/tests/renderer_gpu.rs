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
use atomcad_renderer::line_mesh::LineMesh;
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
