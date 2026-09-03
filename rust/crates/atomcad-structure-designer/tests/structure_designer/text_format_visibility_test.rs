//! `visible: …` — the text-format spelling of a node's whole display state
//! (`NodeNetwork::displayed_nodes`): the display type and the set of displayed
//! output pins.
//!
//! `true` is Normal + pin 0 and prints exactly as it always has; a list of
//! output-pin names shows exactly those pins; `ghost` is the Ghost display
//! type; `{ pins: […], ghost: true }` is the combination. A mentioned
//! property assigns the whole state.

use atomcad_structure_designer::node_network::{NodeDisplayState, NodeDisplayType};
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::serialize_network;
use std::collections::HashSet;

fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

fn edit(sd: &mut StructureDesigner, code: &str) -> Vec<String> {
    sd.ai_text_edit(code, false);
    let record = sd.ai_edit_log.last().unwrap();
    assert!(record.applied, "{:?}", record.errors);
    record.warnings.clone()
}

fn text(sd: &StructureDesigner) -> String {
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    serialize_network(network, &sd.node_type_registry, Some("main"))
}

fn display(sd: &StructureDesigner, name: &str) -> Option<NodeDisplayState> {
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    let id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some(name))
        .map(|(id, _)| *id)
        .unwrap();
    network.displayed_nodes.get(&id).cloned()
}

fn state(display_type: NodeDisplayType, pins: &[i32]) -> NodeDisplayState {
    NodeDisplayState {
        display_type,
        displayed_pins: pins.iter().copied().collect::<HashSet<_>>(),
    }
}

#[test]
fn true_is_normal_pin_zero_and_prints_unchanged() {
    let mut sd = designer();
    let warnings = edit(&mut sd, "u = structure_unpack { visible: true }\n");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Normal, &[0]))
    );
    assert!(text(&sd).contains("visible: true"), "{}", text(&sd));
}

#[test]
fn a_pin_list_shows_exactly_those_pins_and_round_trips() {
    let mut sd = designer();
    // structure_unpack's output pins: lattice_vecs, motif, motif_offset.
    let warnings = edit(
        &mut sd,
        "u = structure_unpack { visible: [motif_offset, motif] }\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Normal, &[1, 2]))
    );

    let first = text(&sd);
    assert!(
        first.contains("visible: [motif, motif_offset]"),
        "written by name, in pin order:\n{first}"
    );

    sd.ai_text_edit(&first, true);
    assert!(sd.ai_edit_log.last().unwrap().applied);
    assert_eq!(text(&sd), first, "and round-trips");
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Normal, &[1, 2]))
    );
}

#[test]
fn a_list_naming_pin_zero_alone_prints_as_true() {
    let mut sd = designer();
    edit(
        &mut sd,
        "u = structure_unpack { visible: [lattice_vecs] }\n",
    );
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Normal, &[0]))
    );
    assert!(text(&sd).contains("visible: true"), "{}", text(&sd));
}

#[test]
fn ghost_is_the_ghost_display_type_and_round_trips() {
    let mut sd = designer();
    let warnings = edit(&mut sd, "u = structure_unpack { visible: ghost }\n");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(display(&sd, "u"), Some(state(NodeDisplayType::Ghost, &[0])));

    let first = text(&sd);
    assert!(first.contains("visible: ghost"), "{first}");
    sd.ai_text_edit(&first, true);
    assert!(sd.ai_edit_log.last().unwrap().applied);
    assert_eq!(text(&sd), first);
    assert_eq!(display(&sd, "u"), Some(state(NodeDisplayType::Ghost, &[0])));
}

#[test]
fn ghost_with_a_custom_pin_set_uses_the_object_form() {
    let mut sd = designer();
    let warnings = edit(
        &mut sd,
        "u = structure_unpack { visible: { pins: [lattice_vecs, motif], ghost: true } }\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Ghost, &[0, 1]))
    );

    let first = text(&sd);
    assert!(
        first.contains("visible: { pins: [lattice_vecs, motif], ghost: true }"),
        "{first}"
    );
    sd.ai_text_edit(&first, true);
    assert!(sd.ai_edit_log.last().unwrap().applied);
    assert_eq!(text(&sd), first);
}

#[test]
fn a_mentioned_visible_assigns_the_whole_state() {
    let mut sd = designer();
    edit(
        &mut sd,
        "u = structure_unpack { visible: [lattice_vecs, motif, motif_offset] }\n",
    );
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Normal, &[0, 1, 2]))
    );

    edit(&mut sd, "u = structure_unpack {}\n");
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Normal, &[0, 1, 2])),
        "an incremental edit that does not mention visible leaves the pins"
    );

    edit(&mut sd, "u = structure_unpack { visible: true }\n");
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Normal, &[0])),
        "`true` brings a three-pin node back to pin 0"
    );
}

#[test]
fn an_unknown_pin_name_warns_and_the_rest_still_lands() {
    let mut sd = designer();
    let warnings = edit(&mut sd, "u = structure_unpack { visible: [motif, nope] }\n");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("Output pin 'nope' not found"),
        "{warnings:?}"
    );
    assert_eq!(
        display(&sd, "u"),
        Some(state(NodeDisplayType::Normal, &[1]))
    );
}

#[test]
fn an_unrecognised_value_warns_and_leaves_the_node_invisible() {
    let mut sd = designer();
    let warnings = edit(&mut sd, "u = structure_unpack { visible: 3 }\n");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("`visible` on 'u'"), "{warnings:?}");
    assert_eq!(display(&sd, "u"), None);
    assert!(!text(&sd).contains("visible"), "{}", text(&sd));
}

#[test]
fn a_pin_list_inside_a_body_resolves_in_that_scope() {
    let mut sd = designer();
    let warnings = edit(
        &mut sd,
        "m = map {\n  input_type: Structure,\n  output_type: Motif,\n  body {\n    u = structure_unpack { structure: $element, visible: [motif] }\n    output u\n  }\n}\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    let m = network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some("m"))
        .unwrap();
    let body = m.zone.as_deref().unwrap();
    let (u_id, _) = body
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("u"))
        .unwrap();
    assert_eq!(
        body.displayed_nodes.get(u_id).cloned(),
        Some(state(NodeDisplayType::Normal, &[1]))
    );
    let first = text(&sd);
    assert!(first.contains("visible: [motif]"), "{first}");
    sd.ai_text_edit(&first, true);
    assert!(sd.ai_edit_log.last().unwrap().applied);
    assert_eq!(text(&sd), first);
}
