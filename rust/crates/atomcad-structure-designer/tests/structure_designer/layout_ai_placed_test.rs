//! `Node::ai_placed`, the ownership tag (`doc/design_deliberate_node_placement.md`
//! D12, Phase 4): set on the nodes an AI edit creates, cleared when a human
//! positions one, not carried by copies, never serialized.

use glam::DVec2;

use atomcad_structure_designer::layout::{ArrangeOp, Gap};
use atomcad_structure_designer::node_network::{Node, NodeNetwork};
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::serialization::node_networks_serialization::serialize_registry_to_string;
use atomcad_structure_designer::structure_designer::StructureDesigner;

fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

fn edit(sd: &mut StructureDesigner, code: &str) {
    let outcome = sd.ai_text_edit(code, false);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);
}

fn main_network(sd: &StructureDesigner) -> &NodeNetwork {
    sd.node_type_registry.node_networks.get("main").unwrap()
}

fn named<'a>(network: &'a NodeNetwork, name: &str) -> &'a Node {
    network
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node `{name}`"))
}

fn id_of(sd: &StructureDesigner, name: &str) -> u64 {
    named(main_network(sd), name).id
}

fn ai(sd: &StructureDesigner, name: &str) -> bool {
    named(main_network(sd), name).ai_placed
}

fn select(sd: &mut StructureDesigner, names: &[&str]) {
    let ids: Vec<u64> = names.iter().map(|name| id_of(sd, name)).collect();
    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    network.clear_selection();
    for id in ids {
        network.toggle_node_selection(id);
    }
}

fn drag(sd: &mut StructureDesigner, name: &str, delta: DVec2) {
    select(sd, &[name]);
    sd.begin_move_nodes();
    sd.move_selected_nodes(delta);
    sd.end_move_nodes();
}

/// Every node of `network`, bodies included.
fn all_nodes(network: &NodeNetwork) -> Vec<&Node> {
    let mut out = Vec::new();
    for node in network.nodes.values() {
        out.push(node);
        if let Some(body) = node.zone.as_deref() {
            out.extend(all_nodes(body));
        }
    }
    out
}

const MAP_WITH_BODY: &str = r#"
r = range { start: 0, step: 1, count: 5 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, expression: "a * 2", parameters: [{ name: "a", data_type: Int }] }
    output d
  }
}
output m1
"#;

// ============================================================================
// Set by an AI edit
// ============================================================================

#[test]
fn an_ai_edit_tags_the_nodes_it_creates_and_only_those() {
    let mut sd = designer();
    let user = sd.add_node("float", DVec2::new(100.0, 100.0));
    assert!(!main_network(&sd).nodes[&user].ai_placed);

    edit(&mut sd, MAP_WITH_BODY);
    assert!(ai(&sd, "r"));
    assert!(ai(&sd, "m1"));
    let body = named(main_network(&sd), "m1").zone.as_deref().unwrap();
    assert!(named(body, "d").ai_placed, "body nodes too");
    assert!(
        !main_network(&sd).nodes[&user].ai_placed,
        "a node that was there before is the user's"
    );

    // A second edit that updates an AI node and adds one more.
    edit(
        &mut sd,
        "r = range { start: 0, step: 1, count: 7 }\nk = int { value: 3 }",
    );
    assert!(ai(&sd, "r"), "an update does not take the node from the AI");
    assert!(ai(&sd, "k"));
}

#[test]
fn a_replace_keeps_each_recreated_node_s_owner() {
    let mut sd = designer();
    sd.add_node("float", DVec2::new(100.0, 100.0));
    let user_name = main_network(&sd)
        .nodes
        .values()
        .next()
        .unwrap()
        .custom_name
        .clone()
        .unwrap();
    edit(&mut sd, "k = int { value: 3 }");

    let text = sd.query_active_network_text();
    let outcome = sd.ai_text_edit(&format!("{text}\nz = int {{ value: 1 }}"), true);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);
    assert!(ai(&sd, "k"), "recreated under its old name: still the AI's");
    assert!(ai(&sd, "z"), "new");
    assert!(
        !ai(&sd, &user_name),
        "recreated under its old name: still the user's"
    );
}

#[test]
fn a_dry_run_leaves_every_tag_as_it_was() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    sd.add_node("float", DVec2::new(900.0, 900.0));

    let outcome = sd.ai_text_edit_with_options("k = int { value: 3 }", false, true);
    assert!(outcome.result.success);
    assert!(
        main_network(&sd)
            .nodes
            .values()
            .all(|node| node.custom_name.as_deref() != Some("k")),
        "the preview reverted"
    );
    assert!(
        ai(&sd, "r"),
        "the revert rebuilt the network; the tag survived it"
    );
    assert!(ai(&sd, "m1"));
    let body = named(main_network(&sd), "m1").zone.as_deref().unwrap();
    assert!(named(body, "d").ai_placed);
    let untagged = all_nodes(main_network(&sd))
        .into_iter()
        .filter(|node| !node.ai_placed)
        .count();
    assert_eq!(untagged, 1, "only the user's float");
}

// ============================================================================
// Cleared by a human placement
// ============================================================================

#[test]
fn a_drag_takes_the_node_from_the_ai_and_undo_gives_it_back() {
    let mut sd = designer();
    edit(&mut sd, "a = int { value: 1 }\nb = int { value: 2 }");
    sd.undo_stack.clear();

    drag(&mut sd, "a", DVec2::new(40.0, 0.0));
    assert!(!ai(&sd, "a"));
    assert!(ai(&sd, "b"), "only the dragged node");

    assert!(sd.undo());
    assert!(
        ai(&sd, "a"),
        "one Ctrl+Z puts back the position and the owner"
    );
    assert!(sd.redo());
    assert!(!ai(&sd, "a"));
}

#[test]
fn a_gui_arrange_takes_the_nodes_it_moves_from_the_ai() {
    let mut sd = designer();
    edit(&mut sd, "a = int { value: 1 }\nb = int { value: 2 }");
    let a = id_of(&sd, "a");
    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    network.nodes.get_mut(&a).unwrap().position = DVec2::new(500.0, 40.0);
    sd.undo_stack.clear();

    select(&mut sd, &["a", "b"]);
    assert!(sd.arrange_selected_nodes(&[], ArrangeOp::StackColumn(Gap::Normal)));
    let moved: Vec<&str> = ["a", "b"]
        .into_iter()
        .filter(|name| !ai(&sd, name))
        .collect();
    assert!(!moved.is_empty(), "the arrange moved something");
    for name in &moved {
        assert!(named(main_network(&sd), name).hand_moved);
    }

    assert!(sd.undo());
    assert!(ai(&sd, "a") && ai(&sd, "b"));
}

// ============================================================================
// Not carried by copies
// ============================================================================

#[test]
fn a_duplicate_of_an_ai_node_is_the_user_s() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    let m1 = id_of(&sd, "m1");
    let copy = sd.duplicate_node(m1);

    let copy = &main_network(&sd).nodes[&copy];
    assert!(!copy.ai_placed);
    assert!(
        all_nodes(copy.zone.as_deref().unwrap())
            .iter()
            .all(|node| !node.ai_placed),
        "nor are the copies of its body's nodes"
    );
    assert!(ai(&sd, "m1"), "the original stays the AI's");
    let body = named(main_network(&sd), "m1").zone.as_deref().unwrap();
    assert!(named(body, "d").ai_placed, "and so does its body");
}

#[test]
fn a_pasted_copy_of_an_ai_node_is_the_user_s() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    let before: Vec<u64> = main_network(&sd).nodes.keys().copied().collect();
    select(&mut sd, &["m1"]);
    sd.copy_selection();
    let pasted = sd
        .paste_at_position(DVec2::new(800.0, 800.0))
        .unwrap_or_else(|_| panic!("the paste was refused"));
    assert!(!pasted.is_empty());

    for id in pasted {
        assert!(!before.contains(&id));
        let node = &main_network(&sd).nodes[&id];
        assert!(!node.ai_placed);
        if let Some(body) = node.zone.as_deref() {
            assert!(all_nodes(body).iter().all(|node| !node.ai_placed));
        }
    }
    assert!(ai(&sd, "m1"));
}

// ============================================================================
// Never serialized
// ============================================================================

fn saved_text(sd: &mut StructureDesigner) -> String {
    let rules = sd.cli_access_rules.clone();
    let mode = sd.direct_editing_mode;
    serialize_registry_to_string(&mut sd.node_type_registry, None, mode, &rules)
        .expect("serializes")
}

#[test]
fn the_flag_is_not_saved_and_a_reload_makes_every_node_the_user_s() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    let tagged = saved_text(&mut sd);

    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    atomcad_structure_designer::node_network::walk_all_nodes_mut(network, &mut |node| {
        node.ai_placed = false;
    });
    let untagged = saved_text(&mut sd);
    assert_eq!(
        tagged, untagged,
        "the saved file is byte-identical with the flag cleared"
    );
    assert!(!tagged.contains("ai_placed"));

    // A real round trip through a file.
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    let dir = std::env::temp_dir().join(format!("atomcad_ai_placed_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("roundtrip.cnnd");
    sd.save_node_networks_as(&path.to_string_lossy()).unwrap();
    let mut loaded = StructureDesigner::new();
    loaded
        .load_node_networks(&path.to_string_lossy())
        .expect("loads");
    let _ = std::fs::remove_dir_all(&dir);
    let network = loaded.node_type_registry.node_networks.get("main").unwrap();
    assert!(!all_nodes(network).is_empty());
    assert!(all_nodes(network).iter().all(|node| !node.ai_placed));
}

// ============================================================================
// `hand_moved` is untouched by creation
// ============================================================================

#[test]
fn no_creation_path_sets_hand_moved() {
    let mut sd = designer();
    let a = sd.add_node("float", DVec2::ZERO);
    let b = sd.duplicate_node(a);
    edit(&mut sd, MAP_WITH_BODY);
    select(&mut sd, &["m1"]);
    sd.copy_selection();
    sd.paste_at_position(DVec2::new(600.0, 600.0))
        .unwrap_or_else(|_| panic!("the paste was refused"));

    let network = main_network(&sd);
    assert!(!network.nodes[&a].hand_moved);
    assert!(!network.nodes[&b].hand_moved);
    assert!(
        all_nodes(network).iter().all(|node| !node.hand_moved),
        "GUI add, duplicate, paste and an AI edit claim nothing about a human"
    );
}
