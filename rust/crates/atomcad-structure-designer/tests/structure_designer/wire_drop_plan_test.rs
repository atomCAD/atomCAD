//! The plan-first wire drop (`doc/design_insert_node_on_wire.md` Phase 0):
//! `plan_wire_drop`, `add_node_and_connect`, and the add primitive both build
//! on, `add_node_core`. The read-only refusal is in the library-linking
//! mutation alphabet (`library_links_readonly_test.rs`).

use crate::splice_support::{assert_no_mutation, assert_one_step_round_trip};
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_data::DragDirection;
use atomcad_structure_designer::node_network::{ArgumentKind, NodeNetwork, SourcePin};
use atomcad_structure_designer::nodes::map::MapData;
use atomcad_structure_designer::nodes::record_construct::RecordConstructData;
use atomcad_structure_designer::splice_ops::{DraggedPin, SpliceRefusal};
use atomcad_structure_designer::splice_plan::{Arrangement, ArrangementInput, InputChoice};
use atomcad_structure_designer::structure_designer::{DragSource, StructureDesigner};
use glam::f64::DVec2;

const NET: &str = "Main";
const OUT: SourcePin = SourcePin::NodeOutput { pin_index: 0 };
const AT: DVec2 = DVec2::new(400.0, 300.0);

/// `x` (an Int), `b` (a Blueprint), `rl` (a `relax` fed by `mt`, a Crystal),
/// and `mp`, a `map` whose body holds `g` (an Int) returned as the body's
/// result.
const CODE: &str = "x = int { value: 5 }
b = cuboid {}
mt = materialize { shape: b }
rl = relax { molecule: mt }
r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, body { g = int { value: 1 } output g } }
";

fn setup() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    let outcome = d.ai_text_edit(CODE, false);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    d.validate_active_network();
    d
}

fn id_in(network: &NodeNetwork, name: &str) -> u64 {
    network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node '{}'", name))
        .id
}

fn id(d: &StructureDesigner, name: &str) -> u64 {
    id_in(d.get_scope_network(&[]).unwrap(), name)
}

/// The body scope of `mp` and the id of its body node `g`.
fn body(d: &StructureDesigner) -> (Vec<u64>, u64) {
    let scope = vec![id(d, "mp")];
    let g = id_in(d.get_scope_network(&scope).unwrap(), "g");
    (scope, g)
}

fn output_of(node_id: u64) -> DraggedPin {
    DraggedPin::Output { node_id, pin: OUT }
}

fn input_of(node_id: u64, index: usize) -> DraggedPin {
    DraggedPin::Input {
        node_id,
        kind: ArgumentKind::External,
        index,
    }
}

fn input_names(plan: &[Arrangement]) -> Vec<String> {
    plan.iter()
        .map(|a| a.input.as_ref().unwrap().name.clone())
        .collect()
}

fn output_names(plan: &[Arrangement]) -> Vec<String> {
    plan.iter()
        .map(|a| a.output.as_ref().unwrap().name.clone())
        .collect()
}

/// `(source node, source pin, depth)` of every wire into each input of
/// `node_id` in `scope`.
fn wires_into(
    d: &StructureDesigner,
    scope: &[u64],
    node_id: u64,
) -> Vec<Vec<(u64, SourcePin, u8)>> {
    d.get_scope_network(scope).unwrap().nodes[&node_id]
        .arguments
        .iter()
        .map(|arg| {
            arg.incoming_wires
                .iter()
                .map(|w| (w.source_node_id, w.source_pin, w.source_scope_depth))
                .collect()
        })
        .collect()
}

/// Plan `node_type` for `pin` and commit its only arrangement.
fn drop_and_connect(
    d: &mut StructureDesigner,
    pin_scope: &[u64],
    pin: DraggedPin,
    drop_scope: &[u64],
    node_type: &str,
) -> u64 {
    let plan = d
        .plan_wire_drop(pin_scope, &pin, drop_scope, node_type)
        .unwrap();
    assert_eq!(plan.len(), 1, "{:?}", plan);
    d.add_node_and_connect(pin_scope, &pin, drop_scope, node_type, AT, &plan[0])
        .unwrap()
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[test]
fn a_drop_scope_that_cannot_see_the_pin_is_refused_and_changes_nothing() {
    let mut d = setup();
    let (body_scope, g) = body(&d);
    let rl = id(&d, "rl");

    // A body node's output is invisible outside the body.
    let refused = assert_no_mutation(&mut d, |d| {
        d.plan_wire_drop(&body_scope, &output_of(g), &[], "expr")
    });
    assert_eq!(refused, Err(SpliceRefusal::ScopeNotVisible));

    // A top-level input cannot be fed from inside a body.
    let refused = assert_no_mutation(&mut d, |d| {
        d.plan_wire_drop(&[], &input_of(rl, 0), &body_scope, "atom_edit")
    });
    assert_eq!(refused, Err(SpliceRefusal::ScopeNotVisible));

    // The commit refuses the same way.
    let arrangement = Arrangement {
        input: None,
        output: None,
    };
    let refused = assert_no_mutation(&mut d, |d| {
        d.add_node_and_connect(&body_scope, &output_of(g), &[], "expr", AT, &arrangement)
    });
    assert_eq!(refused, Err(SpliceRefusal::ScopeNotVisible));
}

#[test]
fn a_stale_arrangement_is_refused_and_changes_nothing() {
    let mut d = setup();
    let x = id(&d, "x");
    let stale = Arrangement {
        input: Some(ArrangementInput {
            choice: InputChoice::Pin(0),
            name: "no_such_input".to_string(),
            data_type: DataType::Int,
        }),
        output: None,
    };
    let refused = assert_no_mutation(&mut d, |d| {
        d.add_node_and_connect(&[], &output_of(x), &[], "expr", AT, &stale)
    });
    assert_eq!(refused, Err(SpliceRefusal::StaleArrangement));
}

#[test]
fn unknown_types_and_missing_pins_are_refused() {
    let d = setup();
    let x = id(&d, "x");
    assert_eq!(
        d.plan_wire_drop(&[], &output_of(x), &[], "no_such_type"),
        Err(SpliceRefusal::UnknownNodeType("no_such_type".to_string()))
    );
    assert_eq!(
        d.plan_wire_drop(&[], &output_of(9999), &[], "expr"),
        Err(SpliceRefusal::PinNotFound)
    );
    assert_eq!(
        d.plan_wire_drop(&[], &output_of(x), &[9999], "expr"),
        Err(SpliceRefusal::ScopeNotFound)
    );
}

// ---------------------------------------------------------------------------
// The plan's answers
// ---------------------------------------------------------------------------

#[test]
fn off_a_blueprint_output_diff_offers_base_and_sub() {
    let mut d = setup();
    let b = id(&d, "b");
    let plan = assert_no_mutation(&mut d, |d| {
        d.plan_wire_drop(&[], &output_of(b), &[], "diff").unwrap()
    });
    assert_eq!(input_names(&plan), ["base", "sub"]);
    assert!(plan.iter().all(|a| a.output.is_none()));
}

#[test]
fn off_an_input_every_fitting_output_is_offered_not_only_pin_0() {
    let mut d = setup();
    let rl = id(&d, "rl");
    // Off relax's HasAtoms input: atom_edit's `result` (a Molecule when
    // nothing is wired into it) and its `diff` both fit.
    let plan = assert_no_mutation(&mut d, |d| {
        d.plan_wire_drop(&[], &input_of(rl, 0), &[], "atom_edit")
            .unwrap()
    });
    assert_eq!(output_names(&plan), ["result", "diff"]);
    assert!(plan.iter().all(|a| a.input.is_none()));
}

#[test]
fn a_type_not_allowed_in_a_body_plans_to_nothing_there() {
    let d = setup();
    let (body_scope, _) = body(&d);
    let x = id(&d, "x");
    let plan = d
        .plan_wire_drop(&[], &input_of(id(&d, "mt"), 0), &[], "parameter")
        .unwrap();
    assert!(!plan.is_empty(), "a parameter can feed a top-level input");
    assert!(
        d.plan_wire_drop(&[], &output_of(x), &body_scope, "parameter")
            .unwrap()
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// add_node_and_connect
// ---------------------------------------------------------------------------

#[test]
fn add_node_and_connect_is_one_undo_step_off_an_output() {
    let mut d = setup();
    let x = id(&d, "x");
    let node = assert_one_step_round_trip(&mut d, |d| {
        drop_and_connect(d, &[], output_of(x), &[], "expr")
    });
    assert_eq!(wires_into(&d, &[], node), vec![vec![(x, OUT, 0)]]);
    assert_eq!(d.undo_stack.undo_description(), Some("Add node"));
    let network = d.get_scope_network(&[]).unwrap();
    assert_eq!(network.nodes[&node].position, AT);
    assert!(
        network
            .validation_errors
            .iter()
            .all(|e| e.node_id != Some(node))
    );
}

#[test]
fn add_node_and_connect_off_an_input_wires_the_chosen_output() {
    let mut d = setup();
    let rl = id(&d, "rl");
    let plan = d
        .plan_wire_drop(&[], &input_of(rl, 0), &[], "atom_edit")
        .unwrap();
    let diff = plan
        .iter()
        .find(|a| a.output.as_ref().unwrap().name == "diff")
        .unwrap()
        .clone();
    let node = assert_one_step_round_trip(&mut d, |d| {
        d.add_node_and_connect(&[], &input_of(rl, 0), &[], "atom_edit", AT, &diff)
            .unwrap()
    });
    assert_eq!(
        wires_into(&d, &[], rl)[0],
        vec![(node, SourcePin::NodeOutput { pin_index: 1 }, 0)]
    );
}

#[test]
fn a_drop_inside_a_body_makes_a_depth_0_wire_stored_on_the_body_node() {
    let mut d = setup();
    let (body_scope, g) = body(&d);
    let node = assert_one_step_round_trip(&mut d, |d| {
        drop_and_connect(d, &body_scope, output_of(g), &body_scope, "expr")
    });
    assert_eq!(wires_into(&d, &body_scope, node), vec![vec![(g, OUT, 0)]]);
}

#[test]
fn a_drop_of_an_outer_output_into_a_body_makes_a_depth_1_capture() {
    let mut d = setup();
    let (body_scope, _) = body(&d);
    let x = id(&d, "x");
    let node = assert_one_step_round_trip(&mut d, |d| {
        drop_and_connect(d, &[], output_of(x), &body_scope, "expr")
    });
    assert_eq!(wires_into(&d, &body_scope, node), vec![vec![(x, OUT, 1)]]);
}

#[test]
fn a_drop_off_a_zone_input_into_its_body_reads_the_element_at_depth_1() {
    let mut d = setup();
    let (body_scope, _) = body(&d);
    let mp = id(&d, "mp");
    let element = DraggedPin::Output {
        node_id: mp,
        pin: SourcePin::ZoneInput { pin_index: 0 },
    };
    let node = assert_one_step_round_trip(&mut d, |d| {
        drop_and_connect(d, &[], element, &body_scope, "expr")
    });
    assert_eq!(
        wires_into(&d, &body_scope, node),
        vec![vec![(mp, SourcePin::ZoneInput { pin_index: 0 }, 1)]]
    );
}

#[test]
fn a_drop_off_a_body_return_pin_feeds_the_hof() {
    let mut d = setup();
    let (body_scope, _) = body(&d);
    let mp = id(&d, "mp");
    let result = DraggedPin::Input {
        node_id: mp,
        kind: ArgumentKind::ZoneOutput,
        index: 0,
    };
    let node = assert_one_step_round_trip(&mut d, |d| {
        drop_and_connect(d, &[], result, &body_scope, "int")
    });
    let wires: Vec<_> = d.get_scope_network(&[]).unwrap().nodes[&mp].zone_output_arguments[0]
        .incoming_wires
        .iter()
        .map(|w| (w.source_node_id, w.source_pin, w.source_scope_depth))
        .collect();
    // The single-wire slot is replaced, as a plain connect replaces it.
    assert_eq!(wires, vec![(node, OUT, 0)]);
}

#[test]
fn an_open_port_target_declares_its_input() {
    let mut d = setup();
    let x = id(&d, "x");
    // A fresh anonymous record_construct has no field to take an Int, so the
    // wire enters through the open port, which declares the field.
    let plan = d
        .plan_wire_drop(&[], &output_of(x), &[], "record_construct")
        .unwrap();
    assert_eq!(plan.len(), 1);
    assert_eq!(
        plan[0].input.as_ref().unwrap().choice,
        InputChoice::OpenPort
    );
    let node = assert_one_step_round_trip(&mut d, |d| {
        d.add_node_and_connect(&[], &output_of(x), &[], "record_construct", AT, &plan[0])
            .unwrap()
    });
    let data = d
        .get_node_network_data_scoped(&[], node)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .unwrap();
    assert_eq!(data.fields.len(), 1);
    assert_eq!(data.fields[0].name, plan[0].input.as_ref().unwrap().name);
    assert_eq!(data.fields[0].data_type, DataType::Int);
    assert_eq!(wires_into(&d, &[], node), vec![vec![(x, OUT, 0)]]);
}

#[test]
fn a_parameter_made_by_a_wire_drop_is_a_full_interface_pin() {
    use atomcad_structure_designer::nodes::parameter::ParameterData;
    let mut d = setup();
    let mt = id(&d, "mt");
    let node = assert_one_step_round_trip(&mut d, |d| {
        drop_and_connect(d, &[], input_of(mt, 0), &[], "parameter")
    });
    let data = d
        .get_node_network_data_scoped(&[], node)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap();
    assert!(data.param_id.is_some(), "the parameter has a param_id");
    assert_eq!(data.data_type, DataType::Blueprint);
    let network = d.get_scope_network(&[]).unwrap();
    assert!(network.valid);
    assert_eq!(wires_into(&d, &[], mt)[0], vec![(node, OUT, 0)]);
}

// ---------------------------------------------------------------------------
// add_node_core
// ---------------------------------------------------------------------------

#[test]
fn add_node_core_pushes_no_undo_step() {
    let mut d = setup();
    let pushes = d.undo_stack.push_count();
    let node = d.add_node_core(&[], "int", AT, None);
    assert_ne!(node, 0);
    assert!(d.get_scope_network(&[]).unwrap().nodes.contains_key(&node));
    assert_eq!(pushes, d.undo_stack.push_count());
}

#[test]
fn a_node_added_in_a_body_is_adapted_to_the_drag_as_at_top_level() {
    let mut d = setup();
    let (body_scope, _) = body(&d);
    let drag = DragSource {
        source_type: DataType::Array(Box::new(DataType::Int)),
        direction: DragDirection::FromOutput,
    };
    let top = d.add_node_scoped(&[], "map", AT, Some(drag.clone()));
    let inner = d.add_node_scoped(&body_scope, "map", AT, Some(drag));
    let input_type = |scope: &[u64], node: u64| {
        d.get_node_network_data_scoped(scope, node)
            .unwrap()
            .as_any_ref()
            .downcast_ref::<MapData>()
            .unwrap()
            .input_type
            .clone()
    };
    assert_eq!(input_type(&[], top), DataType::Int);
    assert_eq!(input_type(&body_scope, inner), DataType::Int);
}
