//! The baked-AO exposure cache on the scene (`doc/design_depth_perception.md`
//! D9, Phase 4b).
//!
//! Each test drives `refresh` followed by `update_exposure`, the sequence
//! `refresh_structure_designer` runs. Work is observed, never inferred from
//! timing: "computes nothing" reads `last_exposure_stats`, and "re-evaluated
//! nothing" is a refresh whose armed per-node profiler produced no table (a
//! partial refresh that finds no work reports `eval_ms == Some(0.0)`, not
//! `None`, so the profiler is the observable).
//!
//! Every new preference is set explicitly: the designer reads the user's
//! `preferences.json`.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::SelectModifier;
use atomcad_display::ambient_occlusion::{AtomExposure, R_MAX, atom_exposure, occluder_radius};
use atomcad_display::atomic_tessellator::ao_receivers;
use atomcad_display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
    BackgroundPreferences, DisplayPreferences, GeometryVisualizationPreferences, MeshSmoothing,
};
use atomcad_renderer::atom_impostor_mesh::FULLY_EXPOSED;
use atomcad_renderer::camera::Camera;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, AtomEditEvalCache, default_tool_pointer_down, default_tool_pointer_move,
    default_tool_pointer_up, drag_selected_by_delta, with_atom_edit_undo,
};
use atomcad_structure_designer::nodes::cuboid::CuboidData;
use atomcad_structure_designer::nodes::structure_move::StructureMoveData;
use atomcad_structure_designer::preferences::{
    AtomicRenderingMethod as DomainRenderingMethod, NodeDisplayPolicy,
};
use atomcad_structure_designer::refresh_profile::RefreshSubPhases;
use atomcad_structure_designer::scene_exposure::SceneExposureStats;
use atomcad_structure_designer::scene_tessellator::tessellate_scene_content;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_scene::NodeOutput;
use atomcad_util::transform::Transform;
use glam::f64::{DMat4, DQuat, DVec2, DVec3};
use glam::i32::IVec3;

// ============================================================================
// Fixture
// ============================================================================

/// The display preferences `update_exposure` and the tessellator receive.
fn display_prefs(ambient_occlusion: bool, cull_depth: Option<f64>) -> DisplayPreferences {
    DisplayPreferences {
        geometry_visualization: GeometryVisualizationPreferences {
            wireframe_geometry: false,
            mesh_smoothing: MeshSmoothing::Smooth,
            display_camera_target: false,
            wireframe_active_color: [1.0, 1.0, 1.0],
            wireframe_inactive_color: [0.5, 0.5, 0.5],
            hide_coplanar_edges: false,
        },
        atomic_structure_visualization: AtomicStructureVisualizationPreferences {
            visualization: AtomicStructureVisualization::BallAndStick,
            rendering_method: AtomicRenderingMethod::Impostors,
            ball_and_stick_cull_depth: cull_depth,
            space_filling_cull_depth: cull_depth,
            scene_transparency_enabled: false,
            scene_alpha: 1.0,
            label_scale: 0.7,
            show_tool_envelopes: false,
            tool_envelope_color: [0, 0, 0],
            ambient_occlusion,
        },
        background: BackgroundPreferences {
            show_axes: false,
            show_grid: false,
            grid_size: 10,
            grid_color: [0, 0, 0],
            grid_strong_color: [0, 0, 0],
            show_lattice_axes: false,
            show_lattice_grid: false,
            lattice_grid_color: [0, 0, 0],
            lattice_grid_strong_color: [0, 0, 0],
            drawing_plane_grid_color: [0, 0, 0],
            drawing_plane_grid_strong_color: [0, 0, 0],
            unit_cell_wireframe_color: [0, 0, 0],
        },
    }
}

fn set_node_data(designer: &mut StructureDesigner, node_id: u64, data: Box<dyn NodeData>) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut("main").unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// A designer with every preference this design reads set explicitly.
fn designer(ambient_occlusion: bool) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    designer.preferences.rendering_preferences.ambient_occlusion = ambient_occlusion;
    designer
        .preferences
        .rendering_preferences
        .ambient_occlusion_strength = 1.0;
    designer
        .preferences
        .atomic_structure_visualization_preferences
        .rendering_method = DomainRenderingMethod::Impostors;
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    designer
}

/// `cuboid(extent) → materialize → <consumer>` with the consumer displayed
/// and selected; returns the consumer's id.
fn add_lattice_chain(designer: &mut StructureDesigner, extent: i32, consumer: &str) -> u64 {
    let cuboid = designer.add_node("cuboid", DVec2::new(0.0, 0.0));
    set_node_data(
        designer,
        cuboid,
        Box::new(CuboidData {
            min_corner: IVec3::ZERO,
            extent: IVec3::splat(extent),
            subdivision: 1,
        }),
    );
    let materialize = designer.add_node("materialize", DVec2::new(150.0, 0.0));
    designer.connect_nodes(cuboid, 0, materialize, 0);
    let consumer_id = designer.add_node(consumer, DVec2::new(300.0, 0.0));
    designer.connect_nodes(materialize, 0, consumer_id, 0);
    designer.validate_active_network();
    // Only the consumer is displayed: an added node may come up displayed.
    designer.set_node_display(cuboid, false);
    designer.set_node_display(materialize, false);
    designer.set_node_display(consumer_id, true);
    designer.select_node(consumer_id);
    consumer_id
}

/// The usual network: a lattice-filled block of `extent` cells edited by
/// `atom_edit`, plus an unrelated, displayed `cuboid` (geometry, no atoms).
/// Fully refreshed and with its exposure filled. Returns
/// `(designer, atom_edit id, cuboid id)`.
fn atom_edit_network_of(extent: i32, prefs: &DisplayPreferences) -> (StructureDesigner, u64, u64) {
    let mut designer = designer(prefs.atomic_structure_visualization.ambient_occlusion);
    let atom_edit = add_lattice_chain(&mut designer, extent, "atom_edit");
    let unrelated = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    designer.set_node_display(unrelated, true);
    designer.select_node(atom_edit);
    designer.mark_full_refresh();
    refresh(&mut designer, prefs);
    (designer, atom_edit, unrelated)
}

fn atom_edit_network(prefs: &DisplayPreferences) -> (StructureDesigner, u64, u64) {
    atom_edit_network_of(3, prefs)
}

/// One `refresh_structure_designer`, minus tessellation and upload: the
/// pending changes, `refresh`, then `update_exposure`. The per-node profiler
/// is armed, so the returned sub-phases say whether anything was evaluated.
fn refresh(designer: &mut StructureDesigner, prefs: &DisplayPreferences) -> RefreshSubPhases {
    designer.eval_profiling_enabled = true;
    let changes = designer.get_pending_changes();
    let mut sub_phases = designer.refresh(&changes);
    sub_phases.exposure_ms = designer.update_exposure(prefs, changes.is_lightweight());
    sub_phases
}

fn evaluated_nothing(sub_phases: &RefreshSubPhases) -> bool {
    sub_phases.node_stats.is_none()
}

fn stats(designer: &StructureDesigner) -> SceneExposureStats {
    designer.last_exposure_stats
}

fn structure(designer: &StructureDesigner, node_id: u64) -> &AtomicStructure {
    match &designer.last_generated_structure_designer_scene.node_data[&NodeRef::top(node_id)].output
    {
        NodeOutput::Atomic(structure, _) => structure,
        _ => panic!("node {node_id} has no atomic output"),
    }
}

fn cached(designer: &StructureDesigner, node_id: u64) -> Option<&AtomExposure> {
    designer.last_generated_structure_designer_scene.node_data[&NodeRef::top(node_id)]
        .exposure
        .values(0)
}

fn pending(designer: &StructureDesigner, node_id: u64) -> usize {
    designer.last_generated_structure_designer_scene.node_data[&NodeRef::top(node_id)]
        .exposure
        .pending_change_count(0)
}

/// What a full computation on the node's current structure gives.
fn expected(
    designer: &StructureDesigner,
    node_id: u64,
    prefs: &DisplayPreferences,
) -> AtomExposure {
    let structure = structure(designer, node_id);
    let receivers = ao_receivers(structure, &prefs.atomic_structure_visualization);
    atom_exposure(structure, &receivers).0
}

fn assert_cache_is_exact(designer: &StructureDesigner, node_id: u64, prefs: &DisplayPreferences) {
    let cached = cached(designer, node_id).expect("the cache has values");
    assert!(
        *cached == expected(designer, node_id, prefs),
        "the cache differs from a full computation on the current structure"
    );
}

fn atom_edit_data_mut(designer: &mut StructureDesigner, node_id: u64) -> &mut AtomEditData {
    // Through the designer's own accessor, so the node is marked changed as an
    // API mutation would mark it.
    designer
        .get_node_network_data_mut(node_id)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<AtomEditData>()
        .unwrap()
}

/// Selects the result atoms `ids` of the active atom_edit node, by their base
/// provenance, and sets the selection transform at their centroid.
fn select_result_atoms(designer: &mut StructureDesigner, node_id: u64, ids: &[u32]) {
    let base_ids: Vec<u32> = {
        let cache = designer
            .get_selected_node_eval_cache()
            .unwrap()
            .downcast_ref::<AtomEditEvalCache>()
            .unwrap();
        ids.iter()
            .map(|result_id| {
                *cache
                    .provenance
                    .base_to_result
                    .iter()
                    .find(|(_, r)| *r == result_id)
                    .expect("an unedited atom comes from the base")
                    .0
            })
            .collect()
    };
    let centroid = {
        let structure = structure(designer, node_id);
        ids.iter()
            .map(|id| structure.get_atom(*id).unwrap().position)
            .sum::<DVec3>()
            / ids.len() as f64
    };
    let data = atom_edit_data_mut(designer, node_id);
    data.selection.clear();
    data.selection.selected_base_atoms.extend(base_ids);
    data.selection.selection_transform = Some(Transform::new(centroid, DQuat::IDENTITY));
}

// ============================================================================
// Tests
// ============================================================================

/// The fixture is meaningful: its first refresh computed every receiver of
/// the block, and the values equal a full computation.
#[test]
fn the_first_refresh_computes_every_receiver() {
    let prefs = display_prefs(true, None);
    let (designer, atom_edit, _) = atom_edit_network(&prefs);
    let receivers = ao_receivers(
        structure(&designer, atom_edit),
        &prefs.atomic_structure_visualization,
    );
    assert!(receivers.len() > 100, "{} receivers", receivers.len());
    assert_eq!(stats(&designer).receivers_computed, receivers.len());
    assert_eq!(stats(&designer).full_path_updates, 1);
    assert_cache_is_exact(&designer, atom_edit, &prefs);
}

/// The gating inside `update_exposure`: a lightweight refresh computes
/// nothing and reports no exposure time.
#[test]
fn a_lightweight_refresh_computes_nothing() {
    let prefs = display_prefs(true, None);
    let (mut designer, atom_edit, _) = atom_edit_network(&prefs);
    // A structural change the lightweight refresh must not act on.
    designer.mark_node_data_changed(atom_edit);
    designer.mark_lightweight_refresh();
    let sub_phases = refresh(&mut designer, &prefs);
    assert_eq!(sub_phases.exposure_ms, None);
    assert_eq!(stats(&designer), SceneExposureStats::default());
}

#[test]
fn editing_an_unrelated_node_computes_nothing() {
    let prefs = display_prefs(true, None);
    let (mut designer, _, unrelated) = atom_edit_network(&prefs);
    designer.mark_node_data_changed(unrelated);
    let sub_phases = refresh(&mut designer, &prefs);
    assert!(
        !evaluated_nothing(&sub_phases),
        "the unrelated node re-evaluates"
    );
    assert!(sub_phases.exposure_ms.is_some());
    assert_eq!(stats(&designer).receivers_computed, 0);
}

/// Selecting another node re-evaluates the old and the new selection; the
/// atom_edit entry is replaced, and its exposure is carried over with an
/// empty diff. Mutation: drop `carry_over_exposure` from `refresh_partial`.
#[test]
fn selecting_another_node_computes_nothing() {
    let prefs = display_prefs(true, None);
    let (mut designer, _, unrelated) = atom_edit_network(&prefs);
    designer.select_node(unrelated);
    let sub_phases = refresh(&mut designer, &prefs);
    assert!(
        !evaluated_nothing(&sub_phases),
        "both selections re-evaluate"
    );
    assert_eq!(
        stats(&designer).diffs_run,
        1,
        "the atom_edit entry was carried"
    );
    assert_eq!(stats(&designer).receivers_computed, 0);
}

#[test]
fn an_atom_edit_selection_change_computes_nothing() {
    let prefs = display_prefs(true, None);
    let (mut designer, atom_edit, _) = atom_edit_network(&prefs);
    let some_atom = structure(&designer, atom_edit)
        .atoms_values()
        .next()
        .unwrap()
        .id;
    select_result_atoms(&mut designer, atom_edit, &[some_atom]);
    let sub_phases = refresh(&mut designer, &prefs);
    assert!(
        !evaluated_nothing(&sub_phases),
        "the atom_edit node re-evaluates"
    );
    assert_eq!(stats(&designer).diffs_run, 1);
    assert_eq!(stats(&designer).receivers_computed, 0);
    assert_cache_is_exact(&designer, atom_edit, &prefs);
}

/// Moving one atom recomputes exactly the receivers within `R_MAX` plus its
/// occluder radius of its old or its new position, and the result equals a
/// full computation — although the move renumbers most of the atoms.
/// Mutation: diff by id (`occlusion_changes`) in `carry_over_exposure`.
#[test]
fn moving_one_atom_computes_only_the_receivers_near_it() {
    let prefs = display_prefs(true, None);
    // Large enough that one atom's neighbourhood stays under the full-path
    // limit of a quarter of the receivers.
    let (mut designer, atom_edit, _) = atom_edit_network_of(5, &prefs);
    let (moved, old_position, radius) = {
        let structure = structure(&designer, atom_edit);
        // An atom near the middle, so its neighbourhood is a real subset.
        let centre = structure.atoms_values().map(|a| a.position).sum::<DVec3>()
            / structure.get_num_of_atoms() as f64;
        let atom = structure
            .atoms_values()
            .min_by(|a, b| {
                a.position
                    .distance_squared(centre)
                    .total_cmp(&b.position.distance_squared(centre))
            })
            .unwrap();
        (atom.id, atom.position, occluder_radius(atom))
    };
    select_result_atoms(&mut designer, atom_edit, &[moved]);
    refresh(&mut designer, &prefs);

    let delta = DVec3::new(0.5, 0.0, 0.0);
    let ids_before: Vec<(u32, DVec3)> = structure(&designer, atom_edit)
        .atoms_values()
        .map(|a| (a.id, a.position))
        .collect();
    with_atom_edit_undo(&mut designer, "Move atom", |designer| {
        drag_selected_by_delta(designer, delta);
    });
    refresh(&mut designer, &prefs);
    // `apply_diff` renumbers the result once its diff changes, so most
    // unmoved atoms come back under another id: the diff must match atoms
    // by place, or this move would recompute everything.
    let renumbered = {
        let structure = structure(&designer, atom_edit);
        ids_before
            .iter()
            .filter(|(id, p)| structure.get_atom(*id).is_none_or(|a| a.position != *p))
            .count()
    };
    assert!(renumbered > 100, "{renumbered} ids changed place");

    let new_position = old_position + delta;
    let structure = structure(&designer, atom_edit);
    let receivers = ao_receivers(structure, &prefs.atomic_structure_visualization);
    let reach = R_MAX + radius;
    let near = receivers
        .iter()
        .filter(|id| {
            let p = structure.get_atom(**id).unwrap().position;
            p.distance(old_position) <= reach || p.distance(new_position) <= reach
        })
        .count();
    assert!(
        near < receivers.len() / 4,
        "{near} of {} receivers",
        receivers.len()
    );
    assert_eq!(stats(&designer).receivers_computed, near);
    assert_eq!(stats(&designer).full_path_updates, 0);
    assert_cache_is_exact(&designer, atom_edit, &prefs);
}

/// A cull-depth change needs no special case: the receivers it uncovers have
/// no value and are computed, and nothing else is.
#[test]
fn a_changed_cull_depth_computes_only_the_newly_unculled_receivers() {
    let culled = display_prefs(true, Some(2.0));
    let (mut designer, atom_edit, _) = atom_edit_network(&culled);
    let unculled = display_prefs(true, None);
    let (before, after) = {
        let structure = structure(&designer, atom_edit);
        (
            ao_receivers(structure, &culled.atomic_structure_visualization).len(),
            ao_receivers(structure, &unculled.atomic_structure_visualization).len(),
        )
    };
    assert!(
        after > before,
        "culling must hide some atoms: {before} vs {after}"
    );
    let sub_phases = refresh(&mut designer, &unculled);
    assert!(evaluated_nothing(&sub_phases));
    assert_eq!(stats(&designer).receivers_computed, after - before);
    assert_cache_is_exact(&designer, atom_edit, &unculled);
}

/// AO off costs nothing: no exposure is computed, no entry is carried over
/// (so no occluder diff runs), and the pass reports no time. Switching it
/// back on through `set_preferences` computes on the next refresh and
/// re-evaluates nothing.
#[test]
fn ambient_occlusion_off_computes_and_carries_nothing() {
    let off = display_prefs(false, None);
    let (mut designer, atom_edit, _) = atom_edit_network(&off);
    assert_eq!(stats(&designer), SceneExposureStats::default());
    assert!(cached(&designer, atom_edit).is_none());

    // An edit that would be carried over with AO on.
    let some_atom = structure(&designer, atom_edit)
        .atoms_values()
        .next()
        .unwrap()
        .id;
    select_result_atoms(&mut designer, atom_edit, &[some_atom]);
    let sub_phases = refresh(&mut designer, &off);
    assert!(!evaluated_nothing(&sub_phases));
    assert_eq!(sub_phases.exposure_ms, None);
    assert_eq!(stats(&designer), SceneExposureStats::default());

    let mut preferences = designer.preferences.clone();
    preferences.rendering_preferences.ambient_occlusion = true;
    designer.set_preferences(preferences);
    let on = display_prefs(true, None);
    let sub_phases = refresh(&mut designer, &on);
    assert!(
        evaluated_nothing(&sub_phases),
        "changing the Rendering preferences must not re-evaluate"
    );
    assert!(stats(&designer).receivers_computed > 0);
    assert_cache_is_exact(&designer, atom_edit, &on);
}

/// A node restored from the invisible-node cache keeps its exposure.
#[test]
fn hide_then_show_restores_the_exposure() {
    let prefs = display_prefs(true, None);
    let (mut designer, atom_edit, _) = atom_edit_network(&prefs);
    designer.set_node_display(atom_edit, false);
    refresh(&mut designer, &prefs);
    designer.set_node_display(atom_edit, true);
    let sub_phases = refresh(&mut designer, &prefs);
    assert!(evaluated_nothing(&sub_phases), "restored from the cache");
    assert_eq!(stats(&designer).receivers_computed, 0);
    assert_cache_is_exact(&designer, atom_edit, &prefs);
}

/// A full refresh (what an undo of a structural command runs) takes the
/// exposure out of the outgoing scene and hands it to the new entries.
/// Mutation: drop `carry_over_exposure` from `refresh_full`.
#[test]
fn a_full_refresh_with_an_empty_diff_computes_nothing() {
    let prefs = display_prefs(true, None);
    let (mut designer, atom_edit, _) = atom_edit_network(&prefs);
    designer.mark_full_refresh();
    let sub_phases = refresh(&mut designer, &prefs);
    assert!(!evaluated_nothing(&sub_phases));
    assert_eq!(stats(&designer).diffs_run, 1);
    assert_eq!(stats(&designer).receivers_computed, 0);
    assert_cache_is_exact(&designer, atom_edit, &prefs);
}

/// A large atom_edit drag through the Default tool's pointer entry points:
/// every tick re-evaluates the node with the interaction open, and since the
/// update would take the full path it is deferred, with the changes kept
/// pending. The refresh after `pointer_up` runs with the interaction closed
/// and completes the update; the cache then equals a full computation on the
/// final structure.
#[test]
fn a_large_atom_drag_defers_its_update_to_the_drag_end() {
    let prefs = display_prefs(true, None);
    let (mut designer, atom_edit, _) = atom_edit_network(&prefs);

    // The upper half of the block: more than FULL_PATH_FRACTION of it.
    let (selected, grab) = {
        let structure = structure(&designer, atom_edit);
        let mid_z = structure.atoms_values().map(|a| a.position.z).sum::<f64>()
            / structure.get_num_of_atoms() as f64;
        let selected: Vec<u32> = structure
            .atoms_values()
            .filter(|a| a.position.z > mid_z)
            .map(|a| a.id)
            .collect();
        let top = structure
            .atoms_values()
            .max_by(|a, b| a.position.z.total_cmp(&b.position.z))
            .unwrap()
            .position;
        (selected, top)
    };
    select_result_atoms(&mut designer, atom_edit, &selected);
    refresh(&mut designer, &prefs);
    assert_eq!(stats(&designer).receivers_computed, 0);

    // Look straight down at the topmost atom and drag sideways.
    let down = DVec3::new(0.0, 0.0, -1.0);
    let ray_at = |dx: f64| DVec3::new(grab.x + dx, grab.y, grab.z + 50.0);
    default_tool_pointer_down(
        &mut designer,
        DVec2::ZERO,
        &ray_at(0.0),
        &down,
        SelectModifier::Replace,
    );
    // Past the drag threshold: this move only starts the drag.
    default_tool_pointer_move(
        &mut designer,
        DVec2::new(10.0, 0.0),
        &ray_at(0.0),
        &down,
        800.0,
        600.0,
        &down,
    );
    for tick in 1..=3 {
        let dx = 0.4 * tick as f64;
        default_tool_pointer_move(
            &mut designer,
            DVec2::new(10.0 + 10.0 * tick as f64, 0.0),
            &ray_at(dx),
            &down,
            800.0,
            600.0,
            &down,
        );
        assert!(designer.open_interaction().is_some(), "the drag is open");
        let sub_phases = refresh(&mut designer, &prefs);
        assert!(!evaluated_nothing(&sub_phases), "tick {tick} re-evaluates");
        assert_eq!(stats(&designer).receivers_computed, 0, "tick {tick}");
        assert_eq!(stats(&designer).deferred_updates, 1, "tick {tick}");
        assert!(pending(&designer, atom_edit) > 0, "tick {tick}");
    }

    default_tool_pointer_up(
        &mut designer,
        DVec2::new(40.0, 0.0),
        &ray_at(1.2),
        &down,
        SelectModifier::Replace,
        800.0,
        600.0,
        &DMat4::IDENTITY,
    );
    assert!(designer.open_interaction().is_none());
    refresh(&mut designer, &prefs);
    assert_eq!(stats(&designer).deferred_updates, 0);
    assert_eq!(stats(&designer).full_path_updates, 1);
    assert_eq!(pending(&designer, atom_edit), 0);
    assert_cache_is_exact(&designer, atom_edit, &prefs);
}

/// A gadget drag (`structure_move`'s gizmo, which moves every atom): its
/// ticks are lightweight and compute nothing; `gadget_end_drag` closes the
/// interaction before the refresh that follows it, which completes the
/// update.
#[test]
fn a_gadget_drag_completes_its_update_at_the_drag_end() {
    let prefs = display_prefs(true, None);
    let mut designer = designer(true);
    let mover = add_lattice_chain(&mut designer, 3, "structure_move");
    set_node_data(
        &mut designer,
        mover,
        Box::new(StructureMoveData {
            translation: IVec3::ZERO,
            lattice_subdivision: IVec3::ONE,
        }),
    );
    designer.mark_full_refresh();
    refresh(&mut designer, &prefs);
    assert!(stats(&designer).receivers_computed > 0);
    let before = structure(&designer, mover)
        .atoms_values()
        .next()
        .unwrap()
        .position;

    // The +x handle, grabbed at the origin and dragged one cell along x.
    let ray = |x: f64| (DVec3::new(x, 100.0, 0.0), DVec3::new(0.0, -1.0, 0.0));
    let (origin, direction) = ray(0.0);
    designer.gadget_start_drag(0, origin, direction);
    assert!(designer.open_interaction().is_some(), "the drag is open");
    refresh(&mut designer, &prefs);
    let (origin, direction) = ray(3.6);
    designer.gadget_drag(0, origin, direction);
    let sub_phases = refresh(&mut designer, &prefs);
    assert_eq!(sub_phases.exposure_ms, None, "a drag tick is lightweight");
    assert_eq!(stats(&designer), SceneExposureStats::default());

    designer.gadget_end_drag();
    assert!(designer.open_interaction().is_none());
    refresh(&mut designer, &prefs);
    let after = structure(&designer, mover)
        .atoms_values()
        .next()
        .unwrap()
        .position;
    assert!(after.distance(before) > 1.0, "the drag moved the structure");
    assert_eq!(stats(&designer).deferred_updates, 0);
    assert_eq!(stats(&designer).full_path_updates, 1);
    assert_cache_is_exact(&designer, mover, &prefs);
}

fn test_camera() -> Camera {
    Camera {
        eye: DVec3::new(0.0, -30.0, 10.0),
        target: DVec3::ZERO,
        up: DVec3::new(0.0, 0.32, 0.95).normalize(),
        aspect: 1.0,
        fovy: std::f64::consts::PI * 0.15,
        znear: 1.5,
        zfar: 2400.0,
        orthographic: false,
        ortho_half_height: 10.0,
        pivot_point: DVec3::ZERO,
        nav_up: DVec3::Z,
        nav_up_label: "Z".to_string(),
    }
}

/// The tessellator only reads the cache: with AO on but no `update_exposure`
/// run, every vertex is fully exposed. Once the pass has run, the opaque
/// atoms carry their values.
#[test]
fn the_tessellator_only_reads_the_cache() {
    let prefs = display_prefs(true, None);
    let mut designer = designer(true);
    let atom_edit = add_lattice_chain(&mut designer, 2, "atom_edit");
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);

    let tessellate = |designer: &StructureDesigner| {
        let (_, _, _, _, _, atoms, bonds, ..) = tessellate_scene_content(
            &designer.last_generated_structure_designer_scene,
            &test_camera(),
            false,
            &prefs,
        );
        (atoms, bonds)
    };
    let (atoms, bonds) = tessellate(&designer);
    assert!(!atoms.vertices.is_empty());
    assert!(atoms.vertices.iter().all(|v| v.exposure == FULLY_EXPOSED));
    assert!(
        bonds
            .vertices
            .iter()
            .all(|v| { v.exposure_start == FULLY_EXPOSED && v.exposure_end == FULLY_EXPOSED })
    );

    designer.update_exposure(&prefs, false);
    assert!(cached(&designer, atom_edit).is_some());
    let (atoms, _) = tessellate(&designer);
    assert!(atoms.vertices.iter().any(|v| v.exposure != FULLY_EXPOSED));

    // AO off in the preferences: the cached values are not used.
    let (_, _, _, _, _, atoms, ..) = tessellate_scene_content(
        &designer.last_generated_structure_designer_scene,
        &test_camera(),
        false,
        &display_prefs(false, None),
    );
    assert!(atoms.vertices.iter().all(|v| v.exposure == FULLY_EXPOSED));
}

/// Phase 4b's measurement: the refresh after an unrelated-node edit on a
/// 1 M-atom cuboid, with AO on and off, and the cost of the first fill.
/// Timing is never asserted; run in release and read the output:
///
/// ```text
/// cd rust
/// cargo test --release -p atomcad-structure-designer --test structure_designer \
///     ambient_occlusion_cache_timing -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a timing measurement; run in release with --ignored --nocapture"]
fn ambient_occlusion_cache_timing() {
    use std::time::Instant;
    for ambient_occlusion in [true, false] {
        let prefs = display_prefs(ambient_occlusion, Some(8.0));
        let mut designer = designer(ambient_occlusion);
        let mover = add_lattice_chain(&mut designer, 50, "structure_move");
        let unrelated = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
        designer.set_node_display(unrelated, true);
        designer.select_node(unrelated);
        designer.mark_full_refresh();
        let start = Instant::now();
        let first = refresh(&mut designer, &prefs);
        println!(
            "AO {}: {} atoms; first refresh {:.0} ms (eval {:.0}, exposure {:?} ms, {} receivers)",
            if ambient_occlusion { "on" } else { "off" },
            structure(&designer, mover).get_num_of_atoms(),
            start.elapsed().as_secs_f64() * 1000.0,
            first.eval_ms.unwrap_or(0.0),
            first.exposure_ms.map(|ms| ms.round()),
            stats(&designer).receivers_computed,
        );
        for _ in 0..3 {
            designer.mark_node_data_changed(unrelated);
            let start = Instant::now();
            let sub_phases = refresh(&mut designer, &prefs);
            println!(
                "  unrelated edit: refresh {:.1} ms (eval {:.1}, exposure {:?} ms, {} computed)",
                start.elapsed().as_secs_f64() * 1000.0,
                sub_phases.eval_ms.unwrap_or(0.0),
                sub_phases.exposure_ms.map(|ms| (ms * 10.0).round() / 10.0),
                stats(&designer).receivers_computed,
            );
        }
        // Selecting the structure re-evaluates it: the carry-over's diff.
        designer.select_node(mover);
        let start = Instant::now();
        let sub_phases = refresh(&mut designer, &prefs);
        println!(
            "  select the structure: refresh {:.0} ms (eval {:.0}, exposure {:?} ms, {} diffs, {} computed)",
            start.elapsed().as_secs_f64() * 1000.0,
            sub_phases.eval_ms.unwrap_or(0.0),
            sub_phases.exposure_ms.map(|ms| ms.round()),
            stats(&designer).diffs_run,
            stats(&designer).receivers_computed,
        );
    }
}
