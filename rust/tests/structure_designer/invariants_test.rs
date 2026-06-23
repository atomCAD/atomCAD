//! Phase 0 invariant-checker tests (`doc/design_identity_vs_naming_phase0.md`).
//!
//! - §6.1: one forcing test per `InvariantKind` (white-box — `Node` /
//!   `ParameterData` fields are `pub`), plus `should_panic` proof that the debug
//!   wrapper has teeth for the fatal kinds.
//! - §6.2: a seeded property/fuzz suite — structure-preserving mutations crossed
//!   with the persistence axis, asserting the wire-identity oracle and that
//!   `check_document_invariants` stays fatal-free.
//! - §6.3: the lint entry point + a committed healthy fixture.

use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::invariants::{
    InvariantKind, InvariantViolation, check_document_invariants, check_network_invariants,
};
use rust_lib_flutter_cad::structure_designer::node_network::{NodeNetwork, SourcePin};
use rust_lib_flutter_cad::structure_designer::nodes::closure::ClosureData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// A designer with one empty active network named `net`.
fn designer_with_net() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network("net");
    d.set_active_node_network_name(Some("net".to_string()));
    d
}

/// Borrow the (already-validated) `net` network out of the registry.
fn net<'a>(d: &'a StructureDesigner) -> &'a NodeNetwork {
    d.node_type_registry.node_networks.get("net").unwrap()
}

fn net_mut<'a>(d: &'a mut StructureDesigner) -> &'a mut NodeNetwork {
    d.node_type_registry.node_networks.get_mut("net").unwrap()
}

/// Run the per-network checker over `net`.
fn check(d: &StructureDesigner) -> Vec<InvariantViolation> {
    check_network_invariants(net(d), &d.node_type_registry)
}

fn has_kind(violations: &[InvariantViolation], kind: &InvariantKind) -> bool {
    violations.iter().any(|v| &v.kind == kind)
}

fn find_kind<'a>(
    violations: &'a [InvariantViolation],
    kind: &InvariantKind,
) -> Option<&'a InvariantViolation> {
    violations.iter().find(|v| &v.kind == kind)
}

// ---------------------------------------------------------------------------
// §6.1 — one forcing test per InvariantKind
// ---------------------------------------------------------------------------

#[test]
fn arg_count_mismatch_is_reported() {
    let mut d = designer_with_net();
    let id = d.add_node("sphere", DVec2::ZERO);
    d.validate_active_network();

    // Push an extra Argument so arguments.len() != params.len().
    net_mut(&mut d)
        .nodes
        .get_mut(&id)
        .unwrap()
        .arguments
        .push(rust_lib_flutter_cad::structure_designer::node_network::Argument::new());

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::ArgCountMismatch));
    assert!(
        find_kind(&v, &InvariantKind::ArgCountMismatch)
            .unwrap()
            .is_fatal()
    );
}

#[test]
fn zone_arg_count_mismatch_is_reported() {
    let mut d = designer_with_net();
    let id = d.add_node("map", DVec2::ZERO);
    d.validate_active_network();

    // Force the zone-output argument list out of sync with the type's pins.
    net_mut(&mut d)
        .nodes
        .get_mut(&id)
        .unwrap()
        .zone_output_arguments
        .clear();

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::ZoneArgCountMismatch));
    assert!(
        find_kind(&v, &InvariantKind::ZoneArgCountMismatch)
            .unwrap()
            .is_fatal()
    );
}

#[test]
fn cache_none_is_reported_with_legacy_substring() {
    let mut d = designer_with_net();
    let id = d.add_node("parameter", DVec2::ZERO);
    d.validate_active_network();

    // Force the derived `parameter` node into the stale state (B).
    net_mut(&mut d).nodes.get_mut(&id).unwrap().custom_node_type = None;

    let v = check(&d);
    let violation = find_kind(&v, &InvariantKind::CacheNone)
        .expect("CacheNone should be reported for a derived node with a None cache");
    assert!(violation.is_fatal());
    // §7: the existing `#[should_panic]` regression test keys on this substring.
    assert!(
        violation
            .detail
            .contains("custom_node_type cache invariant violated"),
        "CacheNone detail must contain the legacy substring, got: {}",
        violation.detail
    );
}

#[test]
fn duplicate_param_id_is_reported() {
    let mut d = designer_with_net();
    let a = d.add_node("parameter", DVec2::new(0.0, 0.0));
    let b = d.add_node("parameter", DVec2::new(0.0, 100.0));
    d.validate_active_network();

    // Copy a's param_id onto b.
    let a_pid = net(&d)
        .nodes
        .get(&a)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap()
        .param_id;
    {
        let n = net_mut(&mut d);
        let bd = n
            .nodes
            .get_mut(&b)
            .unwrap()
            .data
            .as_any_mut()
            .downcast_mut::<ParameterData>()
            .unwrap();
        bd.param_id = a_pid;
    }

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::DuplicateParamId));
    assert!(
        find_kind(&v, &InvariantKind::DuplicateParamId)
            .unwrap()
            .is_fatal()
    );
}

#[test]
fn param_id_floor_is_reported() {
    let mut d = designer_with_net();
    d.add_node("parameter", DVec2::ZERO);
    d.validate_active_network();

    // Drop next_param_id below the max assigned param_id.
    net_mut(&mut d).next_param_id = 0;

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::ParamIdFloor));
}

#[test]
fn next_node_id_floor_is_reported() {
    let mut d = designer_with_net();
    d.add_node("int", DVec2::ZERO);
    d.validate_active_network();

    net_mut(&mut d).next_node_id = 0;

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::NextNodeIdFloor));
}

#[test]
fn unresolved_node_type_is_reported() {
    let mut d = designer_with_net();
    let id = d.add_node("int", DVec2::ZERO);
    d.validate_active_network();

    {
        let node = net_mut(&mut d).nodes.get_mut(&id).unwrap();
        node.node_type_name = "no_such_type".to_string();
        node.custom_node_type = None; // else the cache would shadow the bogus name
    }

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::UnresolvedNodeType));
}

#[test]
fn unresolved_record_name_via_parameter_is_reported_and_fatal() {
    let mut d = designer_with_net();
    let id = d.add_node("parameter", DVec2::ZERO);
    d.validate_active_network();

    // Retype the parameter to reference a record def that doesn't exist.
    {
        let pd = net_mut(&mut d)
            .nodes
            .get_mut(&id)
            .unwrap()
            .data
            .as_any_mut()
            .downcast_mut::<ParameterData>()
            .unwrap();
        pd.data_type = DataType::Record(RecordType::Named("ghost".to_string()));
    }

    let v = check(&d);
    let violation = find_kind(&v, &InvariantKind::UnresolvedRecordName)
        .expect("an embedded Record(Named(\"ghost\")) with no def must be reported");
    // No validation error sits on this node (the rename-walk omission bug is
    // *silent*), so it is fatal.
    assert!(violation.is_fatal());
}

#[test]
fn unresolved_record_name_via_closure_type_args_is_reported() {
    // The load-bearing case (design §6.1 / §9): a `closure` left with a stale
    // record name in `type_args`. R2 catches it via the *outcome* (does it
    // resolve?), regardless of which node type embeds it.
    let mut d = designer_with_net();
    let id = d.add_node("closure", DVec2::ZERO);
    d.validate_active_network();

    {
        let cd = net_mut(&mut d)
            .nodes
            .get_mut(&id)
            .unwrap()
            .data
            .as_any_mut()
            .downcast_mut::<ClosureData>()
            .unwrap();
        cd.type_args[0] = DataType::Record(RecordType::Named("ghost".to_string()));
    }

    let v = check(&d);
    assert!(
        has_kind(&v, &InvariantKind::UnresolvedRecordName),
        "closure.type_args ghost ref must be reported, got: {:#?}",
        v
    );
}

#[test]
fn unresolved_schema_is_reported() {
    let mut d = designer_with_net();
    let id = d.add_node("record_construct", DVec2::ZERO);
    d.validate_active_network();

    {
        let rc = net_mut(&mut d)
            .nodes
            .get_mut(&id)
            .unwrap()
            .data
            .as_any_mut()
            .downcast_mut::<RecordConstructData>()
            .unwrap();
        rc.schema = "ghost".to_string();
    }

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::UnresolvedSchema));
}

#[test]
fn empty_schema_is_not_reported() {
    // An unset (empty) schema is a not-yet-configured state, not a dangling
    // reference — it must NOT be flagged.
    let mut d = designer_with_net();
    d.add_node("record_construct", DVec2::ZERO);
    d.validate_active_network();

    let v = check(&d);
    assert!(!has_kind(&v, &InvariantKind::UnresolvedSchema));
}

#[test]
fn missing_wire_source_is_reported() {
    let mut d = designer_with_net();
    let src = d.add_node("int", DVec2::new(0.0, 0.0));
    let dst = d.add_node("sphere", DVec2::new(100.0, 0.0));
    // sphere arg 1 is `radius` (Float); int's output broadcasts/converts fine.
    d.connect_nodes(src, 0, dst, 1);
    d.validate_active_network();

    // Delete the source node out from under the wire.
    net_mut(&mut d).nodes.remove(&src);

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::MissingWireSource));
    assert!(
        find_kind(&v, &InvariantKind::MissingWireSource)
            .unwrap()
            .is_fatal()
    );
}

#[test]
fn pin_index_out_of_range_is_reported() {
    let mut d = designer_with_net();
    let src = d.add_node("int", DVec2::new(0.0, 0.0));
    let dst = d.add_node("sphere", DVec2::new(100.0, 0.0));
    d.connect_nodes(src, 0, dst, 1);
    d.validate_active_network();

    // Point the wire at a non-existent output pin of the source.
    {
        let wire = &mut net_mut(&mut d).nodes.get_mut(&dst).unwrap().arguments[1].incoming_wires[0];
        wire.source_pin = SourcePin::NodeOutput { pin_index: 7 };
    }

    let v = check(&d);
    assert!(has_kind(&v, &InvariantKind::PinIndexOutOfRange));
}

#[test]
fn incompatible_wire_type_tier_semantics() {
    // T1 is not emitted live in Phase 0, but its tier semantics are defined:
    // Tier 3 → fatal only when NOT accounted-for.
    let unaccounted = InvariantViolation {
        scope_path: vec![],
        node_id: Some(1),
        kind: InvariantKind::IncompatibleWireType,
        detail: "Bool -> Int".to_string(),
        accounted_for: false,
    };
    let accounted = InvariantViolation {
        accounted_for: true,
        ..unaccounted.clone()
    };
    assert!(!InvariantKind::IncompatibleWireType.is_tier1());
    assert!(unaccounted.is_fatal(), "unaccounted Tier-3 is fatal");
    assert!(!accounted.is_fatal(), "accounted Tier-3 is not fatal");
}

// ---------------------------------------------------------------------------
// §6.1 — the debug wrapper has teeth for the fatal kinds (and floors don't
// false-fire on the hot path).
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "network invariant(s) violated")]
fn debug_wrapper_panics_on_arg_count_mismatch() {
    use rust_lib_flutter_cad::structure_designer::invariants::debug_assert_network_invariants;
    let mut d = designer_with_net();
    let id = d.add_node("sphere", DVec2::ZERO);
    d.validate_active_network();
    net_mut(&mut d)
        .nodes
        .get_mut(&id)
        .unwrap()
        .arguments
        .push(rust_lib_flutter_cad::structure_designer::node_network::Argument::new());
    debug_assert_network_invariants(net(&d), &d.node_type_registry);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "network invariant(s) violated")]
fn debug_wrapper_panics_on_silent_unresolved_record_name() {
    use rust_lib_flutter_cad::structure_designer::invariants::debug_assert_network_invariants;
    let mut d = designer_with_net();
    let id = d.add_node("parameter", DVec2::ZERO);
    d.validate_active_network();
    {
        let pd = net_mut(&mut d)
            .nodes
            .get_mut(&id)
            .unwrap()
            .data
            .as_any_mut()
            .downcast_mut::<ParameterData>()
            .unwrap();
        pd.data_type = DataType::Record(RecordType::Named("ghost".to_string()));
    }
    debug_assert_network_invariants(net(&d), &d.node_type_registry);
}

#[cfg(debug_assertions)]
#[test]
fn debug_wrapper_does_not_panic_on_id_counter_floor() {
    // Floors are excluded from the hot path (they false-fire on honest
    // counter-lag transients) but remain fatal in the document checker.
    use rust_lib_flutter_cad::structure_designer::invariants::debug_assert_network_invariants;
    let mut d = designer_with_net();
    d.add_node("int", DVec2::ZERO);
    d.validate_active_network();
    net_mut(&mut d).next_node_id = 0;

    // Hot path: must NOT panic.
    debug_assert_network_invariants(net(&d), &d.node_type_registry);

    // But the checker still *reports* it, and the document checker treats it as
    // fatal.
    assert!(has_kind(&check(&d), &InvariantKind::NextNodeIdFloor));
    let doc = check_document_invariants(&d.node_type_registry);
    assert!(
        doc.iter()
            .any(|v| v.kind == InvariantKind::NextNodeIdFloor && v.is_fatal())
    );
}

// ---------------------------------------------------------------------------
// Document-level: record-def → record-def reference resolution.
// ---------------------------------------------------------------------------

#[test]
fn document_checker_flags_dangling_record_def_field_ref() {
    use rust_lib_flutter_cad::structure_designer::node_type_registry::RecordTypeDef;
    let mut d = StructureDesigner::new();
    // A def whose field references a non-existent def.
    d.node_type_registry.record_type_defs.insert(
        "R".to_string(),
        RecordTypeDef {
            name: "R".to_string(),
            fields: vec![(
                "f".to_string(),
                DataType::Record(RecordType::Named("ghost".to_string())),
            )],
        },
    );

    let v = check_document_invariants(&d.node_type_registry);
    assert!(
        v.iter()
            .any(|x| x.kind == InvariantKind::UnresolvedRecordName
                && x.node_id.is_none()
                && x.is_fatal()),
        "dangling record-def field ref must be a fatal document violation"
    );
}

// ---------------------------------------------------------------------------
// §6.2 — property / fuzz suite (the F5 generalization, focused subset).
// ---------------------------------------------------------------------------
//
// A seeded deterministic generator drives a sequence of *structure-preserving*
// mutations (param rename / reorder / retype — the ops whose whole point is
// that the wire set must not change) and, after each, forks the state through
// the persistence axis (fresh + save→load — the fork that exposed the
// `next_param_id` reset). At every fork it asserts:
//   (a) the **wire-identity oracle**: each wire keyed by
//       `(source_node_id, source_pin, dest_node_id, dest_param_id)` — where
//       `dest_param_id` is resolved from the positional index via the node type
//       at assert time — is unchanged from the pre-mutation set; and
//   (b) `check_document_invariants` reports no fatal violation.
//
// Scope note: the mutation alphabet here is the structure-preserving core
// (param rename/reorder/retype). The remaining alphabet entries from the design
// (record-def field reorder, factor/inline, duplicate-network, closure⇄network)
// and the export→import persistence fork are left for a follow-up — the param
// oracle under save→load is the highest-value slice (it guards the
// `param_id`-recycling bug family directly).

use std::collections::BTreeSet;

/// A wire keyed by source + identity-stable destination (param_id), not by
/// positional index. `dest_param_id` is `None` only for a param that has no id
/// (legacy) — kept distinct so a missing id can't silently alias.
type OracleKey = (u64, i32, u64, Option<u64>);

const SCRATCH: &str = "C:\\Users\\DMNAGY~1\\AppData\\Local\\Temp\\claude\\C--machine-phase-systems-flutter-cad\\61b3d3f7-5d8e-4034-a264-0c0024a7a7c2\\scratchpad";

/// Set all four text properties on a parameter node, then validate the active
/// network (which re-derives the interface and repairs call sites).
fn set_param_props(
    d: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    name: &str,
    sort_order: i32,
    data_type: DataType,
) {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    use std::collections::HashMap;

    d.set_active_node_network_name(Some(network_name.to_string()));
    {
        let network = d
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        let node = network.nodes.get_mut(&node_id).unwrap();
        let pd = node
            .data
            .as_any_mut()
            .downcast_mut::<ParameterData>()
            .unwrap();
        let mut props = HashMap::new();
        props.insert(
            "param_name".to_string(),
            TextValue::String(name.to_string()),
        );
        props.insert("data_type".to_string(), TextValue::DataType(data_type));
        props.insert("sort_order".to_string(), TextValue::Int(sort_order));
        props.insert(
            "param_index".to_string(),
            TextValue::Int(pd.param_index as i32),
        );
        pd.set_text_properties(&props).unwrap();
    }
    d.validate_active_network();
}

/// Compute the wire-identity oracle for the Sub instance in `main`.
fn oracle(d: &StructureDesigner, sub_instance_id: u64) -> BTreeSet<OracleKey> {
    let registry = &d.node_type_registry;
    let main = registry.node_networks.get("main").unwrap();
    let inst = main.nodes.get(&sub_instance_id).unwrap();
    // Resolve the instance's parameter ids from its node type at assert time.
    let node_type = registry.get_node_type_for_node(inst).unwrap();
    let mut set = BTreeSet::new();
    for (i, arg) in inst.arguments.iter().enumerate() {
        let dest_param_id = node_type.parameters.get(i).and_then(|p| p.id);
        for wire in &arg.incoming_wires {
            if let Some((src, pin)) = wire.as_legacy_pair() {
                set.insert((src, pin, sub_instance_id, dest_param_id));
            }
        }
    }
    set
}

/// Save the designer to a temp `.cnnd` and load it back into a fresh designer.
fn save_load_roundtrip(d: &mut StructureDesigner, tag: &str) -> StructureDesigner {
    let path = format!("{}\\prop_{}.cnnd", SCRATCH, tag);
    d.save_node_networks_as(&path).expect("save");
    let mut loaded = StructureDesigner::new();
    loaded.load_node_networks(&path).expect("load");
    loaded
}

fn assert_no_fatal_doc(d: &StructureDesigner, ctx: &str) {
    let v = check_document_invariants(&d.node_type_registry);
    let fatal: Vec<&InvariantViolation> = v.iter().filter(|x| x.is_fatal()).collect();
    assert!(
        fatal.is_empty(),
        "fatal document invariant(s) [{}]: {:#?}",
        ctx,
        fatal
    );
}

/// Build the base scenario: a `Sub` network with two Int parameters + a return
/// node, instanced in `main` with both pins wired from `int` sources.
/// Returns the Sub-instance node id in `main` and the two parameter node ids in
/// `Sub`.
fn build_base() -> (StructureDesigner, u64, u64, u64) {
    let mut d = StructureDesigner::new();

    // Sub: parameter p0, parameter p1, return int.
    d.add_node_network("Sub");
    d.set_active_node_network_name(Some("Sub".to_string()));
    let p0 = d.add_node("parameter", DVec2::new(0.0, 0.0));
    let p1 = d.add_node("parameter", DVec2::new(0.0, 100.0));
    let ret = d.add_node("int", DVec2::new(200.0, 0.0));
    d.set_return_node_id(Some(ret));
    set_param_props(&mut d, "Sub", p0, "p0", 0, DataType::Int);
    set_param_props(&mut d, "Sub", p1, "p1", 1, DataType::Int);

    // main: two int sources wired into a Sub instance.
    d.add_node_network("main");
    d.set_active_node_network_name(Some("main".to_string()));
    let s0 = d.add_node("int", DVec2::new(0.0, 0.0));
    let s1 = d.add_node("int", DVec2::new(0.0, 100.0));
    let sub = d.add_node("Sub", DVec2::new(200.0, 0.0));
    d.connect_nodes(s0, 0, sub, 0);
    d.connect_nodes(s1, 0, sub, 1);
    d.validate_active_network();

    (d, sub, p0, p1)
}

fn property_run(seed: u64) {
    let (mut d, sub, p0, p1) = build_base();

    // Capture the invariant set the structure-preserving ops must preserve.
    let initial = oracle(&d, sub);
    assert_eq!(initial.len(), 2, "base scenario should have two Sub wires");
    assert_no_fatal_doc(&d, "base");

    // Track current param state so renames stay unique / types stay valid.
    let mut state = seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1);
    let mut next_name = 0u64;
    // sort orders + types per param (start matching the base).
    let mut so = [0i32, 1i32];
    let mut ty = [DataType::Int, DataType::Int];

    for step in 0..14u64 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let choice = (state >> 33) % 4;
        match choice {
            0 => {
                // rename p0
                next_name += 1;
                set_param_props(
                    &mut d,
                    "Sub",
                    p0,
                    &format!("a{}", next_name),
                    so[0],
                    ty[0].clone(),
                );
            }
            1 => {
                // rename p1
                next_name += 1;
                set_param_props(
                    &mut d,
                    "Sub",
                    p1,
                    &format!("b{}", next_name),
                    so[1],
                    ty[1].clone(),
                );
            }
            2 => {
                // swap sort order (reorder the interface)
                so.swap(0, 1);
                let n0 = cur_name(&d, p0);
                let n1 = cur_name(&d, p1);
                set_param_props(&mut d, "Sub", p0, &n0, so[0], ty[0].clone());
                set_param_props(&mut d, "Sub", p1, &n1, so[1], ty[1].clone());
            }
            _ => {
                // retype p1 Int<->Float (always convertible from the Int source)
                ty[1] = if ty[1] == DataType::Int {
                    DataType::Float
                } else {
                    DataType::Int
                };
                let n1 = cur_name(&d, p1);
                set_param_props(&mut d, "Sub", p1, &n1, so[1], ty[1].clone());
            }
        }

        // Fork A — fresh (in-memory).
        let fresh = oracle(&d, sub);
        assert_eq!(
            fresh, initial,
            "seed={} step={} choice={}: fresh oracle drifted",
            seed, step, choice
        );
        assert_no_fatal_doc(&d, &format!("seed={} step={} fresh", seed, step));

        // Fork B — save → load.
        let loaded = save_load_roundtrip(&mut d, &format!("{}_{}", seed, step));
        let loaded_oracle = oracle(&loaded, sub);
        assert_eq!(
            loaded_oracle, initial,
            "seed={} step={} choice={}: save→load oracle drifted",
            seed, step, choice
        );
        assert_no_fatal_doc(&loaded, &format!("seed={} step={} loaded", seed, step));
    }
}

/// Current param name of a parameter node in `Sub`.
fn cur_name(d: &StructureDesigner, node_id: u64) -> String {
    d.node_type_registry
        .node_networks
        .get("Sub")
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap()
        .param_name
        .clone()
}

#[test]
fn property_structure_preserving_mutations_preserve_wire_oracle() {
    for seed in [1u64, 7, 42, 1234, 99999] {
        property_run(seed);
    }
}

// ---------------------------------------------------------------------------
// §6.3 — lint entry point + committed healthy fixture.
// ---------------------------------------------------------------------------

const HEALTHY_FIXTURE: &str = "tests/fixtures/invariants/healthy.cnnd";

/// Regenerate the committed healthy fixture. Run once with
/// `cargo test --test structure_designer generate_healthy_fixture -- --ignored`,
/// then commit `tests/fixtures/invariants/healthy.cnnd`.
#[test]
#[ignore]
fn generate_healthy_fixture() {
    let (mut d, _sub, _p0, _p1) = build_base();
    std::fs::create_dir_all("tests/fixtures/invariants").unwrap();
    d.save_node_networks_as(HEALTHY_FIXTURE).unwrap();
}

#[test]
fn healthy_fixture_has_no_fatal_violations() {
    let mut d = StructureDesigner::new();
    d.load_node_networks(HEALTHY_FIXTURE)
        .unwrap_or_else(|e| panic!("healthy fixture failed to load: {}", e));
    let v = check_document_invariants(&d.node_type_registry);
    let fatal: Vec<&InvariantViolation> = v.iter().filter(|x| x.is_fatal()).collect();
    assert!(
        fatal.is_empty(),
        "healthy fixture has fatal violations: {:#?}",
        fatal
    );
}

/// Lint real `.cnnd` projects. Reads `;`-separated paths from
/// `ATOMCAD_LINT_FILES`, loads each, runs the document checker, and prints all
/// violations grouped by network. `#[ignore]`d — run it to calibrate the
/// catalogue against real projects:
/// `ATOMCAD_LINT_FILES="a.cnnd;b.cnnd" cargo test lint_projects -- --ignored --nocapture`.
#[test]
#[ignore]
fn lint_projects() {
    let files = std::env::var("ATOMCAD_LINT_FILES").unwrap_or_default();
    let paths: Vec<&str> = files.split(';').filter(|s| !s.is_empty()).collect();
    assert!(
        !paths.is_empty(),
        "set ATOMCAD_LINT_FILES to a ;-separated list of .cnnd files"
    );
    let mut total_fatal = 0usize;
    for path in paths {
        let mut d = StructureDesigner::new();
        match d.load_node_networks(path) {
            Ok(_) => {}
            Err(e) => {
                println!("[{}] FAILED TO LOAD: {}", path, e);
                continue;
            }
        }
        let violations = check_document_invariants(&d.node_type_registry);
        println!(
            "[{}] {} violation(s), {} fatal:",
            path,
            violations.len(),
            violations.iter().filter(|v| v.is_fatal()).count()
        );
        for v in &violations {
            total_fatal += usize::from(v.is_fatal());
            println!(
                "    {}{:?} {:?} node={:?}: {}",
                if v.is_fatal() { "FATAL " } else { "" },
                v.kind,
                v.scope_path,
                v.node_id,
                v.detail
            );
        }
    }
    println!("TOTAL fatal across all files: {}", total_fatal);
}
