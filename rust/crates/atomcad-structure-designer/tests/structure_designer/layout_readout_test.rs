//! The layout read-out, `query --layout` (`doc/design_deliberate_node_placement.md`
//! A1, Phase 4).
//!
//! Hand-built drawings pin the format and the group rules exactly; demolib
//! snapshots pin what a real hand-drawn network reads like under every filter.

use glam::DVec2;

use atomcad_structure_designer::layout::rendered_node_size;
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;

use super::layout_corpus_test::{demolib_path, load};

// ============================================================================
// Harness
// ============================================================================

fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

/// Add a node of `type_name` at `(x, y)` and rename it `name`.
fn put(sd: &mut StructureDesigner, type_name: &str, name: &str, x: f64, y: f64) {
    let id = sd.add_node(type_name, DVec2::new(x, y));
    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    network.nodes.get_mut(&id).unwrap().custom_name = Some(name.to_string());
}

/// The rendered size of a `float` node, which every hand-built drawing here
/// is made of.
fn float_size(sd: &StructureDesigner) -> DVec2 {
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    let node = network
        .nodes
        .values()
        .find(|node| node.node_type_name == "float")
        .expect("a float node");
    rendered_node_size(node, &sd.node_type_registry)
}

fn readout(sd: &StructureDesigner, scope: &str, nodes: &str, bbox: &str) -> String {
    sd.query_active_layout(scope, nodes, bbox)
}

/// A column `w, h, d` (lefts within the tolerance, 30 px apart) and a row
/// `r1, r2` (50 px apart) under it, plus a loner far to the right.
fn column_and_row() -> (StructureDesigner, DVec2) {
    let mut sd = designer();
    put(&mut sd, "float", "w", 100.0, 100.0);
    let size = float_size(&sd);
    let h = size.y;
    put(&mut sd, "float", "h", 104.0, 100.0 + h + 30.0);
    put(&mut sd, "float", "d", 98.0, 100.0 + 2.0 * (h + 30.0));
    let row_y = 100.0 + 3.0 * (h + 30.0) + 100.0;
    put(&mut sd, "float", "r1", 100.0, row_y);
    put(&mut sd, "float", "r2", 100.0 + size.x + 50.0, row_y + 3.0);
    put(&mut sd, "float", "far", 1000.0, 100.0);
    (sd, size)
}

// ============================================================================
// Format and filters
// ============================================================================

#[test]
fn the_unfiltered_readout_lists_every_node_top_then_left_with_groups() {
    let (sd, size) = column_and_row();
    let (w, h) = (size.x, size.y);
    let row_y = 100.0 + 3.0 * (h + 30.0) + 100.0;
    let px = |v: f64| v.round() as i64;
    let expected = format!(
        "# main: extent (98,100)–({},{}), 6 nodes\n\
         w    100,100  {w}×{h}\n\
         far  1000,100  {w}×{h}\n\
         h    104,{}  {w}×{h}\n\
         d    98,{}  {w}×{h}\n\
         r1   100,{}  {w}×{h}\n\
         r2   {},{}  {w}×{h}\n\
         # groups (inferred)\n\
         column  w, h, d  gap 30\n\
         row     r1, r2   gap 50\n",
        px(1000.0 + w),
        px(row_y + 3.0 + h),
        px(100.0 + h + 30.0),
        px(100.0 + 2.0 * (h + 30.0)),
        px(row_y),
        px(100.0 + w + 50.0),
        px(row_y + 3.0),
        w = px(w),
        h = px(h),
    );
    // The node lines are padded to one column width; normalise runs of
    // spaces before comparing the lines, then check the padding separately.
    let got = readout(&sd, "", "", "");
    let squash = |s: &str| {
        s.lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
    };
    assert_eq!(squash(&got), squash(&expected), "got:\n{got}");
    let first = got.lines().nth(1).unwrap();
    let second = got.lines().nth(2).unwrap();
    assert_eq!(
        first.find(char::is_numeric),
        second.find(char::is_numeric),
        "the position column is aligned:\n{got}"
    );
}

#[test]
fn nodes_lists_exactly_the_named_ones_and_flags_an_unknown_name() {
    let (sd, _) = column_and_row();
    let got = readout(&sd, "", "d,nope,w", "");
    let lines: Vec<&str> = got.lines().collect();
    assert!(lines[0].ends_with("6 nodes; 2 listed"), "{got}");
    assert!(
        lines[1].starts_with("w "),
        "sorted by top, not by the request"
    );
    assert!(lines[2].starts_with("d "));
    assert_eq!(
        lines[3]
            .split_once(' ')
            .map(|(name, rest)| (name, rest.trim())),
        Some(("nope", "(no such node)"))
    );
    assert_eq!(
        lines[4..],
        ["# groups (inferred)", "column  w, …, d  gap 30"],
        "the column holds a listed node, so it is listed; the unlisted member is `…`"
    );
}

#[test]
fn a_box_selects_what_intersects_it_and_keeps_a_cut_column_whole() {
    let (sd, size) = column_and_row();
    // Covers `w` and the top of `h`; `d` is outside.
    let bbox = format!("50,50,300,{}", 100.0 + size.y + 40.0);
    let got = readout(&sd, "", "", &bbox);
    let lines: Vec<&str> = got.lines().collect();
    assert!(
        lines[0].ends_with(&format!(
            "; 2 in box (50,50)–(300,{})",
            (100.0 + size.y + 40.0).round() as i64
        )),
        "{got}"
    );
    assert!(lines[1].starts_with("w "));
    assert!(lines[2].starts_with("h "));
    assert_eq!(
        lines[3..],
        ["# groups (inferred)", "column  w, h, …  gap 30"],
        "groups are detected over the whole scope, so the box cuts the listing, not the column"
    );
}

#[test]
fn a_box_over_free_space_is_empty_and_corners_may_come_in_any_order() {
    let (sd, _) = column_and_row();
    let got = readout(&sd, "", "", "700,900,600,800");
    assert_eq!(got.lines().count(), 1, "no nodes, no groups:\n{got}");
    assert!(got.ends_with("; 0 in box (600,800)–(700,900)\n"), "{got}");
}

#[test]
fn a_box_that_only_touches_a_node_does_not_select_it() {
    let (sd, _) = column_and_row();
    // `w`'s left edge is x = 100.
    let got = readout(&sd, "", "", "0,0,100,150");
    assert!(got.contains("; 0 in box"), "{got}");
}

#[test]
fn nodes_and_box_together_are_refused() {
    let (sd, _) = column_and_row();
    assert_eq!(
        readout(&sd, "", "w", "0,0,10,10"),
        "# Error: --nodes and --box are mutually exclusive\n"
    );
    assert!(readout(&sd, "", "", "1,2,3").starts_with("# Error: --box expects"));
}

#[test]
fn an_empty_network_says_so() {
    let sd = designer();
    assert_eq!(readout(&sd, "", "", ""), "# main: no nodes\n");
}

// ============================================================================
// Groups
// ============================================================================

#[test]
fn a_column_whose_lefts_drift_past_the_tolerance_is_cut() {
    let mut sd = designer();
    put(&mut sd, "float", "a", 100.0, 100.0);
    let h = float_size(&sd).y;
    put(&mut sd, "float", "b", 106.0, 100.0 + h + 30.0);
    put(&mut sd, "float", "c", 112.0, 100.0 + 2.0 * (h + 30.0));
    let got = readout(&sd, "", "", "");
    assert!(
        got.contains("column  a, b  gap 30"),
        "a and b are 6 px apart, c is 12 px from a: one column of two\n{got}"
    );
    assert!(!got.contains("a, b, c"), "{got}");
}

#[test]
fn a_gap_wider_than_wide_breaks_a_column_and_a_lone_node_is_no_group() {
    let mut sd = designer();
    put(&mut sd, "float", "a", 100.0, 100.0);
    let h = float_size(&sd).y;
    put(&mut sd, "float", "b", 100.0, 100.0 + h + 61.0);
    let got = readout(&sd, "", "", "");
    assert!(
        !got.contains("# groups"),
        "61 px is over `wide` (60)\n{got}"
    );

    put(&mut sd, "float", "c", 100.0, 100.0 + 2.0 * h + 61.0 + 60.0);
    let got = readout(&sd, "", "", "");
    assert!(
        got.contains("column  b, c  gap 60"),
        "exactly `wide` still joins\n{got}"
    );
}

#[test]
fn the_gap_is_the_median() {
    let mut sd = designer();
    let names = ["a", "b", "c", "d"];
    put(&mut sd, "float", names[0], 100.0, 100.0);
    let w = float_size(&sd).x;
    let mut x = 100.0 + w;
    for (name, gap) in names[1..].iter().zip([10.0, 40.0, 90.0]) {
        x += gap;
        put(&mut sd, "float", name, x, 100.0);
        x += w;
    }
    let got = readout(&sd, "", "", "");
    assert!(got.contains("row  a, b, c, d  gap 40"), "{got}");
}

// ============================================================================
// Tags and scopes
// ============================================================================

const MAP_WITH_BODY: &str = r#"
r = range { start: 0, step: 1, count: 5 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, expression: "a * 2", parameters: [{ name: "a", data_type: Int }] }
    e = expr { a: d, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output e
  }
}
output m1
"#;

#[test]
fn tags_name_the_ai_s_nodes_comments_and_expanded_bodies() {
    let mut sd = designer();
    put(&mut sd, "Comment", "note", 100.0, 600.0);
    let outcome = sd.ai_text_edit(MAP_WITH_BODY, false);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);

    let got = readout(&sd, "", "", "");
    let tags = |name: &str| -> Vec<String> {
        let line = got
            .lines()
            .find(|line| line.split_whitespace().next() == Some(name))
            .unwrap_or_else(|| panic!("no line for {name}:\n{got}"));
        line.split_whitespace()
            .skip(3)
            .map(str::to_string)
            .collect()
    };
    assert_eq!(tags("r"), ["ai"]);
    assert_eq!(tags("m1"), ["ai", "body"]);
    assert_eq!(
        tags("note"),
        ["comment"],
        "the user's comment is untagged `ai`"
    );
}

#[test]
fn scope_lists_a_body_in_body_coordinates() {
    let mut sd = designer();
    let outcome = sd.ai_text_edit(MAP_WITH_BODY, false);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);

    let got = readout(&sd, "m1", "", "");
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    let m1 = network
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some("m1"))
        .unwrap();
    let body = m1.zone.as_deref().unwrap();
    let d = body
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some("d"))
        .unwrap();
    assert!(got.starts_with("# main/m1: extent "), "{got}");
    assert!(got.contains("2 nodes"), "{got}");
    let line = got.lines().find(|line| line.starts_with("d ")).unwrap();
    assert_eq!(
        line.split_whitespace().nth(1).unwrap(),
        format!(
            "{},{}",
            d.position.x.round() as i64,
            d.position.y.round() as i64
        ),
        "the stored, body-local position"
    );
    assert!(line.ends_with("ai"), "{line}");
}

#[test]
fn an_unknown_scope_or_a_scope_without_a_body_is_an_error() {
    let mut sd = designer();
    let outcome = sd.ai_text_edit(MAP_WITH_BODY, false);
    assert!(outcome.result.success);
    assert_eq!(
        readout(&sd, "nope", "", ""),
        "# Error: no node 'nope' in main\n"
    );
    assert_eq!(
        readout(&sd, "r", "", ""),
        "# Error: 'r' in main has no body\n"
    );
    assert_eq!(
        readout(&sd, "m1/x", "", ""),
        "# Error: no node 'x' in main/m1\n"
    );
}

#[test]
fn no_active_network_is_reported() {
    let sd = StructureDesigner::new();
    assert_eq!(readout(&sd, "", "", ""), "# No active node network\n");
}

// ============================================================================
// demolib snapshots
// ============================================================================

/// The read-out of real hand-drawn networks, under every filter. demolib has
/// no bodies; the body scope is snapshotted from the `map` above.
#[test]
fn demolib_readouts() {
    let mut sd = load(&demolib_path());
    let mut out = String::new();
    for name in ["demo_rack_x", "lib_iwall_positive"] {
        sd.set_active_node_network_name(Some(name.to_string()));
        out.push_str(&format!("=== {name}: no filter\n"));
        out.push_str(&readout(&sd, "", "", ""));
        // The first two names the full listing prints, and one that is not
        // there.
        let names: Vec<String> = readout(&sd, "", "", "")
            .lines()
            .skip(1)
            .take(2)
            .map(|line| line.split_whitespace().next().unwrap().to_string())
            .collect();
        let nodes = format!("{},{},no_such_node", names[1], names[0]);
        out.push_str(&format!("=== {name}: --nodes {nodes}\n"));
        out.push_str(&readout(&sd, "", &nodes, ""));
        out.push_str(&format!("=== {name}: --box 0,0,400,300\n"));
        out.push_str(&readout(&sd, "", "", "0,0,400,300"));
    }
    insta::assert_snapshot!(out);
}

#[test]
fn body_scope_readout() {
    let mut sd = designer();
    let outcome = sd.ai_text_edit(MAP_WITH_BODY, false);
    assert!(outcome.result.success);
    let mut out = readout(&sd, "", "", "");
    out.push_str(&readout(&sd, "m1", "", ""));
    insta::assert_snapshot!(out);
}

// ============================================================================
// The image frame (`query --layout --image`)
// ============================================================================

use atomcad_structure_designer::layout::IMAGE_FRAME_MARGIN;

/// A box is the frame exactly: the image crops, it does not filter.
#[test]
fn an_image_of_a_box_is_framed_by_the_box() {
    let (sd, _) = column_and_row();
    let frame = sd
        .active_layout_image_frame("", "", "700,600,-50,10")
        .unwrap();
    assert!(frame.scope_ids.is_empty());
    assert_eq!(frame.rect.pos, DVec2::new(-50.0, 10.0));
    assert_eq!(frame.rect.size, DVec2::new(750.0, 590.0));
}

/// `--nodes` frames the named nodes, and no filter the whole scope, each with
/// the margin; an unknown name is skipped, as the read-out lists it.
#[test]
fn an_image_of_nodes_or_of_everything_frames_them_with_the_margin() {
    let (sd, size) = column_and_row();
    let m = IMAGE_FRAME_MARGIN;

    let frame = sd.active_layout_image_frame("", "w,far,nope", "").unwrap();
    assert_eq!(frame.rect.pos, DVec2::new(100.0 - m, 100.0 - m));
    assert_eq!(
        frame.rect.size,
        DVec2::new(1000.0 + size.x - 100.0 + 2.0 * m, size.y + 2.0 * m)
    );

    let all = sd.active_layout_image_frame("", "", "").unwrap();
    // The extent the read-out's header prints, plus the margin.
    assert_eq!(all.rect.pos, DVec2::new(98.0 - m, 100.0 - m));
    assert_eq!(all.rect.right(), 1000.0 + size.x + m);
}

#[test]
fn an_image_with_nothing_to_frame_is_an_error() {
    let (sd, _) = column_and_row();
    assert!(
        sd.active_layout_image_frame("", "nope", "")
            .unwrap_err()
            .contains("none of the named nodes")
    );
    assert!(
        sd.active_layout_image_frame("", "", "5,5,5,90")
            .unwrap_err()
            .contains("no area")
    );
    let empty = designer();
    assert!(
        empty
            .active_layout_image_frame("", "", "")
            .unwrap_err()
            .contains("no nodes")
    );
    assert_eq!(
        empty
            .active_layout_image_frame("", "a", "0,0,1,1")
            .unwrap_err(),
        "--nodes and --box are mutually exclusive"
    );
}

/// A body is framed in body coordinates and names its HOF chain; a body that
/// is collapsed is not drawn, so it cannot be pictured.
#[test]
fn an_image_of_a_body_carries_its_chain_and_a_collapsed_one_is_refused() {
    let mut sd = designer();
    let outcome = sd.ai_text_edit(MAP_WITH_BODY, false);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);
    let m1_id = sd.node_type_registry.node_networks["main"]
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some("m1"))
        .unwrap()
        .id;

    let frame = sd.active_layout_image_frame("m1", "d", "").unwrap();
    assert_eq!(frame.scope_ids, vec![m1_id]);
    let d = sd.node_type_registry.node_networks["main"].nodes[&m1_id]
        .zone
        .as_deref()
        .unwrap()
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some("d"))
        .unwrap()
        .position;
    assert_eq!(frame.rect.pos, d - DVec2::splat(IMAGE_FRAME_MARGIN));

    sd.node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap()
        .nodes
        .get_mut(&m1_id)
        .unwrap()
        .collapse_mode = atomcad_structure_designer::node_network::CollapseMode::Collapsed;
    assert!(
        sd.active_layout_image_frame("m1", "", "")
            .unwrap_err()
            .contains("collapsed")
    );
}
