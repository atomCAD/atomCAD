//! `pin_roles: { … }` — the text-format spelling of a node's function-pin
//! role overrides (`doc/design_function_pin_roles.md`).
//!
//! `Node` state, not `NodeData` state, and keyed by pin index in memory;
//! written by pin name. Absence is `Auto` (never stored), a mentioned property
//! assigns the whole map, and `{}` clears it.

use atomcad_structure_designer::node_network::FunctionPinRole;
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::serialize_network;
use std::collections::BTreeMap;

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

fn roles(sd: &StructureDesigner, name: &str) -> BTreeMap<usize, FunctionPinRole> {
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap()
        .function_pin_roles
        .clone()
}

fn map(entries: &[(usize, FunctionPinRole)]) -> BTreeMap<usize, FunctionPinRole> {
    entries.iter().copied().collect()
}

#[test]
fn roles_are_authored_by_pin_name_and_written_back_in_pin_order() {
    let mut sd = designer();
    let warnings = edit(
        &mut sd,
        "c = cuboid { extent: (1, 1, 1) }\n\
         sm = structure_move { input: c, pin_roles: { translation: supplied, input: delayed } }\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    // structure_move's pins: input, translation, subdivision, subdiv_xyz.
    assert_eq!(
        roles(&sd, "sm"),
        map(&[
            (0, FunctionPinRole::Delayed),
            (1, FunctionPinRole::Supplied)
        ])
    );

    let first = text(&sd);
    assert!(
        first.contains("pin_roles: { input: delayed, translation: supplied }"),
        "written by name, in pin order:\n{first}"
    );

    sd.ai_text_edit(&first, true);
    assert!(sd.ai_edit_log.last().unwrap().applied);
    assert_eq!(text(&sd), first, "and round-trips");
    assert_eq!(
        roles(&sd, "sm"),
        map(&[
            (0, FunctionPinRole::Delayed),
            (1, FunctionPinRole::Supplied)
        ])
    );
}

#[test]
fn a_node_without_overrides_prints_no_pin_roles() {
    let mut sd = designer();
    edit(
        &mut sd,
        "c = cuboid { extent: (1, 1, 1) }\nsm = structure_move { input: c }\n",
    );
    assert!(!text(&sd).contains("pin_roles"), "{}", text(&sd));
}

#[test]
fn an_omitted_property_leaves_roles_alone_and_an_empty_one_clears_them() {
    let mut sd = designer();
    edit(
        &mut sd,
        "c = cuboid { extent: (1, 1, 1) }\nsm = structure_move { input: c, pin_roles: { input: delayed } }\n",
    );

    edit(&mut sd, "sm = structure_move { translation: (2, 0, 0) }\n");
    assert_eq!(
        roles(&sd, "sm"),
        map(&[(0, FunctionPinRole::Delayed)]),
        "an incremental edit that does not mention pin_roles leaves them"
    );

    edit(&mut sd, "sm = structure_move { pin_roles: {} }\n");
    assert!(roles(&sd, "sm").is_empty(), "`{{}}` clears every override");
}

#[test]
fn auto_is_never_stored() {
    let mut sd = designer();
    edit(
        &mut sd,
        "c = cuboid { extent: (1, 1, 1) }\nsm = structure_move { input: c, pin_roles: { input: auto, translation: supplied } }\n",
    );
    assert_eq!(roles(&sd, "sm"), map(&[(1, FunctionPinRole::Supplied)]));
}

#[test]
fn an_unknown_pin_or_role_is_a_warning_and_the_rest_still_lands() {
    let mut sd = designer();
    let warnings = edit(
        &mut sd,
        "c = cuboid { extent: (1, 1, 1) }\n\
         sm = structure_move { input: c, pin_roles: { input: delayed, nope: supplied, translation: sometimes } }\n",
    );
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("no input pin named 'nope'")),
        "{warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("unknown role 'sometimes'")),
        "{warnings:?}"
    );
    assert_eq!(roles(&sd, "sm"), map(&[(0, FunctionPinRole::Delayed)]));
}

#[test]
fn a_role_on_an_applys_derived_argument_pin_resolves_after_its_f_wire() {
    // `apply`'s `arg0…` pins only exist once `f` is wired, and the same
    // statement wires it — roles are resolved after the wire passes.
    let mut sd = designer();
    let warnings = edit(
        &mut sd,
        "sq = expr { expression: \"x * x\" }\n\
         v = int { value: 3 }\n\
         a = apply { f: @sq, arg0: v, pin_roles: { arg0: delayed } }\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(roles(&sd, "a"), map(&[(1, FunctionPinRole::Delayed)]));
    let first = text(&sd);
    assert!(first.contains("pin_roles: { arg0: delayed }"), "{first}");
}
