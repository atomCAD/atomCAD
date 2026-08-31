//! Phase 1 of `doc/design_wire_annotations.md` (issue #427) — the Rust core of
//! comment anchors: `CommentAnchor` / `WireAnchor` / `ResolvedAnchor`,
//! `CommentData.anchors`, `resolve`, and the drop-dangling pass in
//! `repair_node_network`.
//!
//! The two behaviours worth stating up front, because they are the ones a
//! future change is most likely to break:
//!
//! - **D4 precedence.** A wire anchor addresses its destination slot by
//!   `destination_param_id` when the destination's parameters carry persistent
//!   ids (the dynamic-arity node types) and by `destination_argument_index`
//!   otherwise. A pin reorder therefore *remaps* the anchor rather than
//!   silently re-pointing it at whatever field slid into the old slot.
//! - **D6 drop, never re-resolve.** A dangling anchor is removed. A leader line
//!   pointing at the wrong wire is documentation that actively lies, so the
//!   only two outcomes are "the same target" or "no anchor".

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_network::{ArgumentKind, IncomingWire, NodeNetwork};
use atomcad_structure_designer::node_type_registry::{
    FieldId, NodeTypeRegistry, RecordFieldEdit, RecordTypeDef,
};
use atomcad_structure_designer::nodes::comment::{
    CommentAnchor, CommentData, ResolvedAnchor, WireAnchor,
};
use atomcad_structure_designer::serialization::node_networks_serialization::{
    node_network_to_serializable, serializable_to_node_network,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::DVec2;
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

fn network<'a>(designer: &'a StructureDesigner, name: &str) -> &'a NodeNetwork {
    designer.node_type_registry.node_networks.get(name).unwrap()
}

/// `repair_node_network` takes `&self` on the registry and `&mut` on the
/// network, so the network is lifted out and put back — the established
/// remove/edit/reinsert idiom.
fn repair(designer: &mut StructureDesigner, name: &str) {
    let mut net = designer
        .node_type_registry
        .node_networks
        .remove(name)
        .unwrap();
    designer.node_type_registry.repair_node_network(&mut net);
    designer
        .node_type_registry
        .node_networks
        .insert(name.to_string(), net);
}

fn set_anchors(
    designer: &mut StructureDesigner,
    name: &str,
    comment_id: u64,
    anchors: Vec<CommentAnchor>,
) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(name)
        .unwrap();
    let node = net.nodes.get_mut(&comment_id).unwrap();
    node.data = Box::new(CommentData {
        anchors,
        ..CommentData::default()
    });
}

fn anchors_of(designer: &StructureDesigner, name: &str, comment_id: u64) -> Vec<CommentAnchor> {
    network(designer, name)
        .nodes
        .get(&comment_id)
        .expect("comment node must exist")
        .data
        .as_any_ref()
        .downcast_ref::<CommentData>()
        .expect("node must be a comment")
        .anchors
        .clone()
}

/// A wire anchor keyed purely by slot index — the built-in-destination shape,
/// where the pin layout is fixed and carries no parameter ids.
fn index_anchor(dest: u64, arg_index: usize, source: u64) -> CommentAnchor {
    CommentAnchor::Wire(WireAnchor {
        destination_node_id: dest,
        destination_argument_kind: ArgumentKind::External,
        destination_argument_index: arg_index,
        destination_param_id: None,
        source_node_id: source,
    })
}

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

fn set_record_construct(designer: &mut StructureDesigner, name: &str, node_id: u64, schema: &str) {
    use atomcad_structure_designer::nodes::record_construct::RecordConstructData;
    let registry = &mut designer.node_type_registry;
    let net = registry.node_networks.get_mut(name).unwrap();
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

fn wire(
    designer: &mut StructureDesigner,
    name: &str,
    source: u64,
    pin: i32,
    dest: u64,
    arg: usize,
) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(name)
        .unwrap();
    net.nodes.get_mut(&dest).unwrap().arguments[arg]
        .incoming_wires
        .push(IncomingWire::node_output(source, pin));
}

/// A `float -> vec3.y` graph plus an unanchored comment. `vec3` is a built-in
/// with a fixed three-pin layout and `id: None` on every parameter, so it is
/// the index-addressed half of D4.
fn float_into_vec3(designer: &mut StructureDesigner, name: &str) -> (u64, u64, u64) {
    let source = designer.add_node("float", DVec2::new(0.0, 0.0));
    let dest = designer.add_node("vec3", DVec2::new(200.0, 0.0));
    let comment = designer.add_node("Comment", DVec2::new(0.0, 200.0));
    wire(designer, name, source, 0, dest, 1);
    (source, dest, comment)
}

// ============================================================================
// resolve
// ============================================================================

#[test]
fn wire_anchor_resolves_on_builtin_destination_by_index() {
    let mut designer = setup("Main");
    let (source, dest, _comment) = float_into_vec3(&mut designer, "Main");

    let anchor = index_anchor(dest, 1, source);
    let resolved = anchor
        .resolve(network(&designer, "Main"))
        .expect("anchor must resolve");

    match resolved {
        ResolvedAnchor::Wire(w) => {
            assert_eq!(w.source_node_id, source);
            assert_eq!(w.destination_node_id, dest);
            assert_eq!(w.destination_argument_index, 1);
            assert_eq!(w.destination_argument_kind, ArgumentKind::External);
            assert_eq!(w.source_pin_index(), Some(0));
        }
        other => panic!("expected a wire anchor, got {:?}", other),
    }
}

#[test]
fn node_anchor_resolves_to_its_target() {
    let mut designer = setup("Main");
    let (source, _dest, _comment) = float_into_vec3(&mut designer, "Main");

    let resolved = CommentAnchor::Node(source)
        .resolve(network(&designer, "Main"))
        .expect("node anchor must resolve");
    assert_eq!(resolved, ResolvedAnchor::Node(source));
}

#[test]
fn wire_anchor_resolves_on_dynamic_arity_destination_by_param_id() {
    let mut designer = setup("Main");
    designer.add_record_type_def(triple_def()).unwrap();

    let source = designer.add_node("int", DVec2::new(0.0, 0.0));
    let holder = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", holder, "Triple");
    // Authored order is [a, b, c], so field `b` is slot 1.
    wire(&mut designer, "Main", source, 0, holder, 1);

    let id_b = field_id(&designer, "Triple", "b");
    let anchor = CommentAnchor::Wire(WireAnchor {
        destination_node_id: holder,
        destination_argument_kind: ArgumentKind::External,
        // Deliberately WRONG index: the param id must win (D4).
        destination_argument_index: 99,
        destination_param_id: Some(id_b.0),
        source_node_id: source,
    });

    match anchor
        .resolve(network(&designer, "Main"))
        .expect("param-id path must resolve")
    {
        ResolvedAnchor::Wire(w) => {
            assert_eq!(w.destination_argument_index, 1);
            assert_eq!(w.source_node_id, source);
        }
        other => panic!("expected a wire anchor, got {:?}", other),
    }
}

#[test]
fn from_wire_captures_the_destination_param_id() {
    let mut designer = setup("Main");
    designer.add_record_type_def(triple_def()).unwrap();

    let source = designer.add_node("int", DVec2::new(0.0, 0.0));
    let holder = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", holder, "Triple");
    wire(&mut designer, "Main", source, 0, holder, 2);

    let net = network(&designer, "Main");
    let w = WireAnchor {
        destination_node_id: holder,
        destination_argument_kind: ArgumentKind::External,
        destination_argument_index: 2,
        destination_param_id: None,
        source_node_id: source,
    }
    .resolve(net)
    .unwrap();

    let anchor = WireAnchor::from_wire(&w, net);
    assert_eq!(
        anchor.destination_param_id,
        Some(field_id(&designer, "Triple", "c").0),
        "a dynamic-arity destination must contribute its persistent param id"
    );

    // The built-in half: `vec3`'s parameters carry no ids, so there is nothing
    // to capture and the index stays the sole addressing.
    let mut designer2 = setup("Other");
    let (src2, dst2, _c) = float_into_vec3(&mut designer2, "Other");
    let net2 = network(&designer2, "Other");
    let w2 = WireAnchor {
        destination_node_id: dst2,
        destination_argument_kind: ArgumentKind::External,
        destination_argument_index: 1,
        destination_param_id: None,
        source_node_id: src2,
    }
    .resolve(net2)
    .unwrap();
    assert_eq!(WireAnchor::from_wire(&w2, net2).destination_param_id, None);
}

// ============================================================================
// D4 — a pin reorder remaps rather than drops
// ============================================================================

#[test]
fn wire_anchor_survives_a_pin_reorder_on_a_dynamic_arity_node() {
    let mut designer = setup("Main");
    designer.add_record_type_def(triple_def()).unwrap();

    let source = designer.add_node("int", DVec2::new(0.0, 0.0));
    let holder = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", holder, "Triple");
    let comment = designer.add_node("Comment", DVec2::new(0.0, 200.0));
    wire(&mut designer, "Main", source, 0, holder, 0); // field `a`

    let id_a = field_id(&designer, "Triple", "a");
    let id_b = field_id(&designer, "Triple", "b");
    let id_c = field_id(&designer, "Triple", "c");
    set_anchors(
        &mut designer,
        "Main",
        comment,
        vec![CommentAnchor::Wire(WireAnchor {
            destination_node_id: holder,
            destination_argument_kind: ArgumentKind::External,
            destination_argument_index: 0,
            destination_param_id: Some(id_a.0),
            source_node_id: source,
        })],
    );

    // Reorder Triple's fields to [c, a, b] — `a` moves from slot 0 to slot 1.
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

    let kept = anchors_of(&designer, "Main", comment);
    assert_eq!(kept.len(), 1, "the anchor must survive a pure reorder");

    match kept[0]
        .resolve(network(&designer, "Main"))
        .expect("the reordered anchor must still resolve")
    {
        ResolvedAnchor::Wire(w) => {
            assert_eq!(
                w.destination_argument_index, 1,
                "the anchor must follow field `a` to its new slot, not stay on slot 0"
            );
            assert_eq!(w.source_node_id, source);
        }
        other => panic!("expected a wire anchor, got {:?}", other),
    }
}

/// The canvas addresses a wire by slot **index** only — it never sees
/// persistent parameter ids — so `set_comment_anchors` canonicalizes what it
/// is handed. Without that, an anchor authored by the Phase 4 drag gesture
/// would silently forfeit D4's remap-on-reorder and start pointing at whatever
/// field slid into its slot.
#[test]
fn set_comment_anchors_canonicalizes_an_index_only_wire_anchor() {
    let mut designer = setup("Main");
    designer.add_record_type_def(triple_def()).unwrap();

    let source = designer.add_node("int", DVec2::new(0.0, 0.0));
    let holder = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", holder, "Triple");
    let comment = designer.add_node("Comment", DVec2::new(0.0, 200.0));
    wire(&mut designer, "Main", source, 0, holder, 0); // field `a`

    let id_a = field_id(&designer, "Triple", "a");
    let id_b = field_id(&designer, "Triple", "b");
    let id_c = field_id(&designer, "Triple", "c");

    // What the drag gesture sends: the resolved slot index, no param id.
    designer.set_comment_anchors(&[], comment, vec![index_anchor(holder, 0, source)]);

    assert_eq!(
        anchors_of(&designer, "Main", comment),
        vec![CommentAnchor::Wire(WireAnchor {
            destination_node_id: holder,
            destination_argument_kind: ArgumentKind::External,
            destination_argument_index: 0,
            destination_param_id: Some(id_a.0),
            source_node_id: source,
        })],
        "the stored anchor must have picked up field `a`'s persistent id"
    );

    // And the id is load-bearing: reorder Triple to [c, a, b].
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

    let kept = anchors_of(&designer, "Main", comment);
    assert_eq!(kept.len(), 1, "the canonicalized anchor must survive");
    match kept[0]
        .resolve(network(&designer, "Main"))
        .expect("it must still resolve")
    {
        ResolvedAnchor::Wire(w) => assert_eq!(
            w.destination_argument_index, 1,
            "it must follow field `a` to its new slot"
        ),
        other => panic!("expected a wire anchor, got {:?}", other),
    }
}

#[test]
fn wire_anchor_drops_when_its_destination_field_is_deleted() {
    let mut designer = setup("Main");
    designer.add_record_type_def(triple_def()).unwrap();

    let source = designer.add_node("int", DVec2::new(0.0, 0.0));
    let holder = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", holder, "Triple");
    let comment = designer.add_node("Comment", DVec2::new(0.0, 200.0));
    wire(&mut designer, "Main", source, 0, holder, 0); // field `a`

    let id_a = field_id(&designer, "Triple", "a");
    let id_b = field_id(&designer, "Triple", "b");
    let id_c = field_id(&designer, "Triple", "c");
    set_anchors(
        &mut designer,
        "Main",
        comment,
        vec![CommentAnchor::Wire(WireAnchor {
            destination_node_id: holder,
            destination_argument_kind: ArgumentKind::External,
            destination_argument_index: 0,
            destination_param_id: Some(id_a.0),
            source_node_id: source,
        })],
    );

    // Drop field `a` entirely. Its slot is now occupied by `b`; re-pointing
    // there is exactly what D6 forbids.
    designer
        .update_record_type_def_with_ids(
            "Triple",
            vec![
                existing(id_b, "b", DataType::Int),
                existing(id_c, "c", DataType::Int),
            ],
        )
        .unwrap();

    assert!(
        anchors_of(&designer, "Main", comment).is_empty(),
        "an anchor whose destination field was deleted must drop, not slide"
    );
}

// ============================================================================
// D6 — dangling anchors drop
// ============================================================================

#[test]
fn wire_anchor_drops_when_the_wire_is_deleted() {
    let mut designer = setup("Main");
    let (source, dest, comment) = float_into_vec3(&mut designer, "Main");
    set_anchors(
        &mut designer,
        "Main",
        comment,
        vec![index_anchor(dest, 1, source)],
    );

    // Disconnect the wire, leaving both endpoint nodes in place.
    designer
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .get_mut(&dest)
        .unwrap()
        .arguments[1]
        .clear();

    repair(&mut designer, "Main");
    assert!(anchors_of(&designer, "Main", comment).is_empty());
}

#[test]
fn wire_anchor_drops_when_the_source_node_is_deleted() {
    let mut designer = setup("Main");
    let (source, dest, comment) = float_into_vec3(&mut designer, "Main");
    set_anchors(
        &mut designer,
        "Main",
        comment,
        vec![index_anchor(dest, 1, source)],
    );

    designer
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .remove(&source);

    repair(&mut designer, "Main");
    assert!(anchors_of(&designer, "Main", comment).is_empty());
}

#[test]
fn wire_anchor_drops_when_the_destination_node_is_deleted() {
    let mut designer = setup("Main");
    let (source, dest, comment) = float_into_vec3(&mut designer, "Main");
    set_anchors(
        &mut designer,
        "Main",
        comment,
        vec![index_anchor(dest, 1, source)],
    );

    designer
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .remove(&dest);

    repair(&mut designer, "Main");
    assert!(anchors_of(&designer, "Main", comment).is_empty());
}

#[test]
fn node_anchor_drops_when_its_target_is_deleted() {
    let mut designer = setup("Main");
    let (source, _dest, comment) = float_into_vec3(&mut designer, "Main");
    set_anchors(
        &mut designer,
        "Main",
        comment,
        vec![CommentAnchor::Node(source)],
    );

    designer
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .remove(&source);

    repair(&mut designer, "Main");
    assert!(anchors_of(&designer, "Main", comment).is_empty());
}

#[test]
fn repair_keeps_the_live_anchors_and_drops_only_the_dangling_one() {
    let mut designer = setup("Main");
    let (source, dest, comment) = float_into_vec3(&mut designer, "Main");
    let orphan = designer.add_node("float", DVec2::new(0.0, 400.0));

    set_anchors(
        &mut designer,
        "Main",
        comment,
        vec![
            index_anchor(dest, 1, source),
            CommentAnchor::Node(orphan),
            CommentAnchor::Node(dest),
        ],
    );

    designer
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .remove(&orphan);

    repair(&mut designer, "Main");
    assert_eq!(
        anchors_of(&designer, "Main", comment),
        vec![index_anchor(dest, 1, source), CommentAnchor::Node(dest)],
        "surviving anchors keep their order; only the dangling one is removed"
    );
}

#[test]
fn repair_leaves_a_fully_live_anchor_list_untouched() {
    let mut designer = setup("Main");
    let (source, dest, comment) = float_into_vec3(&mut designer, "Main");
    let anchors = vec![index_anchor(dest, 1, source), CommentAnchor::Node(source)];
    set_anchors(&mut designer, "Main", comment, anchors.clone());

    repair(&mut designer, "Main");
    assert_eq!(anchors_of(&designer, "Main", comment), anchors);
}

// ============================================================================
// Serialization / back-compat
// ============================================================================

#[test]
fn unanchored_comment_serializes_without_an_anchors_field() {
    let json = serde_json::to_value(CommentData::default()).unwrap();
    let object = json.as_object().unwrap();
    let mut keys: Vec<&str> = object.keys().map(|k| k.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["height", "label", "text", "width"],
        "an unanchored comment must serialize byte-identically to the pre-anchor shape"
    );
}

#[test]
fn pre_anchor_comment_json_loads_with_no_anchors() {
    let json = serde_json::json!({
        "label": "chassis",
        "text": "the load-bearing frame",
        "width": 250.0,
        "height": 150.0,
    });
    let data: CommentData = serde_json::from_value(json).unwrap();
    assert_eq!(data.label, "chassis");
    assert!(data.anchors.is_empty());
}

#[test]
fn anchors_round_trip_through_the_cnnd_node_data() {
    let mut designer = setup("Main");
    let (source, dest, comment) = float_into_vec3(&mut designer, "Main");
    let anchors = vec![index_anchor(dest, 1, source), CommentAnchor::Node(dest)];
    set_anchors(&mut designer, "Main", comment, anchors.clone());

    let built_ins = designer.node_type_registry.built_in_node_types.clone();
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap();
    let serializable = node_network_to_serializable(net, &built_ins, None).unwrap();
    let reloaded = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    let restored = reloaded
        .nodes
        .get(&comment)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<CommentData>()
        .unwrap();
    assert_eq!(restored.anchors, anchors);

    // And the reloaded anchors still resolve against the reloaded network.
    assert!(restored.anchors[0].resolve(&reloaded).is_some());
}
