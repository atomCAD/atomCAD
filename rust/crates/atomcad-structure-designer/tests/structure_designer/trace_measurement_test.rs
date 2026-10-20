//! The Phase 1 measurement of `doc/design_output_driven_editing.md`: trace time
//! and memory on the demo library (`samples/*.cnnd`). Prints a table and
//! asserts nothing, like `renderer_gpu_timing.rs`; run it in release:
//!
//! ```text
//! cargo test --release -p atomcad-structure-designer --test structure_designer \
//!     trace_measurement -- --ignored --nocapture
//! ```
//!
//! For every network of every sample, every top-level displayed node is
//! traced once from a ray through the middle of its content: aimed at the atom
//! nearest the centroid for atomic output, else along the six axes through the
//! origin until one hits its geometry.

use std::time::Instant;

use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_changes::{
    RefreshMode, StructureDesignerChanges,
};
use atomcad_structure_designer::structure_designer_scene::NodeOutput;
use glam::f64::DVec3;

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// A ray (origin, direction) through the middle of `node`'s displayed content.
fn ray_for(
    designer: &StructureDesigner,
    node_ref: &atomcad_structure_designer::node_network::NodeRef,
) -> Vec<(DVec3, DVec3)> {
    let scene = &designer.last_generated_structure_designer_scene;
    let Some(data) = scene.node_data.get(node_ref) else {
        return vec![];
    };
    for (_pin, output, _geo) in data.displayed_outputs() {
        if let NodeOutput::Atomic(atoms, _) = output {
            let count = atoms.atoms_values().count();
            if count == 0 {
                continue;
            }
            let centroid = atoms.atoms_values().map(|a| a.position).sum::<DVec3>() / count as f64;
            let target = atoms
                .atoms_values()
                .min_by(|a, b| {
                    a.position
                        .distance(centroid)
                        .total_cmp(&b.position.distance(centroid))
                })
                .unwrap()
                .position;
            return vec![(target - DVec3::Z * 1000.0, DVec3::Z)];
        }
    }
    [
        DVec3::X,
        DVec3::Y,
        DVec3::Z,
        -DVec3::X,
        -DVec3::Y,
        -DVec3::Z,
    ]
    .into_iter()
    .map(|d| (-d * 1000.0, d))
    .collect()
}

#[test]
#[ignore = "measurement; run in release with --ignored --nocapture"]
fn trace_measurement() {
    let mut samples: Vec<_> = std::fs::read_dir(atomcad_test_support::sample_path(""))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "cnnd"))
        .collect();
    samples.sort();

    println!(
        "{:<44} {:<28} {:>5} {:>5} {:>9} {:>9} {:>9} {:>9}",
        "sample", "network / node", "rows", "fills", "ms", "refresh", "memo MB", "store MB"
    );
    for sample in samples {
        let mut designer = StructureDesigner::new();
        if designer
            .load_node_networks(sample.to_str().unwrap())
            .is_err()
        {
            println!("{}: failed to load", sample.display());
            continue;
        }
        let mut networks: Vec<String> = designer
            .node_type_registry
            .node_networks
            .keys()
            .cloned()
            .collect();
        networks.sort();
        for network in networks {
            designer.set_active_node_network_name(Some(network.clone()));
            let started = Instant::now();
            designer.refresh(&StructureDesignerChanges {
                mode: RefreshMode::Full,
                ..Default::default()
            });
            let refresh_ms = started.elapsed().as_secs_f64() * 1e3;
            let mut node_refs: Vec<_> = designer
                .last_generated_structure_designer_scene
                .node_data
                .keys()
                .filter(|r| r.is_top_level())
                .cloned()
                .collect();
            node_refs.sort_by_key(|r| r.node_id);
            for node_ref in node_refs {
                let hit = ray_for(&designer, &node_ref)
                    .into_iter()
                    .find_map(|(o, d)| {
                        designer
                            .raytrace_per_node(
                                &o,
                                &d,
                                &designer
                                    .preferences
                                    .atomic_structure_visualization_preferences
                                    .visualization,
                            )
                            .into_iter()
                            .find(|h| h.node_ref == node_ref)
                    });
                let Some(hit) = hit else {
                    continue;
                };
                let started = Instant::now();
                let report = designer.trace_hit(&hit.trace_hit());
                let ms = started.elapsed().as_secs_f64() * 1e3;
                let rows = count_rows(&report.rows);
                println!(
                    "{:<44} {:<28} {:>5} {:>5} {:>9.1} {:>9.1} {:>9.2} {:>9.2}",
                    sample.file_name().unwrap().to_string_lossy(),
                    format!("{} / #{}", network, node_ref.node_id),
                    rows,
                    report.stats.store_fills,
                    ms,
                    refresh_ms,
                    mb(report.stats.memo.peak_bytes),
                    mb(report.stats.store_bytes),
                );
            }
        }
    }
}

fn count_rows(rows: &[atomcad_structure_designer::trace::TraceNode]) -> usize {
    rows.iter().map(|r| 1 + count_rows(&r.children)).sum()
}
