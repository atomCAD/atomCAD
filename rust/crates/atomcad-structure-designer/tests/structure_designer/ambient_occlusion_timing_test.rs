//! The cost of one full baked-AO computation against the evaluation that
//! produced the structure (`doc/design_depth_perception.md` D7, Phase 4).
//!
//! D7's budget: one full `atom_exposure` over every receiver takes at most
//! 15 % of the evaluate time of the same structure. Timing is never
//! asserted, so this test only prints; it is `#[ignore]`d and meant for a
//! release build:
//!
//! ```text
//! cd rust
//! cargo test --release -p atomcad-structure-designer --test structure_designer \
//!     ambient_occlusion_timing -- --ignored --nocapture
//! ```
//!
//! It lives in this harness because lattice fill does, and the numbers are
//! recorded in the design document's phase notes.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_display::ambient_occlusion::atom_exposure;
use atomcad_display::atomic_tessellator::ao_receivers;
use atomcad_display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
};
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_scene::NodeOutput;
use std::time::Instant;

fn visualization(cull_depth: Option<f64>) -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::BallAndStick,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: cull_depth,
        space_filling_cull_depth: cull_depth,
        scene_transparency_enabled: false,
        scene_alpha: 1.0,
        label_scale: 0.7,
        show_tool_envelopes: false,
        tool_envelope_color: [0, 0, 0],
    }
}

/// A lattice-filled, passivated diamond cuboid of `extent` cells, evaluated
/// cold: the structure and the evaluate time in ms.
fn evaluate_cuboid(extent: (i32, i32, i32)) -> (AtomicStructure, f64) {
    let mut designer = StructureDesigner::new();
    designer.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let (x, y, z) = extent;
    let outcome = designer.ai_text_edit(
        &format!(
            "c = cuboid {{ extent: ({x}, {y}, {z}) }}\nm = materialize {{ shape: c, visible: true }}\noutput m\n"
        ),
        false,
    );
    assert!(outcome.result.success, "{:?}", outcome.result.errors);

    let start = Instant::now();
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
    let evaluate_ms = start.elapsed().as_secs_f64() * 1000.0;

    let mut merged = AtomicStructure::new();
    for node_data in designer
        .last_generated_structure_designer_scene
        .node_data
        .values()
    {
        for (_, output, _) in node_data.displayed_outputs() {
            if let NodeOutput::Atomic(structure, _) = output {
                merged.add_atomic_structure(structure).unwrap();
            }
        }
    }
    assert!(merged.get_num_of_atoms() > 0, "nothing was materialized");
    (merged, evaluate_ms)
}

fn measure(name: &str, extent: (i32, i32, i32)) {
    let (structure, evaluate_ms) = evaluate_cuboid(extent);
    println!(
        "{name}: {} atoms, evaluate {evaluate_ms:.0} ms",
        structure.get_num_of_atoms()
    );
    for (label, cull_depth) in [("cull 8 Å", Some(8.0)), ("no culling", None)] {
        let receivers = ao_receivers(&structure, &visualization(cull_depth));
        let start = Instant::now();
        let (values, _) = atom_exposure(&structure, &receivers);
        let exposure_ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(values.len(), receivers.len());
        println!(
            "  {label}: {} receivers, atom_exposure {exposure_ms:.0} ms = {:.1} % of evaluate",
            receivers.len(),
            100.0 * exposure_ms / evaluate_ms
        );
    }
}

#[test]
#[ignore = "a timing measurement; run in release with --ignored --nocapture"]
fn ambient_occlusion_timing() {
    // The worked case of D7: about 178 Å on a side, about a million atoms.
    measure("1 M-atom cuboid", (50, 50, 50));
    // The 80-cell rod of the rendering report's shots 60–62.
    measure("80-cell rod", (80, 7, 7));
}
