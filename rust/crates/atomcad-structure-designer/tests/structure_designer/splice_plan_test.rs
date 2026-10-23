//! The pure splice planner (`splice_plan.rs`,
//! `doc/design_insert_node_on_wire.md` Phase 0), driven without a designer:
//! a registry, a planned node, and the two sides.

use atomcad_structure_designer::data_type::{DataType, FunctionType};
use atomcad_structure_designer::declared_inputs::NameSource;
use atomcad_structure_designer::node_data::{DragDirection, NodeData};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::expr::ExprData;
use atomcad_structure_designer::nodes::map::MapData;
use atomcad_structure_designer::nodes::record_construct::RecordConstructData;
use atomcad_structure_designer::splice_plan::{
    Arrangement, ArrangementInput, ArrangementOutput, InputChoice, PlannedNode, SpliceSource,
    plan_arrangements,
};
use atomcad_structure_designer::structure_designer::DragSource;

fn source(data_type: DataType) -> SpliceSource<'static> {
    SpliceSource {
        data_type,
        name: NameSource {
            node_name: Some("src"),
            pin_name: None,
            prefer_pin: false,
        },
    }
}

/// A fresh node of `type_name`, adapted to `source` as an insertion adapts it.
fn new_from_output(registry: &NodeTypeRegistry, type_name: &str, from: &DataType) -> PlannedNode {
    let drag = DragSource {
        source_type: from.clone(),
        direction: DragDirection::FromOutput,
    };
    PlannedNode::new_of_type(registry, type_name, Some(&drag)).unwrap()
}

fn plain(registry: &NodeTypeRegistry, type_name: &str) -> PlannedNode {
    PlannedNode::new_of_type(registry, type_name, None).unwrap()
}

fn planned_with(type_name: &str, data: Box<dyn NodeData>) -> PlannedNode {
    PlannedNode {
        type_name: type_name.to_string(),
        data,
    }
}

/// Each arrangement as `(input name, output name)`, `-` for a missing side.
fn pairs(arrangements: &[Arrangement]) -> Vec<(String, String)> {
    arrangements
        .iter()
        .map(|a| {
            (
                a.input
                    .as_ref()
                    .map_or("-".to_string(), |input| input.name.clone()),
                a.output
                    .as_ref()
                    .map_or("-".to_string(), |output| output.name.clone()),
            )
        })
        .collect()
}

fn strs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(i, o)| (i.to_string(), o.to_string()))
        .collect()
}

fn iter_of(t: DataType) -> DataType {
    DataType::Iterator(Box::new(t))
}

// ---------------------------------------------------------------------------
// diff: fixed pins
// ---------------------------------------------------------------------------

#[test]
fn diff_offers_both_inputs_into_its_one_output() {
    let r = NodeTypeRegistry::new();
    let diff = plain(&r, "diff");
    let blueprint = source(DataType::Blueprint);

    let both = plan_arrangements(
        &r,
        &diff,
        Some(&blueprint),
        Some(&DataType::Blueprint),
        false,
    );
    assert_eq!(pairs(&both), strs(&[("base", "result"), ("sub", "result")]));
    assert_eq!(
        both[0],
        Arrangement {
            input: Some(ArrangementInput {
                choice: InputChoice::Pin(0),
                name: "base".to_string(),
                data_type: DataType::Array(Box::new(DataType::Blueprint)),
            }),
            output: Some(ArrangementOutput {
                pin: 0,
                name: "result".to_string(),
                data_type: DataType::Blueprint,
            }),
        }
    );

    let source_only = plan_arrangements(&r, &diff, Some(&blueprint), None, false);
    assert_eq!(pairs(&source_only), strs(&[("base", "-"), ("sub", "-")]));

    let target_only = plan_arrangements(&r, &diff, None, Some(&DataType::Blueprint), false);
    assert_eq!(pairs(&target_only), strs(&[("-", "result")]));

    // Nothing fits on either side.
    assert!(plan_arrangements(&r, &diff, Some(&source(DataType::Float)), None, false).is_empty());
    assert!(plan_arrangements(&r, &diff, None, Some(&DataType::Float), false).is_empty());
}

// ---------------------------------------------------------------------------
// structure_move: an output that mirrors an input (SameAsInput)
// ---------------------------------------------------------------------------

#[test]
fn structure_move_validity_is_decided_per_pair() {
    let r = NodeTypeRegistry::new();
    let crystal = DataType::Crystal;
    let mv = new_from_output(&r, "structure_move", &crystal);

    // A Crystal through `input` comes out of `result` as a Crystal; `diff` is
    // a Molecule and does not fit a Crystal destination.
    let into_crystal = plan_arrangements(
        &r,
        &mv,
        Some(&source(crystal.clone())),
        Some(&crystal),
        false,
    );
    assert_eq!(pairs(&into_crystal), strs(&[("input", "result")]));
    assert_eq!(
        into_crystal[0].output.as_ref().unwrap().data_type,
        DataType::Crystal
    );

    // Into a Molecule destination only `diff` fits: `result` mirrors the
    // Crystal that entered.
    let into_molecule = plan_arrangements(
        &r,
        &mv,
        Some(&source(crystal.clone())),
        Some(&DataType::Molecule),
        false,
    );
    assert_eq!(pairs(&into_molecule), strs(&[("input", "diff")]));

    // A Blueprint through the same pin makes `result` a Blueprint.
    let blueprint = plan_arrangements(
        &r,
        &mv,
        Some(&source(DataType::Blueprint)),
        Some(&DataType::Blueprint),
        false,
    );
    assert_eq!(pairs(&blueprint), strs(&[("input", "result")]));
}

// ---------------------------------------------------------------------------
// expr: the open port, with and without pass-through
// ---------------------------------------------------------------------------

/// A fresh `expr`'s default data: no parameters, and the constant body `0`.
fn bare_expr(r: &NodeTypeRegistry) -> PlannedNode {
    let planned = plain(r, "expr");
    let data = planned
        .data
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .unwrap();
    assert!(data.parameters.is_empty(), "a fresh expr declares nothing");
    assert_eq!(data.expression, "0");
    planned
}

#[test]
fn expr_open_port_passes_the_value_through_only_with_the_pass_through() {
    let r = NodeTypeRegistry::new();
    let expr = bare_expr(&r);
    let vec3 = source(DataType::Vec3);

    // Without a target the open port is offered under the pool's first name.
    let open = plan_arrangements(&r, &expr, Some(&vec3), None, false);
    assert_eq!(pairs(&open), strs(&[("x", "-")]));
    assert_eq!(
        open[0].input.as_ref().unwrap().choice,
        InputChoice::OpenPort
    );
    assert_eq!(open[0].input.as_ref().unwrap().data_type, DataType::Vec3);

    // The constant body `0` is an Int whatever enters, so it cannot feed a
    // Vec3 destination...
    assert!(plan_arrangements(&r, &expr, Some(&vec3), Some(&DataType::Vec3), false).is_empty());
    // ...until the new input is made the identity.
    let through = plan_arrangements(&r, &expr, Some(&vec3), Some(&DataType::Vec3), true);
    assert_eq!(pairs(&through), strs(&[("x", "result")]));
    assert_eq!(
        through[0].output.as_ref().unwrap().data_type,
        DataType::Vec3
    );

    // Into a Float destination the constant type-checks without the identity
    // (Int widens to Float), which is why the identity matters for the value,
    // not only for the type.
    let float = source(DataType::Float);
    let constant = plan_arrangements(&r, &expr, Some(&float), Some(&DataType::Float), false);
    assert_eq!(
        constant[0].output.as_ref().unwrap().data_type,
        DataType::Int
    );
    let identity = plan_arrangements(&r, &expr, Some(&float), Some(&DataType::Float), true);
    assert_eq!(
        identity[0].output.as_ref().unwrap().data_type,
        DataType::Float
    );
}

#[test]
fn an_adapted_expr_offers_its_pin_and_not_the_open_port_beside_it() {
    let r = NodeTypeRegistry::new();
    let expr = new_from_output(&r, "expr", &DataType::Float);
    let arrangements = plan_arrangements(
        &r,
        &expr,
        Some(&source(DataType::Float)),
        Some(&DataType::Float),
        true,
    );
    assert_eq!(pairs(&arrangements), strs(&[("x", "result")]));
    assert_eq!(
        arrangements[0].input.as_ref().unwrap().choice,
        InputChoice::Pin(0)
    );
}

// ---------------------------------------------------------------------------
// map: adapted to the source, and its layout derived from a function source
// ---------------------------------------------------------------------------

#[test]
fn map_is_adapted_to_an_iterator_source() {
    let r = NodeTypeRegistry::new();
    let ints = iter_of(DataType::Int);
    let map = new_from_output(&r, "map", &ints);
    let arrangements = plan_arrangements(&r, &map, Some(&source(ints.clone())), Some(&ints), false);
    assert_eq!(pairs(&arrangements), strs(&[("xs", "result")]));
}

#[test]
fn map_takes_its_output_type_from_a_function_source() {
    let r = NodeTypeRegistry::new();
    // Float -> Int: the default map's element type is Float, so the function
    // enters `f`, and the derived output is Iter[Int], not the stored
    // Iter[Float].
    let function = DataType::Function(FunctionType::new(vec![DataType::Float], DataType::Int));
    let map = new_from_output(&r, "map", &function);
    let into_ints = plan_arrangements(
        &r,
        &map,
        Some(&source(function.clone())),
        Some(&iter_of(DataType::Int)),
        false,
    );
    assert_eq!(pairs(&into_ints), strs(&[("f", "result")]));
    assert_eq!(
        into_ints[0].output.as_ref().unwrap().data_type,
        iter_of(DataType::Int)
    );
    assert!(
        plan_arrangements(
            &r,
            &map,
            Some(&source(function)),
            Some(&iter_of(DataType::Blueprint)),
            false
        )
        .is_empty()
    );
}

// ---------------------------------------------------------------------------
// The function pin, re-adaptation, and the hook's default
// ---------------------------------------------------------------------------

#[test]
fn no_arrangement_has_the_function_pin_as_its_output() {
    let r = NodeTypeRegistry::new();
    let mut names: Vec<&String> = r.built_in_node_types.keys().collect();
    names.sort();
    let targets = [
        DataType::Float,
        DataType::Blueprint,
        DataType::Function(FunctionType::new(vec![DataType::Float], DataType::Float)),
    ];
    let mut seen = 0;
    for name in names {
        for target in &targets {
            let planned = plain(&r, name);
            for arrangement in plan_arrangements(
                &r,
                &planned,
                Some(&source(DataType::Float)),
                Some(target),
                true,
            ) {
                seen += 1;
                assert!(
                    arrangement.output.as_ref().unwrap().pin >= 0,
                    "{} offers its function pin",
                    name
                );
            }
        }
    }
    assert!(seen > 10, "the sweep found arrangements to check");
}

#[test]
fn an_existing_node_is_not_re_adapted() {
    let r = NodeTypeRegistry::new();
    let blueprints = iter_of(DataType::Blueprint);
    // A fresh map adapts its element type to the stream...
    let fresh = new_from_output(&r, "map", &blueprints);
    assert!(
        !plan_arrangements(&r, &fresh, Some(&source(blueprints.clone())), None, false).is_empty()
    );
    // ...an existing one keeps the type the user gave it.
    let existing_data: Box<dyn NodeData> = Box::new(MapData {
        input_type: DataType::Float,
        output_type: DataType::Float,
    });
    let existing = planned_with("map", existing_data);
    assert!(plan_arrangements(&r, &existing, Some(&source(blueprints)), None, false).is_empty());
}

#[test]
fn make_pass_through_defaults_to_doing_nothing() {
    let mut data = RecordConstructData::default();
    let pin_id = {
        let declared = data.declared_inputs_mut().unwrap();
        declared.append("a".to_string(), DataType::Int).unwrap()
    };
    let before = serde_json::to_string(&data).unwrap();
    assert!(
        !data
            .declared_inputs_mut()
            .unwrap()
            .make_pass_through(pin_id)
    );
    assert_eq!(before, serde_json::to_string(&data).unwrap());
}
