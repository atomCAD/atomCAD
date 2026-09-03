//! The AI-history diff engine — Phase 2 of `doc/design_ai_edit_history.md`.
//!
//! Two properties are what this file exists to pin, and both are about the same
//! failure: a text-format snapshot is **topologically sorted**, so a plain line
//! diff reports a large *move* as a large *change*.
//!
//! - *By node* diffs the **set** of per-node blocks, so a statement that only
//!   moved reads as unchanged;
//! - and the split is **recursive**, so the same false positive cannot come back
//!   one scope down — each body scope runs its own topological sort, and one
//!   node inserted into a 19-node body would otherwise report the whole `map` as
//!   changed.
//!
//! The keys are paths (`m1/d`), never bare names: two bodies may each hold a
//! node called `a`.

use atomcad_structure_designer::ai_edit_diff::{
    AiDiff, DiffHunkKind, DiffLineTag, diff_by_node, diff_text, split_snapshot,
};
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

/// A designer with one empty active network called `main`.
///
/// `StructureDesigner::new()` loads the **real** user preferences, so every
/// preference these tests depend on is pinned here rather than inherited from
/// whichever machine runs the suite.
fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

/// Apply a merge edit and assert the editor accepted it.
fn edit(sd: &mut StructureDesigner, code: &str) {
    let outcome = sd.ai_text_edit(code, false);
    assert!(
        outcome.result.success,
        "edit should have succeeded, errors: {:?}",
        outcome.result.errors
    );
}

/// Apply a merge edit without asserting on the *whole-network* verdict.
///
/// `success` folds in a validation pass over the whole network (D8), so a script
/// that lands perfectly can still report `false` — which says nothing about the
/// diff.
fn edit_applied(sd: &mut StructureDesigner, code: &str) {
    let outcome = sd.ai_text_edit(code, false);
    assert!(
        sd.ai_edit_log.last().is_some_and(|record| record.applied),
        "the statements should have applied, errors: {:?}",
        outcome.result.errors
    );
}

/// The *By node* diff of the most recent entry.
fn last_by_node(sd: &StructureDesigner) -> AiDiff {
    let record = sd.ai_edit_log.last().expect("an entry was recorded");
    diff_by_node(&record.before_text, &record.after_text)
}

/// The *Text* diff of the most recent entry.
fn last_text(sd: &StructureDesigner) -> AiDiff {
    let record = sd.ai_edit_log.last().expect("an entry was recorded");
    diff_text(&record.before_text, &record.after_text)
}

/// The most recent entry's `after_text` — what `query` would have returned.
fn last_after_text(sd: &StructureDesigner) -> String {
    sd.ai_edit_log
        .last()
        .expect("an entry was recorded")
        .after_text
        .clone()
}

/// `(kind, path)` for every hunk, in order.
fn hunks(diff: &AiDiff) -> Vec<(DiffHunkKind, String)> {
    diff.hunks
        .iter()
        .map(|hunk| (hunk.kind, hunk.node_path.clone()))
        .collect()
}

/// The lines of the hunk for `path`, as `(tag, text)`.
fn hunk_lines(diff: &AiDiff, path: &str) -> Vec<(DiffLineTag, String)> {
    diff.hunks
        .iter()
        .find(|hunk| hunk.node_path == path)
        .unwrap_or_else(|| panic!("no hunk for {:?}, got {:?}", path, hunks(diff)))
        .lines
        .iter()
        .map(|line| (line.tag, line.text.clone()))
        .collect()
}

const TWO_SPHERES: &str = r#"
s1 = sphere { radius: 5 }
s2 = sphere { radius: 3 }
output s1
"#;

/// The canonical `map`: a range, a captured constant and a two-node body.
const MAP_WITH_BODY: &str = r#"
r = range { start: 0, step: 1, count: 5 }
scale = int { value: 3 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    e = expr { a: d, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output e
  }
}
output m1
"#;

/// A `map` inside a `map`, for the depth-2 path test.
const NESTED_MAPS: &str = r#"
rows = range { start: 0, step: 1, count: 3 }
outer = map {
  xs: rows,
  input_type: Int,
  output_type: Int,
  body {
    inner = map {
      xs: $element,
      input_type: Int,
      output_type: Int,
      body {
        p = expr { a: $element, expression: "a", parameters: [{ name: "a", data_type: Int }] }
        output p
      }
    }
    output inner
  }
}
"#;

// ============================================================================
// By node, at the root scope
// ============================================================================

#[test]
fn a_one_node_addition_is_one_added_block() {
    let mut sd = designer();
    edit(&mut sd, "s1 = sphere { radius: 5 }\noutput s1\n");
    edit(&mut sd, "s2 = sphere { radius: 3 }\n");

    let diff = last_by_node(&sd);
    assert_eq!(hunks(&diff), vec![(DiffHunkKind::Added, "s2".to_string())]);
    assert!(
        diff.hunks[0]
            .lines
            .iter()
            .all(|line| line.tag == DiffLineTag::Add),
        "an added block is all additions"
    );
}

#[test]
fn a_property_change_is_a_line_diff_inside_the_block() {
    let mut sd = designer();
    edit(&mut sd, TWO_SPHERES);
    edit(&mut sd, "s2 = sphere { radius: 9 }\n");

    let diff = last_by_node(&sd);
    assert_eq!(
        hunks(&diff),
        vec![(DiffHunkKind::Changed, "s2".to_string())]
    );

    let lines = hunk_lines(&diff, "s2");
    assert!(
        lines
            .iter()
            .any(|(tag, text)| *tag == DiffLineTag::Remove && text.contains("radius: 3")),
        "the old value is a removal: {:?}",
        lines
    );
    assert!(
        lines
            .iter()
            .any(|(tag, text)| *tag == DiffLineTag::Add && text.contains("radius: 9")),
        "the new value is an addition: {:?}",
        lines
    );
}

#[test]
fn a_deleted_node_is_a_removed_block() {
    let mut sd = designer();
    edit(&mut sd, TWO_SPHERES);
    edit(&mut sd, "delete s2\n");

    let diff = last_by_node(&sd);
    assert_eq!(
        hunks(&diff),
        vec![(DiffHunkKind::Removed, "s2".to_string())]
    );
}

/// The test a naive line diff fails.
///
/// Wiring three spheres into a union that was created *before* them moves the
/// `u` statement from the top of the listing to the bottom — every remaining
/// statement changes line number. *By node* reports the one block whose text
/// actually changed; *Text* reports the statement twice, once as a removal at
/// the old position and once as an addition at the new one.
#[test]
fn a_reordering_edit_changes_one_block_by_node_but_moves_a_statement_in_text() {
    let mut sd = designer();
    edit(
        &mut sd,
        r#"
u = union { }
a = sphere { radius: 1 }
b = sphere { radius: 2 }
c = sphere { radius: 3 }
output u
"#,
    );
    let before = last_after_text(&sd);
    assert!(
        before.find("u = union").unwrap() < before.find("a = sphere").unwrap(),
        "the union starts above its future inputs:\n{}",
        before
    );

    edit(&mut sd, "u = union { shapes: [a, b, c] }\n");

    let after = last_after_text(&sd);
    assert!(
        after.find("u = union").unwrap() > after.find("c = sphere").unwrap(),
        "and the edit pushed it below them:\n{}",
        after
    );

    let by_node = last_by_node(&sd);
    assert_eq!(
        hunks(&by_node),
        vec![(DiffHunkKind::Changed, "u".to_string())]
    );
    assert!(
        by_node.unchanged_count >= 3,
        "a, b and c collapse as unchanged: {:?}",
        by_node
    );

    // The literal truth, which is what the *Text* view is kept for: the
    // statement appears on both sides of the diff because it moved.
    let text = last_text(&sd);
    let lines: Vec<(DiffLineTag, String)> = text
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| (line.tag, line.text.clone()))
        .collect();
    assert!(
        lines
            .iter()
            .any(|(tag, t)| *tag == DiffLineTag::Remove && t.trim_start().starts_with("u = union")),
        "text diff removes the statement at its old position: {:?}",
        lines
    );
    assert!(
        lines
            .iter()
            .any(|(tag, t)| *tag == DiffLineTag::Add && t.trim_start().starts_with("u = union")),
        "…and adds it at the new one: {:?}",
        lines
    );
}

#[test]
fn a_replace_of_an_unchanged_script_yields_an_empty_by_node_diff() {
    let mut sd = designer();
    edit(&mut sd, TWO_SPHERES);
    let snapshot = last_after_text(&sd);

    let outcome = sd.ai_text_edit(&snapshot, true);
    assert!(
        outcome.result.success,
        "the snapshot should replay, errors: {:?}",
        outcome.result.errors
    );

    let diff = last_by_node(&sd);
    assert!(
        diff.is_empty(),
        "a replace that rebuilds the same network changes nothing: {:?}",
        hunks(&diff)
    );
}

#[test]
fn the_header_never_appears_as_a_change() {
    let mut sd = designer();
    edit(&mut sd, TWO_SPHERES);
    edit(&mut sd, "s3 = sphere { radius: 1 }\n");

    let diff = last_by_node(&sd);
    assert!(
        !diff.hunks.iter().any(|hunk| hunk.node_path.is_empty()),
        "the `# Network:` header and the network `output` are untouched: {:?}",
        hunks(&diff)
    );
}

// ============================================================================
// The recursive split
// ============================================================================

#[test]
fn a_change_inside_a_body_is_one_changed_block_and_leaves_the_owner_alone() {
    let mut sd = designer();
    edit_applied(&mut sd, MAP_WITH_BODY);
    edit_applied(
        &mut sd,
        "m1/e = expr { a: d, expression: \"a + 2\", parameters: [{ name: \"a\", data_type: Int }] }\n",
    );

    let diff = last_by_node(&sd);
    assert_eq!(
        hunks(&diff),
        vec![(DiffHunkKind::Changed, "m1/e".to_string())],
        "only the body node changed — not its owner, not its sibling"
    );
}

/// The same false-positive mode as the root-scope test, one scope down.
///
/// A body scope runs its **own** topological sort, so inserting a node that the
/// body's output depends on reorders that body's statements. A splitter that
/// brace-matched but did not recurse would report the whole `m1` block as
/// changed; the recursive one reports one added key and nothing else.
#[test]
fn inserting_into_a_body_adds_one_block_and_changes_no_sibling() {
    let mut sd = designer();
    edit_applied(&mut sd, MAP_WITH_BODY);
    edit_applied(
        &mut sd,
        "m1/f = expr { a: $element, expression: \"a - 1\", parameters: [{ name: \"a\", data_type: Int }] }\n",
    );

    let diff = last_by_node(&sd);
    assert_eq!(
        hunks(&diff),
        vec![(DiffHunkKind::Added, "m1/f".to_string())],
        "the siblings' lines moved but their text did not"
    );
}

#[test]
fn the_footer_is_stripped_so_a_body_only_edit_touches_no_other_block() {
    let mut sd = designer();
    edit_applied(&mut sd, MAP_WITH_BODY);
    let before = last_after_text(&sd);
    edit_applied(
        &mut sd,
        "m1/f = expr { a: $element, expression: \"a - 1\", parameters: [{ name: \"a\", data_type: Int }] }\n",
    );
    let after = last_after_text(&sd);

    // The count in the footer really did change — the node it counts is in a
    // body, and `count_nodes` recurses.
    assert!(before.contains("# 5 nodes"), "before:\n{}", before);
    assert!(after.contains("# 6 nodes"), "after:\n{}", after);

    let diff = last_by_node(&sd);
    assert!(
        !diff.hunks.iter().any(|hunk| hunk.node_path.is_empty()),
        "the footer is dropped before diffing: {:?}",
        hunks(&diff)
    );
}

#[test]
fn a_bodys_output_statement_belongs_to_its_owners_block() {
    let mut sd = designer();
    edit_applied(&mut sd, MAP_WITH_BODY);
    edit_applied(&mut sd, "output m1/d\n");

    let diff = last_by_node(&sd);
    assert_eq!(
        hunks(&diff),
        vec![(DiffHunkKind::Changed, "m1".to_string())],
        "the body's `output` writes the owner's zone_output_arguments, so it is \
         the owner's line — not an orphan in the root scope's bucket"
    );

    let lines = hunk_lines(&diff, "m1");
    assert!(
        lines
            .iter()
            .any(|(tag, text)| *tag == DiffLineTag::Remove && text.trim() == "output e"),
        "{:?}",
        lines
    );
    assert!(
        lines
            .iter()
            .any(|(tag, text)| *tag == DiffLineTag::Add && text.trim() == "output d"),
        "{:?}",
        lines
    );
}

#[test]
fn a_nested_body_reaches_a_two_segment_path() {
    let mut sd = designer();
    edit_applied(&mut sd, NESTED_MAPS);

    let blocks = split_snapshot(&last_after_text(&sd));
    let keys: Vec<String> = blocks.blocks.keys().map(|path| path.join("/")).collect();
    assert!(
        keys.contains(&"outer/inner/p".to_string()),
        "two bodies deep: {:?}",
        keys
    );
    assert!(keys.contains(&"outer/inner".to_string()), "{:?}", keys);
    assert!(keys.contains(&"outer".to_string()), "{:?}", keys);

    // The owner keeps only its own properties and the `body { … }` framing.
    let outer = &blocks.blocks[&vec!["outer".to_string()]];
    assert!(outer.contains("body {"), "{}", outer);
    assert!(
        !outer.contains("inner = map"),
        "the child's text lives under its own key, not its owner's:\n{}",
        outer
    );
}

#[test]
fn a_replace_of_an_unchanged_body_bearing_script_yields_an_empty_by_node_diff() {
    let mut sd = designer();
    edit_applied(&mut sd, MAP_WITH_BODY);
    let snapshot = last_after_text(&sd);

    sd.ai_text_edit(&snapshot, true);

    let diff = last_by_node(&sd);
    assert!(
        diff.is_empty(),
        "identities survive a replace, so the body comes back identical: {:?}",
        hunks(&diff)
    );
    assert!(diff.unchanged_count >= 5, "{:?}", diff);
}

// ============================================================================
// The splitter itself
// ============================================================================

/// Bare names are unique *per scope only*, which is the whole reason the key is
/// a path.
#[test]
fn the_same_bare_name_in_two_bodies_gets_two_keys() {
    let text = r#"# Network: main

m1 = map {
  body {
    a = int { value: 1 }
    output a
  }
}
m2 = map {
  body {
    a = int { value: 2 }
    output a
  }
}

# 4 nodes
"#;

    let blocks = split_snapshot(text);
    let keys: Vec<String> = blocks.blocks.keys().map(|path| path.join("/")).collect();
    assert_eq!(keys, vec!["m1", "m1/a", "m2", "m2/a"]);
    assert!(blocks.blocks[&vec!["m1".into(), "a".into()]].contains("value: 1"));
    assert!(blocks.blocks[&vec!["m2".into(), "a".into()]].contains("value: 2"));
}

/// `/` inside backticks is part of the name, not a path separator.
#[test]
fn a_backtick_quoted_name_containing_a_slash_stays_one_segment() {
    let text = "# Network: main\n\n`a/b` = sphere { radius: 5 }\noutput `a/b`\n\n# 1 node\n";

    let blocks = split_snapshot(text);
    let keys: Vec<Vec<String>> = blocks.blocks.keys().cloned().collect();
    assert_eq!(keys, vec![vec!["a/b".to_string()]]);
    assert_eq!(keys[0].len(), 1, "one segment, not two");
}

/// A `description` can be triple-quoted and span lines, and a line inside it can
/// look exactly like a statement. It is not one.
#[test]
fn a_statement_shaped_line_inside_a_multiline_string_is_not_a_statement() {
    let text = "# Network: main\ndescription \"\"\"first\nfake = sphere { radius: 1 }\nlast\"\"\"\n\ns1 = sphere { radius: 5 }\n\n# 1 node\n";

    let blocks = split_snapshot(text);
    let keys: Vec<String> = blocks.blocks.keys().map(|path| path.join("/")).collect();
    assert_eq!(keys, vec!["s1"]);
    assert!(
        blocks.other.contains("fake = sphere"),
        "it belongs to the description:\n{}",
        blocks.other
    );
}

#[test]
fn the_footer_and_its_blank_line_are_stripped() {
    let with_five = "# Network: main\n\ns1 = sphere { radius: 5 }\n\n# 5 nodes\n";
    let with_six = "# Network: main\n\ns1 = sphere { radius: 5 }\n\n# 6 nodes\n";

    assert_eq!(split_snapshot(with_five), split_snapshot(with_six));
    assert!(diff_by_node(with_five, with_six).is_empty());
    assert!(diff_text(with_five, with_six).is_empty());
}
