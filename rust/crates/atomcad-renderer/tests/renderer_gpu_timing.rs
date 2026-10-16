// The shadows' frame cost (doc/design_shadows.md, Phase 3): frame times with
// shadows off and on, printed, asserted on nothing but the cascade count of
// the four-cascade view. Kept out of `renderer_gpu.rs`, whose `--ignored` run
// would otherwise build a million-atom lattice in a debug build. Run it in a
// release build, with nothing else on the GPU:
//
//     cargo test --release -p atomcad-renderer --test renderer_gpu_timing -- --ignored --nocapture

use atomcad_renderer::atom_impostor_mesh::AtomImpostorMesh;
use atomcad_renderer::bond_impostor_mesh::BondImpostorMesh;
use atomcad_renderer::label_mesh::LabelMesh;
use atomcad_renderer::line_mesh::LineMesh;
use atomcad_renderer::mesh::{Material, Mesh};
use atomcad_renderer::renderer::Renderer;
use atomcad_renderer::shading_uniform::{LightingPreset, ShadingSettings};
use atomcad_renderer::shadow_fit::shadow_setup;
use atomcad_renderer::tessellator::tessellator::tessellate_sphere;
use atomcad_renderer::transparent_impostor_mesh::TransparentImpostorMesh;
use atomcad_renderer::transparent_surface_mesh::TransparentSurfaceMesh;
use glam::f32::Vec3;
use glam::f64::{DQuat, DVec3};
use std::time::{Duration, Instant};

const WIDTH: u32 = 2560;
const HEIGHT: u32 = 1440;
const BACKGROUND: [u8; 3] = [30, 30, 30];
/// Frames drawn before timing, and frames timed.
const WARM_UP: usize = 3;
const FRAMES: usize = 20;
/// The camera's turn per moving frame, about the vertical through its
/// target: small enough that a view keeps its cascade count, large enough
/// that every frame has a new shadow setup.
const ORBIT_STEP_RAD: f64 = 0.002;

/// Diamond's cubic lattice constant, Å.
const DIAMOND_A: f64 = 3.567;
/// Conventional cells of the two 1 M-atom lattices (8 atoms each): a cube,
/// 178 Å a side, for the full view; a slab, 892 × 892 × 7 Å, for the
/// close-up. The cube is too small for four cascades: at the 0.1 Å texel
/// floor one cascade covers ~200 Å (D6).
const CUBE_CELLS: [i64; 3] = [50, 50, 50];
const SLAB_CELLS: [i64; 3] = [250, 250, 2];

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

/// A `WIDTH × HEIGHT` renderer with a 4× multisampled live target and no
/// automatic refinement, or `None` (with a note) without one.
fn try_renderer() -> Option<Renderer> {
    // The adapter `Renderer::new` picks, named so a recorded run says what
    // it ran on.
    let adapter = block_on(wgpu::Instance::default().request_adapter(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        },
    ));
    let Some(adapter) = adapter else {
        println!("renderer_gpu_timing: no wgpu adapter found, skipping");
        return None;
    };
    println!("renderer_gpu_timing: adapter {}", adapter.get_info().name);
    let mut renderer = block_on(Renderer::new(WIDTH, HEIGHT));
    renderer.set_refine_delay(None);
    if renderer.live_sample_count() != 4 {
        println!("renderer_gpu_timing: no 4x MSAA on this adapter, skipping");
        return None;
    }
    Some(renderer)
}

fn shading(shadow_strength: f32) -> ShadingSettings {
    ShadingSettings {
        preset: LightingPreset::Studio,
        shadow_strength,
        ..ShadingSettings::default()
    }
}

/// A diamond block of `cells` conventional cells in ball-and-stick: 8 atoms
/// a cell and about twice as many bonds, every one drawn (no culling).
/// Positions are in units of a/4: the fcc sites `2·(i, j, k)` with
/// `i + j + k` even, each bonded to its four neighbours `+(1, 1, 1)`,
/// `+(1, −1, −1)`, `+(−1, 1, −1)` and `+(−1, −1, 1)`, which are the fcc
/// sites shifted by `(1, 1, 1)`. Returns the meshes and the block's size.
fn diamond_block(cells: [i64; 3]) -> (AtomImpostorMesh, BondImpostorMesh, DVec3) {
    let [mx, my, mz] = cells.map(|c| 2 * c);
    let quarter = DIAMOND_A / 4.0;
    let at = |x: i64, y: i64, z: i64| {
        Vec3::new(
            (x as f64 * quarter) as f32,
            (y as f64 * quarter) as f32,
            (z as f64 * quarter) as f32,
        )
    };
    let grey = [0.6, 0.6, 0.6];
    let mut atoms = AtomImpostorMesh::new();
    let mut bonds = BondImpostorMesh::new();
    for i in 0..mx {
        for j in 0..my {
            for k in 0..mz {
                if (i + j + k) % 2 != 0 {
                    continue;
                }
                let (x, y, z) = (2 * i, 2 * j, 2 * k);
                let a = at(x, y, z);
                let b = at(x + 1, y + 1, z + 1);
                atoms.add_atom_quad(&a, 0.45, &grey, 0.5, 0.0, &[0.0; 4]);
                atoms.add_atom_quad(&b, 0.45, &grey, 0.5, 0.0, &[0.0; 4]);
                bonds.add_bond_with_positions(&a, &b, 0.15, &grey);
                for (di, dj, dk) in [(0, -1, -1), (-1, 0, -1), (-1, -1, 0)] {
                    let (ni, nj, nk) = (i + di, j + dj, k + dk);
                    if ni >= 0 && nj >= 0 && nk >= 0 {
                        let neighbour = at(2 * ni + 1, 2 * nj + 1, 2 * nk + 1);
                        bonds.add_bond_with_positions(&a, &neighbour, 0.15, &grey);
                    }
                }
            }
        }
    }
    let size = DVec3::new(mx as f64, my as f64, mz as f64) * (2.0 * quarter);
    (atoms, bonds, size)
}

/// Builds and uploads the lattice of `cells`, returning its size.
fn upload_lattice(renderer: &mut Renderer, cells: [i64; 3]) -> DVec3 {
    let build = Instant::now();
    let (atoms, bonds, size) = diamond_block(cells);
    println!(
        "renderer_gpu_timing: lattice of {} atoms, {} bonds, {:.1} x {:.1} x {:.1} Å, built in {:.1} s",
        atoms.vertices.len() / 4,
        bonds.vertices.len() / 4,
        size.x,
        size.y,
        size.z,
        build.elapsed().as_secs_f64()
    );
    upload(renderer, &Mesh::new(), &atoms, &bonds);
    size
}

fn upload(
    renderer: &mut Renderer,
    main: &Mesh,
    atoms: &AtomImpostorMesh,
    bonds: &BondImpostorMesh,
) {
    renderer.update_all_gpu_meshes(
        &Mesh::new(),
        &LineMesh::new(),
        main,
        &LineMesh::new(),
        &LineMesh::new(),
        atoms,
        bonds,
        &TransparentImpostorMesh::new(),
        &TransparentSurfaceMesh::new(),
        &LabelMesh::new(),
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
        true,
        Some((1, "Main".to_string())),
    );
}

/// `eye` looking at `target` with +Z as near up as the view allows.
fn look(renderer: &mut Renderer, eye: DVec3, target: DVec3) {
    let forward = (target - eye).normalize();
    let up = (DVec3::Z - forward * forward.dot(DVec3::Z)).normalize();
    renderer.move_camera(&eye, &target, &up);
}

/// The cascade count of the live view as it stands.
fn cascades(renderer: &Renderer) -> usize {
    shadow_setup(
        &renderer.camera,
        renderer.caster_bounds(),
        &renderer.shading(),
        &renderer.live_target_spec(),
    )
    .map_or(0, |setup| setup.cascades.len())
}

/// The camera poses of a moving sequence: `eye` orbiting `target` about
/// the vertical by [`ORBIT_STEP_RAD`] per frame.
fn orbit(eye: DVec3, target: DVec3, frames: usize) -> Vec<(DVec3, DVec3)> {
    (0..frames)
        .map(|n| {
            let turn = DQuat::from_rotation_z(n as f64 * ORBIT_STEP_RAD);
            (target + turn * (eye - target), target)
        })
        .collect()
}

fn mean_ms(times: &[Duration]) -> f64 {
    times.iter().map(Duration::as_secs_f64).sum::<f64>() * 1000.0 / times.len() as f64
}

fn median_ms(times: &[Duration]) -> f64 {
    let mut sorted: Vec<f64> = times.iter().map(|t| t.as_secs_f64() * 1000.0).collect();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

/// The mean and median of moving frames along `poses` (one new pose per
/// frame), with shadows at `strength`.
fn moving_frames(renderer: &mut Renderer, poses: &[(DVec3, DVec3)], strength: f32) -> (f64, f64) {
    renderer.set_shading(shading(strength));
    let mut times = Vec::new();
    for (n, &(eye, target)) in poses.iter().enumerate() {
        look(renderer, eye, target);
        let start = Instant::now();
        renderer.render(BACKGROUND);
        if n >= WARM_UP {
            times.push(start.elapsed());
        }
    }
    (mean_ms(&times), median_ms(&times))
}

/// The mean and median of the refined frame at rest at `pose`, with shadows
/// at `strength`: the shadow map is drawn once, before timing, and cached.
fn refined_frames(renderer: &mut Renderer, pose: (DVec3, DVec3), strength: f32) -> (f64, f64) {
    renderer.set_shading(shading(strength));
    look(renderer, pose.0, pose.1);
    let mut times = Vec::new();
    for n in 0..WARM_UP + FRAMES {
        let start = Instant::now();
        renderer.render_still(BACKGROUND);
        if n >= WARM_UP {
            times.push(start.elapsed());
        }
    }
    (mean_ms(&times), median_ms(&times))
}

/// Prints one row: off, on, and on over off.
fn report(what: &str, off: (f64, f64), on: (f64, f64)) {
    println!(
        "renderer_gpu_timing: {what:<38} off {:7.2} ms (median {:7.2}), on {:7.2} ms \
         (median {:7.2}), {:+6.1} % (median {:+6.1} %)",
        off.0,
        off.1,
        on.0,
        on.1,
        (on.0 / off.0 - 1.0) * 100.0,
        (on.1 / off.1 - 1.0) * 100.0
    );
}

/// The moving and the refined frame of the view at `pose`, shadows off and
/// on, after asserting that every moving pose has `expected` cascades.
fn measure(renderer: &mut Renderer, what: &str, pose: (DVec3, DVec3), expected: usize) {
    let poses = orbit(pose.0, pose.1, WARM_UP + FRAMES);
    renderer.set_shading(shading(1.0));
    for &(eye, target) in &poses {
        look(renderer, eye, target);
        assert_eq!(
            cascades(renderer),
            expected,
            "{what}: a moving pose has another cascade count"
        );
    }
    let off = moving_frames(renderer, &poses, 0.0);
    let on = moving_frames(renderer, &poses, 1.0);
    report(&format!("{what}, moving, {expected} cascade(s)"), off, on);
    let passes = renderer.shadow_passes_drawn();
    let off = refined_frames(renderer, pose, 0.0);
    let on = refined_frames(renderer, pose, 1.0);
    report(&format!("{what}, refined"), off, on);
    // The refined frames drew the map once (when shadows came back on).
    println!(
        "renderer_gpu_timing: {what}: {} caster passes for {} refined frames",
        renderer.shadow_passes_drawn() - passes,
        WARM_UP + FRAMES
    );
}

/// Phase 3's measurement: the 1 M-sphere lattice in full view (one
/// cascade) and in a close-up (four), and a ~100 k-triangle solid, at
/// 2560 × 1440 with 4× MSAA. Budget (D9, Phase 3), over shadows off on the
/// lattice: +60 % with one cascade, +150 % with four; recorded in the
/// design document's Phase 3 notes, not asserted.
#[test]
#[ignore = "needs a GPU and a release build; run with --release -- --ignored --nocapture"]
fn shadow_frame_cost() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    println!(
        "renderer_gpu_timing: {WIDTH} x {HEIGHT}, {}x MSAA, refined frames {}",
        renderer.live_sample_count(),
        if renderer.refinement_supported() {
            "supported"
        } else {
            "unsupported (render_still draws a normal frame)"
        }
    );

    let size = upload_lattice(&mut renderer, CUBE_CELLS);
    let centre = size / 2.0;
    // The whole cube in view, from above a corner.
    let full = (
        centre + DVec3::new(-1.0, -1.4, 1.1).normalize() * (size.x * 3.2),
        centre,
    );
    measure(&mut renderer, "1 M cube, full view", full, 1);

    // A close-up across the slab: the first of a few low eyes over a corner
    // whose whole orbit has four cascades.
    let size = upload_lattice(&mut renderer, SLAB_CELLS);
    renderer.set_shading(shading(1.0));
    let close_up = [2.0, 4.0, 8.0, 16.0, 32.0]
        .into_iter()
        .flat_map(|height| [0.1, 0.25, 0.5].map(|ahead| (height, ahead)))
        .map(|(height, ahead)| {
            let corner = DVec3::new(size.x * 0.05, size.y * 0.05, size.z);
            let eye = corner + DVec3::Z * height;
            let target = corner + DVec3::new(size.x, size.y, 0.0) * ahead;
            (eye, target)
        })
        .find(|&(eye, target)| {
            orbit(eye, target, WARM_UP + FRAMES)
                .into_iter()
                .all(|(eye, target)| {
                    look(&mut renderer, eye, target);
                    cascades(&renderer) == 4
                })
        })
        .expect("no close-up on the slab has four cascades");
    println!(
        "renderer_gpu_timing: close-up eye {:?}, target {:?}",
        close_up.0, close_up.1
    );
    measure(&mut renderer, "1 M slab, close-up", close_up, 4);
    // The slab in full view, for the four-cascade row's own baseline.
    let centre = size / 2.0;
    let slab_full = (
        centre + DVec3::new(-0.2, -0.3, 1.0).normalize() * 1800.0,
        centre,
    );
    renderer.set_shading(shading(1.0));
    look(&mut renderer, slab_full.0, slab_full.1);
    let count = cascades(&renderer);
    measure(&mut renderer, "1 M slab, full view", slab_full, count);

    // A solid of ~100 k triangles: a sphere of 224 × 224 sections.
    let mut solid = Mesh::new();
    tessellate_sphere(
        &mut solid,
        &DVec3::ZERO,
        40.0,
        224,
        224,
        &Material::new(&Vec3::new(0.8, 0.5, 0.3), 0.5, 0.0),
    );
    println!(
        "renderer_gpu_timing: solid of {} triangles",
        solid.indices.len() / 3
    );
    upload(
        &mut renderer,
        &solid,
        &AtomImpostorMesh::new(),
        &BondImpostorMesh::new(),
    );
    let view = (DVec3::new(-0.8, -1.2, 0.9).normalize() * 220.0, DVec3::ZERO);
    measure(&mut renderer, "100 k-triangle solid", view, 1);
}
