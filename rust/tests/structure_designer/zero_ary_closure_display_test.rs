//! Phase 2 of `doc/design_zero_ary_closure_body_display.md` (issue #409) —
//! **backend**: eligibility, scoped scene generation, refresh integration and
//! the scope-extended display-toggle undo command.
//!
//! The feature: nodes living inside a **0-ary closure** body become
//! scene-evaluable — they get their own scope-aware scene entries and render in
//! the viewport like top-level nodes. Everything hinges on one rule:
//!
//! > A body node is scene-evaluable iff every zone-owning ancestor in its scope
//! > chain is a `closure` node with zero zone-input pins.
//!
//! Display flags stored in an *ineligible* body are **dormant**, not cleared —
//! adding a parameter to a closure stops its body rendering, removing the
//! parameter brings the previous display state back.
//!
//! Like `closures_test.rs` / `zones_test.rs`, the fixtures build bodies through
//! the scope-aware `StructureDesigner` mutation APIs (`add_node_scoped`,
//! `connect_nodes_scoped`, `connect_wire_scoped`, …) — the same entry points the
//! API layer drives — and refresh through `StructureDesigner::refresh` with the
//! designer's own pending changes.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::displayed_node_refs::{
    collect_displayed_node_refs, is_eligible_chain, is_zero_ary_closure,
};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{NodeRef, SourcePin};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::string::StringData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::structure_designer_scene::NodeOutput;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Overwrite a node's data **without** going through the validating
/// `set_node_network_data_scoped`, repopulating the derived `custom_node_type`
/// cache. Used where a test needs to observe a desynced state that the normal
/// mutation path would immediately validate away.
fn force_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    scope_path: &[u64],
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let mut network = registry.node_networks.get_mut(network_name).unwrap();
    for hof_id in scope_path {
        network = network.nodes.get_mut(hof_id).unwrap().zone_mut().unwrap();
    }
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

/// `ClosureData` for an n-ary `Custom` closure returning `ret`. `n == 0` is the
/// thunk shape this whole feature is about.
fn custom_closure_data(param_types: Vec<DataType>, ret: DataType) -> ClosureData {
    let param_names: Vec<String> = (0..param_types.len()).map(|i| format!("p{i}")).collect();
    let mut type_args = param_types;
    type_args.push(ret);
    ClosureData {
        kind: ClosureKind::Custom,
        type_args,
        param_names,
        custom_label: None,
    }
}

/// Add a `closure` node in `scope_path` and give it the `Custom` shape
/// `(param_types) -> ret` through the ordinary (validating) data setter.
fn add_custom_closure(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    position: DVec2,
    param_types: Vec<DataType>,
    ret: DataType,
) -> u64 {
    let id = designer.add_node_scoped(scope_path, "closure", position, None);
    designer.set_node_network_data_scoped(
        scope_path,
        id,
        Box::new(custom_closure_data(param_types, ret)),
    );
    id
}

/// Set an existing closure's arity through the ordinary data setter (the path
/// `set_closure_data` takes), leaving its body untouched.
fn set_closure_arity(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    closure_id: u64,
    param_types: Vec<DataType>,
    ret: DataType,
) {
    designer.set_node_network_data_scoped(
        scope_path,
        closure_id,
        Box::new(custom_closure_data(param_types, ret)),
    );
}

/// Node ids of the probe fixture built by [`setup_probe_closure`].
///
/// ```text
///   top level:  radius:int ─────────────────────┐ (capture, depth 1)
///               closure (0-ary, -> Crystal)     │
///                 body:  sphere ◄───────────────┘
///                          └─> materialize ─> tag ──> (zone output)
///                        string ─> print ────────┘  (tag.name, pin 1)
/// ```
///
/// * `tag` is the probed body node: its output is `NodeOutput::Atomic`, i.e.
///   real viewport geometry whose **atom count** tracks the captured radius —
///   so a stale scene entry is directly observable.
/// * `print` (`execute_only == false`) sits upstream of `tag`, so every
///   evaluation that reaches `tag` fires exactly one `take_print_log()` entry:
///   the re-evaluation counter (same technique as `refresh_pipeline_test.rs`).
struct ProbeClosure {
    closure: u64,
    radius: u64,
    sphere: u64,
    materialize: u64,
    string: u64,
    print: u64,
    tag: u64,
}

impl ProbeClosure {
    /// Scope path of the closure's body.
    fn body(&self) -> [u64; 1] {
        [self.closure]
    }

    fn tag_ref(&self) -> NodeRef {
        NodeRef::scoped(&self.body(), self.tag)
    }
}

/// Builds the probe network with **nothing** displayed and no refresh run yet.
/// Every node is displayed on creation, so the helper hides all of them; each
/// test opts the node it probes back in.
fn setup_probe_closure(network: &str) -> (StructureDesigner, ProbeClosure) {
    let mut designer = setup_designer_with_network(network);

    let radius = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.set_node_network_data_scoped(&[], radius, Box::new(IntData { value: 3 }));

    let closure = add_custom_closure(
        &mut designer,
        &[],
        DVec2::new(200.0, 0.0),
        vec![],
        DataType::Crystal,
    );
    let body = [closure];

    let sphere = designer.add_node_scoped(&body, "sphere", DVec2::new(0.0, 0.0), None);
    // radius (top-level `int`) → sphere.radius, as a capture wire (depth 1).
    designer.connect_wire_scoped(
        &body,
        radius,
        SourcePin::NodeOutput { pin_index: 0 },
        1,
        sphere,
        1,
    );

    let materialize = designer.add_node_scoped(&body, "materialize", DVec2::new(200.0, 0.0), None);
    designer.connect_nodes_scoped(&body, sphere, 0, materialize, 0);

    let string = designer.add_node_scoped(&body, "string", DVec2::new(0.0, 200.0), None);
    designer.set_node_network_data_scoped(
        &body,
        string,
        Box::new(StringData {
            value: "probe".to_string(),
        }),
    );

    let print = designer.add_node_scoped(&body, "print", DVec2::new(200.0, 200.0), None);
    designer.connect_nodes_scoped(&body, string, 0, print, 0);

    let tag = designer.add_node_scoped(&body, "tag", DVec2::new(400.0, 0.0), None);
    designer.connect_nodes_scoped(&body, materialize, 0, tag, 0);
    designer.connect_nodes_scoped(&body, print, 0, tag, 1);

    // Body result → the closure's single zone-output pin.
    designer.connect_zone_output_wire(&body, tag, 0, 0);

    designer.validate_active_network();
    assert!(
        designer
            .get_active_node_network()
            .expect("active network")
            .valid,
        "probe fixture must be a valid network, else nothing evaluates"
    );

    // Hide everything (top level + body).
    designer.set_node_display(radius, false);
    designer.set_node_display(closure, false);
    for id in [sphere, materialize, string, print, tag] {
        designer.set_node_display_scoped(&body, id, false);
    }

    let ids = ProbeClosure {
        closure,
        radius,
        sphere,
        materialize,
        string,
        print,
        tag,
    };
    (designer, ids)
}

fn set_radius(designer: &mut StructureDesigner, radius_id: u64, value: i32) {
    designer.set_node_network_data_scoped(&[], radius_id, Box::new(IntData { value }));
}

fn full_refresh_and_reset_counter(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
    designer.take_print_log();
}

/// Runs a refresh with whatever the designer has accumulated, asserting the
/// mode really is `Partial` — the cache/eviction assertions are meaningless
/// under a Full refresh (which rebuilds the scene from scratch).
fn partial_refresh(designer: &mut StructureDesigner) {
    let changes = designer.get_pending_changes();
    assert!(
        changes.is_partial(),
        "this assertion characterizes the PARTIAL refresh path; \
         something upstream escalated to {:?}",
        changes.mode
    );
    designer.refresh(&changes);
}

fn refresh(designer: &mut StructureDesigner) {
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

fn evaluations_since_last_check(designer: &mut StructureDesigner) -> usize {
    designer.take_print_log().len()
}

fn scene_has(designer: &StructureDesigner, node_ref: &NodeRef) -> bool {
    designer
        .last_generated_structure_designer_scene
        .node_data
        .contains_key(node_ref)
}

/// Atom count of a scoped scene entry's pin-0 output. Panics unless the entry
/// exists and carries atoms — both are part of what the callers assert.
fn scene_atom_count(designer: &StructureDesigner, node_ref: &NodeRef) -> usize {
    let entry = designer
        .last_generated_structure_designer_scene
        .node_data
        .get(node_ref)
        .expect("node should have a scene entry");
    match &entry.output {
        NodeOutput::Atomic(structure, _) => structure.get_num_of_atoms(),
        _ => panic!("expected an Atomic scene output"),
    }
}

fn cached_count(designer: &StructureDesigner) -> usize {
    designer
        .last_generated_structure_designer_scene
        .cached_node_count()
}

// ============================================================================
// Eligibility
// ============================================================================

#[test]
fn eligibility_zero_ary_custom_closure_at_top_level() {
    let mut designer = setup_designer_with_network("main");
    let closure = add_custom_closure(&mut designer, &[], DVec2::ZERO, vec![], DataType::Float);

    let network = designer.get_active_node_network().unwrap();
    let node = network.nodes.get(&closure).unwrap();
    assert!(is_zero_ary_closure(node, &designer.node_type_registry));
    assert!(is_eligible_chain(
        network,
        &designer.node_type_registry,
        &[closure]
    ));
}

#[test]
fn eligibility_zero_ary_nested_in_zero_ary_is_eligible() {
    let mut designer = setup_designer_with_network("main");
    let outer = add_custom_closure(&mut designer, &[], DVec2::ZERO, vec![], DataType::Float);
    let inner = add_custom_closure(
        &mut designer,
        &[outer],
        DVec2::ZERO,
        vec![],
        DataType::Float,
    );

    let network = designer.get_active_node_network().unwrap();
    assert!(is_eligible_chain(
        network,
        &designer.node_type_registry,
        &[outer, inner]
    ));
}

#[test]
fn eligibility_zero_ary_inside_map_body_is_not_eligible() {
    let mut designer = setup_designer_with_network("main");
    let map = designer.add_node("map", DVec2::ZERO);
    designer.set_node_network_data_scoped(
        &[],
        map,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    let closure = add_custom_closure(&mut designer, &[map], DVec2::ZERO, vec![], DataType::Float);

    let network = designer.get_active_node_network().unwrap();
    // The inner closure itself is 0-ary...
    let inner_node = network
        .nodes
        .get(&map)
        .unwrap()
        .zone
        .as_deref()
        .unwrap()
        .nodes
        .get(&closure)
        .unwrap();
    assert!(is_zero_ary_closure(
        inner_node,
        &designer.node_type_registry
    ));
    // ...but the `map` hop breaks the chain: `element` is unknowable at scene
    // time, so nothing under it is scene-evaluable.
    assert!(!is_eligible_chain(
        network,
        &designer.node_type_registry,
        &[map]
    ));
    assert!(!is_eligible_chain(
        network,
        &designer.node_type_registry,
        &[map, closure]
    ));
}

#[test]
fn eligibility_preset_kind_and_parameterized_custom_closures_are_not_eligible() {
    let mut designer = setup_designer_with_network("main");

    // Preset `Map` kind: `(T) -> U`, one zone-input pin.
    let preset = designer.add_node("closure", DVec2::ZERO);
    designer.set_node_network_data_scoped(
        &[],
        preset,
        Box::new(ClosureData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );

    // `Custom` kind with one parameter.
    let one_param = add_custom_closure(
        &mut designer,
        &[],
        DVec2::new(200.0, 0.0),
        vec![DataType::Int],
        DataType::Int,
    );

    let network = designer.get_active_node_network().unwrap();
    let registry = &designer.node_type_registry;
    assert!(!is_zero_ary_closure(
        network.nodes.get(&preset).unwrap(),
        registry
    ));
    assert!(!is_zero_ary_closure(
        network.nodes.get(&one_param).unwrap(),
        registry
    ));
    assert!(!is_eligible_chain(network, registry, &[preset]));
    assert!(!is_eligible_chain(network, registry, &[one_param]));
}

#[test]
fn collect_displayed_node_refs_skips_ineligible_bodies() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    designer.set_node_display(ids.radius, true);

    let network = designer.get_active_node_network().unwrap();
    let collected: Vec<NodeRef> =
        collect_displayed_node_refs(network, &designer.node_type_registry)
            .into_iter()
            .map(|(node_ref, _)| node_ref)
            .collect();
    assert!(collected.contains(&NodeRef::top(ids.radius)));
    assert!(collected.contains(&ids.tag_ref()));

    // Give the closure a parameter: the body's flags go dormant.
    set_closure_arity(
        &mut designer,
        &[],
        ids.closure,
        vec![DataType::Int],
        DataType::Crystal,
    );
    let network = designer.get_active_node_network().unwrap();
    let collected: Vec<NodeRef> =
        collect_displayed_node_refs(network, &designer.node_type_registry)
            .into_iter()
            .map(|(node_ref, _)| node_ref)
            .collect();
    assert!(collected.contains(&NodeRef::top(ids.radius)));
    assert!(
        !collected.contains(&ids.tag_ref()),
        "a body under a parameterized closure must contribute nothing"
    );
}

// ============================================================================
// Scoped scene generation
// ============================================================================

#[test]
fn displayed_body_node_gets_a_scoped_scene_entry_with_the_captured_value() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);

    assert!(
        scene_has(&designer, &ids.tag_ref()),
        "displayed body node must have a scene entry keyed by its scoped NodeRef"
    );
    let atoms_r3 = scene_atom_count(&designer, &ids.tag_ref());
    assert!(atoms_r3 > 0, "the probe body must materialize atoms");

    // The capture really is live: a smaller radius yields fewer atoms.
    set_radius(&mut designer, ids.radius, 2);
    full_refresh_and_reset_counter(&mut designer);
    let atoms_r2 = scene_atom_count(&designer, &ids.tag_ref());
    assert!(
        atoms_r2 < atoms_r3,
        "captured radius must drive the body node's output (r2={atoms_r2}, r3={atoms_r3})"
    );
}

#[test]
fn capture_liveness_partial_refresh_reevaluates_the_displayed_body_node() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);
    let before = scene_atom_count(&designer, &ids.tag_ref());

    // Edit the captured top-level source and refresh *partially* — this asserts
    // the ancestor → body dependency edge, not just full-refresh recollection.
    set_radius(&mut designer, ids.radius, 2);
    partial_refresh(&mut designer);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "the displayed body node must be re-evaluated when its capture source changes"
    );
    let after = scene_atom_count(&designer, &ids.tag_ref());
    assert!(
        after < before,
        "partial refresh must reflect the new captured value (before={before}, after={after})"
    );
}

#[test]
fn colliding_ids_top_level_and_body_node_keep_separate_scene_entries() {
    let (mut designer, ids) = setup_probe_closure("main");

    // Body ids come from the body's own `next_node_id` counter, so a collision
    // with a top-level id is the normal case, not a contrivance. Find one.
    let network = designer.get_active_node_network().unwrap();
    let colliding = [ids.sphere, ids.materialize, ids.string, ids.print, ids.tag]
        .into_iter()
        .find(|body_id| network.nodes.contains_key(body_id))
        .expect("expected at least one body id to collide with a top-level id");
    assert!(
        colliding == ids.radius || colliding == ids.closure,
        "the colliding top-level node should be one of the two we created"
    );

    designer.set_node_display(colliding, true);
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);

    assert!(scene_has(&designer, &NodeRef::top(colliding)));
    assert!(scene_has(&designer, &ids.tag_ref()));
    // The body entry carries atoms; the top-level `int` / `closure` does not —
    // proof that the two entries did not clobber each other.
    assert!(scene_atom_count(&designer, &ids.tag_ref()) > 0);
    assert!(matches!(
        designer
            .last_generated_structure_designer_scene
            .node_data
            .get(&NodeRef::top(colliding))
            .unwrap()
            .output,
        NodeOutput::None
    ));
}

#[test]
fn body_node_errors_and_hover_values_key_under_the_scoped_ref() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);

    let strings = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&ids.body(), ids.print);
    assert!(
        strings.is_some(),
        "hover values of nodes in the evaluated body chain must be keyed by their scoped NodeRef"
    );
    // The colliding top-level address must not resolve to the body node's data.
    assert!(
        designer
            .last_generated_structure_designer_scene
            .get_node_output_strings(&[], u64::MAX)
            .is_none()
    );
}

// ============================================================================
// Dormancy (arity changes)
// ============================================================================

#[test]
fn adding_a_parameter_drops_the_body_scene_entry_and_removing_it_brings_it_back() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);
    assert!(scene_has(&designer, &ids.tag_ref()));

    // 0 → 1 parameter: the whole body subtree stops rendering.
    set_closure_arity(
        &mut designer,
        &[],
        ids.closure,
        vec![DataType::Int],
        DataType::Crystal,
    );
    refresh(&mut designer);
    assert!(
        !scene_has(&designer, &ids.tag_ref()),
        "a parameterized closure's body must not render"
    );
    assert_eq!(
        cached_count(&designer),
        0,
        "the evicted subtree must not linger in the invisible cache either"
    );

    // The stored flag is dormant, not cleared.
    let body_network = designer.get_scope_network(&ids.body()).unwrap();
    assert!(
        body_network.is_node_displayed(ids.tag),
        "the display flag must survive the ineligible period (dormant, not cleared)"
    );

    // 1 → 0: the entry reappears without re-toggling anything.
    set_closure_arity(&mut designer, &[], ids.closure, vec![], DataType::Crystal);
    refresh(&mut designer);
    assert!(
        scene_has(&designer, &ids.tag_ref()),
        "restoring arity 0 must reactivate the dormant flag"
    );
    assert!(scene_atom_count(&designer, &ids.tag_ref()) > 0);
}

#[test]
fn nested_dormancy_outer_closure_gaining_a_param_drops_the_inner_body_entry() {
    let mut designer = setup_designer_with_network("main");
    let outer = add_custom_closure(&mut designer, &[], DVec2::ZERO, vec![], DataType::Float);
    let inner = add_custom_closure(
        &mut designer,
        &[outer],
        DVec2::ZERO,
        vec![],
        DataType::Float,
    );
    let inner_body = [outer, inner];
    let leaf = designer.add_node_scoped(&inner_body, "int", DVec2::ZERO, None);
    designer.set_node_network_data_scoped(&inner_body, leaf, Box::new(IntData { value: 7 }));
    designer.validate_active_network();

    designer.set_node_display(outer, false);
    designer.set_node_display_scoped(&[outer], inner, false);
    designer.set_node_display_scoped(&inner_body, leaf, true);
    full_refresh_and_reset_counter(&mut designer);

    let leaf_ref = NodeRef::scoped(&inner_body, leaf);
    assert!(scene_has(&designer, &leaf_ref));

    // Give the OUTER closure a parameter — the inner body is two hops down and
    // must go dormant too.
    set_closure_arity(
        &mut designer,
        &[],
        outer,
        vec![DataType::Int],
        DataType::Float,
    );
    refresh(&mut designer);
    assert!(
        !scene_has(&designer, &leaf_ref),
        "an ineligible ancestor must make the whole nested subtree dormant"
    );
}

#[test]
fn closure_edit_evicts_the_cached_subtree_so_a_later_show_re_evaluates() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);
    let atoms_r3 = scene_atom_count(&designer, &ids.tag_ref());

    // Hide → the entry moves into the invisible cache.
    designer.set_node_display_scoped(&ids.body(), ids.tag, false);
    partial_refresh(&mut designer);
    assert!(!scene_has(&designer, &ids.tag_ref()));
    assert_eq!(cached_count(&designer), 1);

    // Edit the closure (arity 0 → 1) → the cached subtree entry is evicted.
    set_closure_arity(
        &mut designer,
        &[],
        ids.closure,
        vec![DataType::Int],
        DataType::Crystal,
    );
    refresh(&mut designer);
    assert_eq!(
        cached_count(&designer),
        0,
        "a closure data change must evict its body subtree from the invisible cache"
    );

    // Back to arity 0 and show again: no stale restore — a fresh evaluation
    // with the current upstream value.
    set_closure_arity(&mut designer, &[], ids.closure, vec![], DataType::Crystal);
    set_radius(&mut designer, ids.radius, 2);
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    designer.take_print_log();
    refresh(&mut designer);

    assert!(
        evaluations_since_last_check(&mut designer) >= 1,
        "the node must be re-evaluated, not restored from a stale cache entry"
    );
    let atoms_r2 = scene_atom_count(&designer, &ids.tag_ref());
    assert!(
        atoms_r2 < atoms_r3,
        "restored entry must reflect the new captured radius (r2={atoms_r2}, r3={atoms_r3})"
    );
}

#[test]
fn hidden_body_node_cache_is_invalidated_when_its_capture_source_changes() {
    // The "stale restore" failure mode, on the *scoped* side: this is exactly
    // what dropping the `is_top_level()` filter in front of
    // `invalidate_cached_nodes` buys.
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);
    let atoms_r3 = scene_atom_count(&designer, &ids.tag_ref());

    designer.set_node_display_scoped(&ids.body(), ids.tag, false);
    partial_refresh(&mut designer);
    assert_eq!(cached_count(&designer), 1);

    // Change the captured source while the body node is hidden.
    set_radius(&mut designer, ids.radius, 2);
    partial_refresh(&mut designer);
    designer.take_print_log();

    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    partial_refresh(&mut designer);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "showing again must re-evaluate, not restore the stale cache entry"
    );
    let atoms_r2 = scene_atom_count(&designer, &ids.tag_ref());
    assert!(
        atoms_r2 < atoms_r3,
        "restored output must reflect the new captured radius (r2={atoms_r2}, r3={atoms_r3})"
    );
}

// ============================================================================
// Defensive `ZoneInput` floor
// ============================================================================

#[test]
fn zone_input_wire_in_a_zero_ary_body_yields_an_error_not_a_panic() {
    let mut designer = setup_designer_with_network("main");

    // Start from a *legal* 1-ary closure whose body reads its parameter.
    let closure = add_custom_closure(
        &mut designer,
        &[],
        DVec2::ZERO,
        vec![DataType::Int],
        DataType::Int,
    );
    let body = [closure];
    let expr = designer.add_node_scoped(&body, "expr", DVec2::ZERO, None);
    {
        let mut data = ExprData {
            parameters: vec![ExprParameter {
                id: None,
                name: "x".to_string(),
                data_type: DataType::Int,
                data_type_str: None,
            }],
            expression: "x + 1".to_string(),
            expr: None,
            error: None,
            output_type: None,
        };
        let _ = data.parse_and_validate(0);
        designer.set_node_network_data_scoped(&body, expr, Box::new(data));
    }
    designer.connect_wire_scoped(
        &body,
        closure,
        SourcePin::ZoneInput { pin_index: 0 },
        1,
        expr,
        0,
    );
    designer.connect_zone_output_wire(&body, expr, 0, 0);
    designer.validate_active_network();

    designer.set_node_display(closure, false);
    designer.set_node_display_scoped(&body, expr, true);

    // Now force the closure to 0-ary WITHOUT revalidating — the desync state
    // the floor exists for (refresh paths never validate; a stale
    // `custom_node_type` cache can produce the same shape).
    force_node_data(
        &mut designer,
        "main",
        &[],
        closure,
        Box::new(custom_closure_data(vec![], DataType::Int)),
    );
    assert!(
        designer.get_active_node_network().unwrap().valid,
        "the fixture must stay 'valid' — that is what lets the refresh reach the body"
    );

    // Must not panic.
    full_refresh_and_reset_counter(&mut designer);

    let expr_ref = NodeRef::scoped(&body, expr);
    assert!(
        scene_has(&designer, &expr_ref),
        "the body node is eligible (the closure now claims 0 params), so it is evaluated"
    );
    // The wire resolves to `NetworkResult::Error("zone input referenced
    // outside an invocation")`, which the destination surfaces as its own
    // per-input error badge — a localized failure, exactly like any other
    // upstream error, instead of a crash.
    let error = designer
        .last_generated_structure_designer_scene
        .get_node_error(&body, expr);
    assert!(
        error.is_some(),
        "the desynced zone-input wire must surface as a node error"
    );
    assert!(
        matches!(
            designer
                .last_generated_structure_designer_scene
                .node_data
                .get(&expr_ref)
                .unwrap()
                .output,
            NodeOutput::None
        ),
        "an errored body node produces no viewport output"
    );
}

// ============================================================================
// Display toggle undo (node level)
// ============================================================================

#[test]
fn body_display_toggle_is_undoable_and_redoable() {
    let (mut designer, ids) = setup_probe_closure("main");
    full_refresh_and_reset_counter(&mut designer);
    designer.undo_stack.clear();

    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    refresh(&mut designer);
    assert!(scene_has(&designer, &ids.tag_ref()));

    assert!(designer.undo());
    refresh(&mut designer);
    assert!(
        !designer
            .get_scope_network(&ids.body())
            .unwrap()
            .is_node_displayed(ids.tag),
        "undo must clear the flag in the BODY network"
    );
    assert!(!scene_has(&designer, &ids.tag_ref()));

    assert!(designer.redo());
    refresh(&mut designer);
    assert!(
        designer
            .get_scope_network(&ids.body())
            .unwrap()
            .is_node_displayed(ids.tag)
    );
    assert!(scene_has(&designer, &ids.tag_ref()));
}

#[test]
fn body_display_toggle_to_the_current_state_pushes_no_command() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.undo_stack.clear();

    // `tag` is already hidden by the fixture.
    designer.set_node_display_scoped(&ids.body(), ids.tag, false);
    assert!(
        !designer.undo_stack.can_undo(),
        "a no-op display toggle must not push an undo command"
    );
}

#[test]
fn body_display_undo_does_not_disturb_a_colliding_top_level_node() {
    let (mut designer, ids) = setup_probe_closure("main");
    // Pick a top-level node whose id collides with a body id.
    let network = designer.get_active_node_network().unwrap();
    let colliding = [ids.sphere, ids.materialize, ids.string, ids.print, ids.tag]
        .into_iter()
        .find(|body_id| network.nodes.contains_key(body_id))
        .expect("expected an id collision");

    designer.set_node_display(colliding, true);
    designer.undo_stack.clear();

    // Toggle the *body* node that shares an id with `colliding`, then undo.
    designer.set_node_display_scoped(&ids.body(), colliding, true);
    assert!(designer.undo());

    assert!(
        designer
            .get_active_node_network()
            .unwrap()
            .is_node_displayed(colliding),
        "undoing a body-scoped display toggle must not touch the top-level node with the same id"
    );
    assert!(
        !designer
            .get_scope_network(&ids.body())
            .unwrap()
            .is_node_displayed(colliding)
    );
}

// ============================================================================
// Deletion
// ============================================================================

#[test]
fn deleting_the_closure_removes_its_scoped_scene_entries() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);
    assert!(scene_has(&designer, &ids.tag_ref()));

    designer.select_node(ids.closure);
    designer.delete_selected();
    refresh(&mut designer);

    assert!(
        !scene_has(&designer, &ids.tag_ref()),
        "the body ceased to exist; its scene entries must be gone"
    );
    assert!(
        designer
            .last_generated_structure_designer_scene
            .node_data
            .keys()
            .all(|node_ref| node_ref.is_top_level()),
        "no scoped entry may survive the owning closure's deletion"
    );
}

// ============================================================================
// Ineligible-scope toggles are permitted (dormant flags, no error)
// ============================================================================

#[test]
fn toggling_display_in_an_ineligible_scope_stores_a_dormant_flag() {
    let mut designer = setup_designer_with_network("main");
    let map = designer.add_node("map", DVec2::ZERO);
    designer.set_node_network_data_scoped(
        &[],
        map,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    let body = [map];
    let leaf = designer.add_node_scoped(&body, "int", DVec2::ZERO, None);
    designer.validate_active_network();

    designer.set_node_display_scoped(&body, leaf, true);
    assert!(
        designer
            .get_scope_network(&body)
            .unwrap()
            .is_node_displayed(leaf),
        "the flag is stored (dormant) — the API does not reject ineligible scopes"
    );

    full_refresh_and_reset_counter(&mut designer);
    assert!(
        !scene_has(&designer, &NodeRef::scoped(&body, leaf)),
        "a dormant flag must not produce a scene entry"
    );
}

/// A body node hidden behind an ineligible chain must never be restored from
/// the invisible cache when its flag is toggled back on.
#[test]
fn ineligible_chain_blocks_cache_restoration() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);

    designer.set_node_display_scoped(&ids.body(), ids.tag, false);
    partial_refresh(&mut designer);
    assert_eq!(cached_count(&designer), 1);

    // Make the chain ineligible *without* touching the closure's data, by
    // forcing the arity change straight onto the node (no data_changed mark,
    // so the Step 0 eviction cannot help — only the eligibility gate can).
    force_node_data(
        &mut designer,
        "main",
        &[],
        ids.closure,
        Box::new(custom_closure_data(vec![DataType::Int], DataType::Crystal)),
    );

    designer.set_node_display_scoped(&ids.body(), ids.tag, true);
    partial_refresh(&mut designer);

    assert!(
        !scene_has(&designer, &ids.tag_ref()),
        "an ineligible chain must not restore its cached entry"
    );
}

// ============================================================================
// The colliding-id wire test lives here because it needs a real body
// ============================================================================

#[test]
fn zone_input_floor_leaves_invocation_paths_untouched() {
    // Sanity: a *legal* 1-ary closure body still evaluates through the normal
    // invocation path after the `current_zone_input` → `try_current_zone_input`
    // conversion (the floor must not have broken the happy path).
    let mut designer = setup_designer_with_network("main");
    let closure = add_custom_closure(
        &mut designer,
        &[],
        DVec2::ZERO,
        vec![DataType::Int],
        DataType::Int,
    );
    let body = [closure];
    let expr = designer.add_node_scoped(&body, "expr", DVec2::ZERO, None);
    {
        let mut data = ExprData {
            parameters: vec![ExprParameter {
                id: None,
                name: "x".to_string(),
                data_type: DataType::Int,
                data_type_str: None,
            }],
            expression: "x * 2".to_string(),
            expr: None,
            error: None,
            output_type: None,
        };
        let _ = data.parse_and_validate(0);
        designer.set_node_network_data_scoped(&body, expr, Box::new(data));
    }
    designer.connect_wire_scoped(
        &body,
        closure,
        SourcePin::ZoneInput { pin_index: 0 },
        1,
        expr,
        0,
    );
    designer.connect_zone_output_wire(&body, expr, 0, 0);

    let arg = designer.add_node("int", DVec2::new(0.0, 200.0));
    designer.set_node_network_data_scoped(&[], arg, Box::new(IntData { value: 21 }));
    let apply = designer.add_node("apply", DVec2::new(400.0, 0.0));
    designer.connect_nodes(closure, 0, apply, 0);
    designer.validate_active_network();
    designer.connect_nodes(arg, 0, apply, 1);
    designer.validate_active_network();

    designer.set_node_display(apply, true);
    full_refresh_and_reset_counter(&mut designer);

    let strings = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], apply)
        .expect("apply should have hover strings");
    assert!(
        strings.iter().any(|s| s.contains("42")),
        "the invocation path must still resolve zone inputs; got {strings:?}"
    );
}

/// Guard for the fixture itself: a body node with a *wired* body chain fires
/// exactly one print entry per evaluation.
#[test]
fn probe_body_evaluates_once_per_displayed_node_pass() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display_scoped(&ids.body(), ids.tag, true);

    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "one displayed body node downstream of `print` should fire exactly one entry"
    );
}

/// The top-level path must be byte-identical to before: displayed top-level
/// nodes still render, and the collection returns them with `NodeRef::top`.
#[test]
fn top_level_display_is_unaffected() {
    let (mut designer, ids) = setup_probe_closure("main");
    designer.set_node_display(ids.radius, true);
    full_refresh_and_reset_counter(&mut designer);

    assert!(scene_has(&designer, &NodeRef::top(ids.radius)));
    assert_eq!(
        designer
            .last_generated_structure_designer_scene
            .node_data
            .len(),
        1
    );
}
