//! Abstract-typed output pins (`HasAtoms` / `HasStructure` / `HasFreeLinOps`).
//!
//! A user-declared function type may carry an abstract *return* type — e.g. a
//! `Custom`-kind `closure` declared `(theta: Float) -> HasAtoms`. When such a
//! function is consumed by `apply` with every arg pin wired, the post-pass
//! installs `Fixed(HasAtoms)` on the apply's output pin. These tests pin the
//! permissive behavior of that statically-abstract output:
//!
//! 1. `resolve_output_type` resolves the pin to the abstract type (it used to
//!    return `None`, which made the output un-wirable through the interactive
//!    connect gate even into a destination pin declared with the *same*
//!    abstract type, while the text-format path + validator + evaluator all
//!    accepted the identical wire).
//! 2. The connect gate accepts abstract → same-abstract (identity conversion)
//!    and still rejects abstract → concrete downcasts and cross-abstract
//!    edges.
//! 3. A connected abstract → same-abstract wire validates clean.
//! 4. The validator actually type-checks a wire from an abstract source — an
//!    abstract → concrete wire smuggled in (as a hand-edited `.cnnd` or the
//!    text format could) is flagged as a data type mismatch instead of being
//!    silently skipped as "unresolvable".
//! 5. At runtime the value flowing out of the apply is a concrete phase
//!    variant (`Molecule` here) — the abstract annotation is static only.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{Argument, IncomingWire, SourcePin};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::apply::ApplyData;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::float::FloatData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

/// Set node data and refresh the node's custom-type cache (mirrors the helper
/// used by `currying_test.rs`).
fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
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

/// Add a built-in node (with its default data) into a zone-owning node's body.
/// Returns the new body node's id.
fn add_body_node(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    node_type_name: &str,
) -> u64 {
    let registry = &mut designer.node_type_registry;
    let (data, num_params) = {
        let nt = registry
            .built_in_node_types
            .get(node_type_name)
            .expect("built-in node type");
        ((nt.node_data_creator)(), nt.parameters.len())
    };
    let body = registry
        .node_networks
        .get_mut(owner_network)
        .unwrap()
        .nodes
        .get_mut(&owner_node_id)
        .unwrap()
        .zone_mut()
        .expect("zone-owning node missing zone");
    let id = body.add_node(node_type_name, DVec2::new(50.0, 0.0), num_params, data);

    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        registry
            .node_networks
            .get_mut(owner_network)
            .unwrap()
            .nodes
            .get_mut(&owner_node_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&id)
            .unwrap(),
        true,
    );

    id
}

/// Wire a body node into the owner's first zone-output pin.
fn wire_body_node_to_zone_output(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    body_node_id: u64,
) {
    let owner_node = designer
        .node_type_registry
        .node_networks
        .get_mut(owner_network)
        .unwrap()
        .nodes
        .get_mut(&owner_node_id)
        .unwrap();
    if owner_node.zone_output_arguments.is_empty() {
        owner_node.zone_output_arguments.push(Argument::new());
    }
    owner_node.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: body_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

/// Build the scenario from the field report (SPM tip file): a `Custom` closure
/// declared `(theta: Float) -> HasAtoms` whose body produces a concrete
/// `Molecule` (an input-less `atom_edit` — its output falls back to
/// `Molecule`), fully applied by an `apply` node. Returns the designer and the
/// apply node id; the apply's output pin is `Fixed(HasAtoms)` after the
/// post-pass.
fn setup_abstract_return_apply() -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let closure_id = designer.add_node("closure", DVec2::new(0.0, -200.0));
    set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Float, DataType::HasAtoms],
            param_names: vec!["theta".to_string()],
            custom_label: None,
        }),
    );
    let edit_id = add_body_node(&mut designer, "main", closure_id, "atom_edit");
    wire_body_node_to_zone_output(&mut designer, "main", closure_id, edit_id);

    let theta = designer.add_node("float", DVec2::new(0.0, -100.0));
    set_node_data(
        &mut designer,
        "main",
        theta,
        Box::new(FloatData { value: 45.0 }),
    );

    let app = designer.add_node("apply", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "main", app, Box::new(ApplyData::default()));
    designer.connect_nodes(closure_id, 0, app, 0); // f — triggers post-pass derivation
    designer.connect_nodes(theta, 0, app, 1); // arg0 — full application

    (designer, app)
}

#[test]
fn apply_with_abstract_function_return_resolves_output_to_abstract_type() {
    let (designer, app) = setup_abstract_return_apply();
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("main").unwrap();
    let node = network.nodes.get(&app).unwrap();
    assert_eq!(
        registry.resolve_output_type(node, network, 0),
        Some(DataType::HasAtoms),
        "fully-applied apply over (Float) -> HasAtoms must resolve its output \
         pin to HasAtoms instead of dead-ending at None"
    );
}

#[test]
fn connect_gate_accepts_abstract_to_same_abstract_and_rejects_downcasts() {
    let (mut designer, app) = setup_abstract_return_apply();
    let xyz = designer.add_node("export_atoms", DVec2::new(200.0, 0.0));
    let smove = designer.add_node("structure_move", DVec2::new(200.0, 100.0));
    let v3 = designer.add_node("vec3", DVec2::new(200.0, 200.0));

    // export_atoms.molecule is declared HasAtoms — identity conversion.
    assert!(
        designer.can_connect_nodes(app, 0, xyz, 0),
        "HasAtoms output must be connectable to a HasAtoms input pin"
    );
    // structure_move.input is HasStructure — cross-abstract edges stay rejected.
    assert!(
        !designer.can_connect_nodes(app, 0, smove, 0),
        "HasAtoms output must not connect to a HasStructure input"
    );
    // vec3.x is Float — abstract → concrete downcasts stay rejected.
    assert!(
        !designer.can_connect_nodes(app, 0, v3, 0),
        "HasAtoms output must not connect to a concrete Float input"
    );
}

#[test]
fn abstract_to_same_abstract_wire_validates_clean() {
    let (mut designer, app) = setup_abstract_return_apply();
    let xyz = designer.add_node("export_atoms", DVec2::new(200.0, 0.0));
    designer.connect_nodes(app, 0, xyz, 0);
    designer.validate_active_network();

    let network = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(
        network.valid,
        "network with the abstract wire must stay valid"
    );
    assert!(
        network.validation_errors.is_empty(),
        "unexpected validation errors: {:?}",
        network
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn validator_flags_abstract_to_concrete_wire_instead_of_skipping() {
    let (mut designer, app) = setup_abstract_return_apply();
    let v3 = designer.add_node("vec3", DVec2::new(200.0, 200.0));

    // Smuggle the wire past the connect gate, the way a hand-edited `.cnnd`
    // or the text-format path could. Before the abstract-output fix the
    // validator skipped this wire entirely (source "unresolvable"), silently
    // accepting an abstract → Float wire.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.nodes.get_mut(&v3).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: app,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }
    designer.validate_active_network();

    let network = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(
        !network.valid,
        "HasAtoms → Float wire must invalidate the network"
    );
    assert!(
        network
            .validation_errors
            .iter()
            .any(|e| e.error_text.contains("Data type mismatch")),
        "expected a data type mismatch error, got: {:?}",
        network
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn abstract_typed_apply_output_evaluates_to_concrete_phase_variant() {
    let (designer, app) = setup_abstract_return_apply();
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("main").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(&stack, app, 0, registry, false, &mut context);
    match result {
        NetworkResult::Molecule(_) => {}
        NetworkResult::Error(msg) => panic!("expected Molecule, got Error: {msg}"),
        other => panic!(
            "expected concrete Molecule despite the static HasAtoms annotation, got {}",
            other.to_display_string()
        ),
    }
}
