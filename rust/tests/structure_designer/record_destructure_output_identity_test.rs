//! Phase R3 of `doc/design_record_field_identity.md` — `record_destructure`
//! **output** wires must honor field identity across a field *reorder*.
//!
//! Output wires live on the *consumer* as `(source_node, output_pin_index)` and
//! are keyed by index, so a pure rename leaves them intact — but a **reorder**
//! silently re-points every output wire at the wrong field's value (§4.4). R3
//! stamps the field's `FieldId` onto `OutputPinDefinition` and teaches
//! `repair_node_network` to remap output wires by id (and drop wires whose field
//! was deleted) when the source is a record node.
//!
//! - [RED-FIRST] `destructure_reorder_remaps_output_wires_by_identity` — was
//!   written against the index-keyed code and **observed failing** (every output
//!   wire pointed at the field that *moved into* its old slot). Green once the
//!   id-aware remap lands.
//! - [GUARD] `construct_input_reorder_preserves_wires` — reordering preserves
//!   `record_construct` *input* wires; already green pre-R3 via R2's id-first
//!   `set_custom_node_type` matching. Documents that R3 keeps it green.

use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_network::IncomingWire;
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    FieldId, NodeTypeRegistry, RecordFieldEdit, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::nodes::record_destructure::RecordDestructureData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashMap;

// ============================================================================
// Helpers
// ============================================================================

fn setup(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// `Triple { a: Int, b: Int, c: Int }` — authored order a, b, c.
fn triple_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Triple".to_string(),
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
            ("c".to_string(), DataType::Int),
        ],
    )
}

/// `Holder { x: Int, y: Int, z: Int }` — the sink. A *different* def, so
/// reordering `Triple` never reorders `Holder`'s own pins.
fn holder_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Holder".to_string(),
        vec![
            ("x".to_string(), DataType::Int),
            ("y".to_string(), DataType::Int),
            ("z".to_string(), DataType::Int),
        ],
    )
}

fn field_id(designer: &StructureDesigner, def: &str, field: &str) -> FieldId {
    designer.node_type_registry.record_type_defs[def]
        .fields
        .iter()
        .find(|f| f.name == field)
        .unwrap_or_else(|| panic!("field '{}' not found in def '{}'", field, def))
        .id
}

fn existing(id: FieldId, name: &str, ty: DataType) -> RecordFieldEdit {
    RecordFieldEdit {
        id: Some(id),
        name: name.to_string(),
        data_type: ty,
        hint: None,
    }
}

/// Make `node_id` a `record_destructure` bound to `schema`, populating its
/// per-field output-pin layout from the registry.
fn set_record_destructure(
    designer: &mut StructureDesigner,
    network: &str,
    node_id: u64,
    schema: &str,
) {
    let registry = &mut designer.node_type_registry;
    let net = registry.node_networks.get_mut(network).unwrap();
    let node = net.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(RecordDestructureData {
        schema: schema.to_string(),
    });
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

fn set_record_construct(
    designer: &mut StructureDesigner,
    network: &str,
    node_id: u64,
    schema: &str,
) {
    let registry = &mut designer.node_type_registry;
    let net = registry.node_networks.get_mut(network).unwrap();
    let node = net.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(RecordConstructData {
        schema: schema.to_string(),
        literal_values: HashMap::new(),
    });
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// Push a single output wire `(source pin `pin_index`) -> dest arg `arg_index``.
fn wire_output(
    designer: &mut StructureDesigner,
    network: &str,
    source_node_id: u64,
    pin_index: i32,
    dest_node_id: u64,
    arg_index: usize,
) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap();
    let dest = net.nodes.get_mut(&dest_node_id).unwrap();
    dest.arguments[arg_index]
        .incoming_wires
        .push(IncomingWire::node_output(source_node_id, pin_index));
}

/// The output-pin index that `dest_node`'s `arg_index`-th argument reads from
/// `source_node` (panics if there is no such wire).
fn consumer_wire_pin(
    designer: &StructureDesigner,
    network: &str,
    dest_node_id: u64,
    arg_index: usize,
    source_node_id: u64,
) -> i32 {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    let dest = net.nodes.get(&dest_node_id).unwrap();
    dest.arguments[arg_index]
        .incoming_wires
        .iter()
        .find(|w| w.source_node_id == source_node_id)
        .and_then(|w| w.as_legacy_pair())
        .map(|(_, pin)| pin)
        .unwrap_or_else(|| panic!("no wire from {} on arg {}", source_node_id, arg_index))
}

/// The field name of `destr_node`'s output pin at `pin_index`.
fn destr_pin_name(
    designer: &StructureDesigner,
    network: &str,
    destr_node_id: u64,
    pin_index: i32,
) -> String {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    let destr = net.nodes.get(&destr_node_id).unwrap();
    destr
        .custom_node_type
        .as_ref()
        .expect("record_destructure must have a cached custom_node_type")
        .output_pins[pin_index as usize]
        .name
        .clone()
}

// ============================================================================
// [RED-FIRST] reorder must remap destructure output wires by identity
// ============================================================================

#[test]
fn destructure_reorder_remaps_output_wires_by_identity() {
    let mut designer = setup("Main");
    designer.add_record_type_def(triple_def()).unwrap();
    designer.add_record_type_def(holder_def()).unwrap();

    // A dummy Int source for the destructure input (not load-bearing for the
    // output-wire test; just gives the destructure something on pin 0).
    let _src = designer.add_node("int", DVec2::new(-200.0, 0.0));

    let destr = designer.add_node("record_destructure", DVec2::new(0.0, 0.0));
    set_record_destructure(&mut designer, "Main", destr, "Triple");

    let holder = designer.add_node("record_construct", DVec2::new(300.0, 0.0));
    set_record_construct(&mut designer, "Main", holder, "Holder");

    // Wire each Triple output to a distinct Holder field:
    //   Holder.x <- Triple.a (pin 0), Holder.y <- Triple.b (pin 1),
    //   Holder.z <- Triple.c (pin 2).
    wire_output(&mut designer, "Main", destr, 0, holder, 0); // a -> x
    wire_output(&mut designer, "Main", destr, 1, holder, 1); // b -> y
    wire_output(&mut designer, "Main", destr, 2, holder, 2); // c -> z

    // Precondition: each holder arg reads the matching Triple field by name.
    assert_eq!(
        destr_pin_name(
            &designer,
            "Main",
            destr,
            consumer_wire_pin(&designer, "Main", holder, 0, destr)
        ),
        "a"
    );
    assert_eq!(
        destr_pin_name(
            &designer,
            "Main",
            destr,
            consumer_wire_pin(&designer, "Main", holder, 1, destr)
        ),
        "b"
    );
    assert_eq!(
        destr_pin_name(
            &designer,
            "Main",
            destr,
            consumer_wire_pin(&designer, "Main", holder, 2, destr)
        ),
        "c"
    );

    // Reorder Triple's fields to [c, a, b] (authored order changes; ids stay).
    let id_a = field_id(&designer, "Triple", "a");
    let id_b = field_id(&designer, "Triple", "b");
    let id_c = field_id(&designer, "Triple", "c");
    designer
        .update_record_type_def_with_ids(
            "Triple",
            vec![
                existing(id_c, "c", DataType::Int),
                existing(id_a, "a", DataType::Int),
                existing(id_b, "b", DataType::Int),
            ],
        )
        .unwrap();

    // Each Holder field must STILL read its original Triple field's value:
    // the output wire's pin index must follow the field's identity, not its
    // old slot. RED before R3: every wire still points at its old index, which
    // now hosts the field that reordered into that slot.
    assert_eq!(
        destr_pin_name(
            &designer,
            "Main",
            destr,
            consumer_wire_pin(&designer, "Main", holder, 0, destr)
        ),
        "a",
        "Holder.x must still read Triple.a after reorder"
    );
    assert_eq!(
        destr_pin_name(
            &designer,
            "Main",
            destr,
            consumer_wire_pin(&designer, "Main", holder, 1, destr)
        ),
        "b",
        "Holder.y must still read Triple.b after reorder"
    );
    assert_eq!(
        destr_pin_name(
            &designer,
            "Main",
            destr,
            consumer_wire_pin(&designer, "Main", holder, 2, destr)
        ),
        "c",
        "Holder.z must still read Triple.c after reorder"
    );
}

// ============================================================================
// [RED-FIRST] deleting a field drops only its output wire
// ============================================================================

/// Deleting `Triple.b` drops the wire reading `b` while the wires reading `a`
/// and `c` survive and still read their own fields. Guards the remap against
/// silently re-pointing a deleted field's wire at whatever field slid into its
/// old index (the index-count check alone would keep it pointed at the wrong
/// field).
#[test]
fn destructure_delete_field_drops_only_its_output_wire() {
    let mut designer = setup("Main");
    designer.add_record_type_def(triple_def()).unwrap();
    designer.add_record_type_def(holder_def()).unwrap();

    let _src = designer.add_node("int", DVec2::new(-200.0, 0.0));
    let destr = designer.add_node("record_destructure", DVec2::new(0.0, 0.0));
    set_record_destructure(&mut designer, "Main", destr, "Triple");
    let holder = designer.add_node("record_construct", DVec2::new(300.0, 0.0));
    set_record_construct(&mut designer, "Main", holder, "Holder");

    wire_output(&mut designer, "Main", destr, 0, holder, 0); // a -> x
    wire_output(&mut designer, "Main", destr, 1, holder, 1); // b -> y
    wire_output(&mut designer, "Main", destr, 2, holder, 2); // c -> z

    // Delete field `b` (omit its id), keep a and c by identity.
    let id_a = field_id(&designer, "Triple", "a");
    let id_c = field_id(&designer, "Triple", "c");
    designer
        .update_record_type_def_with_ids(
            "Triple",
            vec![
                existing(id_a, "a", DataType::Int),
                existing(id_c, "c", DataType::Int),
            ],
        )
        .unwrap();

    let net = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .unwrap();
    let holder_node = net.nodes.get(&holder).unwrap();

    // Holder.y (which read the now-deleted `b`) has no wire from destr anymore.
    assert!(
        holder_node.arguments[1]
            .incoming_wires
            .iter()
            .all(|w| w.source_node_id != destr),
        "the wire reading the deleted field b must be dropped"
    );
    // Holder.x still reads `a`, Holder.z still reads `c`.
    assert_eq!(
        destr_pin_name(
            &designer,
            "Main",
            destr,
            consumer_wire_pin(&designer, "Main", holder, 0, destr)
        ),
        "a",
        "Holder.x still reads Triple.a after deleting b"
    );
    assert_eq!(
        destr_pin_name(
            &designer,
            "Main",
            destr,
            consumer_wire_pin(&designer, "Main", holder, 2, destr)
        ),
        "c",
        "Holder.z still reads Triple.c after deleting b"
    );
}

// ============================================================================
// [GUARD] construct-input reorder preserves wires (R2 via id; R3 keeps green)
// ============================================================================

#[test]
fn construct_input_reorder_preserves_wires() {
    let mut designer = setup("Main");
    designer.add_record_type_def(triple_def()).unwrap();

    let va = designer.add_node("int", DVec2::new(0.0, 0.0));
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap();
        net.nodes.get_mut(&va).unwrap().data = Box::new(IntData { value: 1 });
    }
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "Triple");

    // Wire va into field `a` (authored index 0).
    wire_output(&mut designer, "Main", va, 0, rc, 0);

    // Reorder to [b, c, a] — field `a` moves from authored index 0 to index 2.
    let id_a = field_id(&designer, "Triple", "a");
    let id_b = field_id(&designer, "Triple", "b");
    let id_c = field_id(&designer, "Triple", "c");
    designer
        .update_record_type_def_with_ids(
            "Triple",
            vec![
                existing(id_b, "b", DataType::Int),
                existing(id_c, "c", DataType::Int),
                existing(id_a, "a", DataType::Int),
            ],
        )
        .unwrap();

    // The wire follows field `a` to its new input-pin index (2).
    let net = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .unwrap();
    let rc_node = net.nodes.get(&rc).unwrap();
    let nt = rc_node.custom_node_type.as_ref().unwrap();
    let a_idx = nt.parameters.iter().position(|p| p.name == "a").unwrap();
    assert_eq!(a_idx, 2, "field a reordered to input pin index 2");
    assert_eq!(
        rc_node.arguments[a_idx].incoming_wires.len(),
        1,
        "the input wire follows field a to its new pin"
    );
    assert_eq!(
        rc_node.arguments[a_idx].incoming_wires[0].source_node_id, va,
        "and it is still fed by va"
    );
}
