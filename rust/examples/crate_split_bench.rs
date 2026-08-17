//! Runtime benchmark harness for the crate-split refactor
//! (`doc/design_rust_crate_split.md`, D13).
//!
//! The refactor's one plausible way of making the *product* worse is that
//! module boundaries become **crate** boundaries, and rustc can no longer
//! inline freely across them. `util`, `geo_tree` and `crystolecule` between
//! them expose ~480 `pub fn` with almost no `#[inline]`, and the per-atom
//! accessors are called in loops that run over structures of >10^6 atoms.
//!
//! D13 therefore makes a runtime measurement a phase gate: capture a baseline
//! in **Phase 0** (before any code moves, but *with* `[profile.release]
//! lto = "thin"` already in place, so the two ends of the comparison share a
//! profile) and re-measure after **Phase 5**, at which point `util`,
//! `geo_tree`, `renderer`, `crystolecule` and `display` have all left the root
//! crate and the cross-crate exposure is at its maximum.
//!
//! ```text
//! cd rust
//! cargo run --release --example crate_split_bench -- <file.cnnd> <network> [reps]
//! ```
//!
//! It reports three wall-clock numbers per repetition:
//!
//! - **load** — `.cnnd` parse + network validation (serde / structure_designer).
//! - **evaluate** — a full refresh of the active network: geo_tree SDF and CSG
//!   evaluation plus lattice materialisation. This is D13's "CSG evaluation".
//! - **tessellate** — turning the resulting `AtomicStructure` into GPU meshes,
//!   which is where `display` hammers `crystolecule`'s per-atom accessors.
//!   Impostor tessellation always runs; the (far heavier) triangle-mesh path
//!   runs only below `TRIANGLE_MESH_ATOM_LIMIT`, because a million-atom
//!   ball-and-stick mesh is tens of gigabytes of vertices and measures the
//!   allocator rather than the code under test.
//!
//! Each repetition uses a **fresh** `StructureDesigner`, so evaluation is cold
//! every time and the geometry caches never carry a result between reps.
//!
//! This file is a measurement tool, not part of the shipping binary: examples
//! are not linked into the cdylib.

use std::time::{Duration, Instant};

use atomcad_renderer::atom_impostor_mesh::AtomImpostorMesh;
use atomcad_renderer::bond_impostor_mesh::BondImpostorMesh;
use atomcad_renderer::mesh::Mesh;
use atomcad_renderer::transparent_impostor_mesh::TransparentImpostorMesh;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::display::atomic_tessellator::{
    AtomicTessellatorParams, tessellate_atomic_structure, tessellate_atomic_structure_impostors,
};
use rust_lib_flutter_cad::display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::structure_designer_scene::NodeOutput;

/// Above this atom count the triangle-mesh tessellation is skipped: at 12x6
/// sphere divisions a ball-and-stick mesh costs ~62 vertices per atom, so a
/// million-atom structure would need tens of GB and would measure the
/// allocator, not the tessellator.
const TRIANGLE_MESH_ATOM_LIMIT: usize = 250_000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!(
            "usage: cargo run --release --example crate_split_bench -- <file.cnnd> <network> [reps]"
        );
        std::process::exit(2);
    }
    let cnnd_file = &args[0];
    let network_name = &args[1];
    let reps: usize = args.get(2).map(|s| s.parse().unwrap_or(3)).unwrap_or(3);

    println!("file    : {cnnd_file}");
    println!("network : {network_name}");
    println!("reps    : {reps}");
    println!();

    let mut load_times = Vec::new();
    let mut eval_times = Vec::new();
    let mut impostor_times = Vec::new();
    let mut triangle_times = Vec::new();
    let mut reported_size = None;

    for rep in 0..reps {
        // A fresh designer per repetition: evaluation must be cold, or the
        // geometry cache turns rep 2+ into a measurement of a HashMap lookup.
        let mut designer = StructureDesigner::new();

        let t = Instant::now();
        designer
            .load_node_networks(cnnd_file)
            .unwrap_or_else(|e| panic!("failed to load '{cnnd_file}': {e}"));
        let load = t.elapsed();

        designer.active_node_network_name = Some(network_name.to_string());

        let t = Instant::now();
        designer.mark_full_refresh();
        let changes = designer.get_pending_changes();
        designer.refresh(&changes);
        let evaluate = t.elapsed();

        let structure = merge_displayed_atomic_structures(&designer);
        let atoms = structure.get_num_of_atoms();
        let bonds = structure.get_num_of_bonds();
        if reported_size.is_none() {
            reported_size = Some((atoms, bonds));
        }

        let viz_prefs = bench_visualization_preferences();

        let t = Instant::now();
        let mut atom_impostors = AtomImpostorMesh::new();
        let mut bond_impostors = BondImpostorMesh::new();
        let mut transparent_impostors = TransparentImpostorMesh::new();
        tessellate_atomic_structure_impostors(
            &mut atom_impostors,
            &mut bond_impostors,
            &mut transparent_impostors,
            &structure,
            &viz_prefs,
        );
        let impostor = t.elapsed();

        let triangle = if atoms <= TRIANGLE_MESH_ATOM_LIMIT {
            let t = Instant::now();
            let mut mesh = Mesh::new();
            tessellate_atomic_structure(
                &mut mesh,
                &structure,
                &bench_tessellator_params(),
                &viz_prefs,
            );
            Some(t.elapsed())
        } else {
            None
        };

        println!(
            "rep {rep}: atoms={atoms} bonds={bonds} load={} evaluate={} impostors={} triangles={}",
            ms(load),
            ms(evaluate),
            ms(impostor),
            triangle.map(ms).unwrap_or_else(|| "skipped".to_string()),
        );

        load_times.push(load);
        eval_times.push(evaluate);
        impostor_times.push(impostor);
        if let Some(d) = triangle {
            triangle_times.push(d);
        }
    }

    let (atoms, bonds) = reported_size.unwrap_or((0, 0));
    println!();
    println!("=== summary ({atoms} atoms, {bonds} bonds, {reps} reps) ===");
    report("load       ", &load_times);
    report("evaluate   ", &eval_times);
    report("impostors  ", &impostor_times);
    if triangle_times.is_empty() {
        println!(
            "triangles  : skipped (> {TRIANGLE_MESH_ATOM_LIMIT} atoms would measure the allocator)"
        );
    } else {
        report("triangles  ", &triangle_times);
    }
}

/// Same merge the CLI export path performs: every displayed `Atomic` output of
/// every scene node, concatenated into one structure.
fn merge_displayed_atomic_structures(designer: &StructureDesigner) -> AtomicStructure {
    let mut merged = AtomicStructure::new();
    for node_data in designer
        .last_generated_structure_designer_scene
        .node_data
        .values()
    {
        for (_pin_index, pin_output, _pin_geo_tree) in node_data.displayed_outputs() {
            if let NodeOutput::Atomic(atomic_structure, _) = pin_output {
                merged
                    .add_atomic_structure(atomic_structure)
                    .expect("failed to merge atomic structure");
            }
        }
    }
    merged
}

/// The values `display::scene_tessellator` uses at runtime, so the benchmark
/// exercises the same code paths as the viewport.
fn bench_tessellator_params() -> AtomicTessellatorParams {
    AtomicTessellatorParams {
        ball_and_stick_sphere_horizontal_divisions: 12,
        ball_and_stick_sphere_vertical_divisions: 6,
        space_filling_sphere_horizontal_divisions: 36,
        space_filling_sphere_vertical_divisions: 18,
        cylinder_divisions: 12,
    }
}

fn bench_visualization_preferences() -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::BallAndStick,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: None,
        space_filling_cull_depth: None,
        scene_transparency_enabled: false,
        scene_alpha: 1.0,
        label_scale: 0.0,
    }
}

fn report(label: &str, times: &[Duration]) {
    if times.is_empty() {
        return;
    }
    let min = times.iter().min().unwrap();
    let total: Duration = times.iter().sum();
    let mean = total / times.len() as u32;
    println!("{label}: min {} | mean {}", ms(*min), ms(mean));
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}
