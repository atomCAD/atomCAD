//! Diffing two AI text-format snapshots — Phase 2 of
//! `doc/design_ai_edit_history.md`.
//!
//! [`AiEditRecord`](crate::ai_edit_log::AiEditRecord) stores a *pair of
//! snapshots*, not a delta (D2), and the diff between them is computed here on
//! demand rather than at record time (D5): the record stays a pure capture, the
//! recording path stays cheap, and the two views below could be — and were —
//! added independently of it.
//!
//! # Two views, and why the default is not a line diff
//!
//! A plain line diff over a text-format snapshot has a known false-positive
//! mode. The output is topologically sorted, so inserting one upstream node
//! shifts every downstream statement and the line diff reports a large *move*
//! as a large *change*. [`diff_by_node`] avoids it by diffing the **set** of
//! per-node blocks — added / removed / changed, in path order — which is stable
//! against reordering. [`diff_text`] keeps the literal unified diff, because
//! sometimes the literal truth is what is wanted (and because the header,
//! footer and `output` statements live outside any node block).
//!
//! # The split is recursive, and the key is a path
//!
//! A zone-bearing node's statement spans lines and contains a whole nested
//! scope (`doc/design_hof_body_text_format.md` D14), so brace-matching alone
//! would put an entire body inside its owner's block — and the false-positive
//! mode would come straight back one scope down, because **each body scope runs
//! its own topological sort**. One node inserted into a 19-node body would
//! reorder that body's statements and report the whole `map` as changed.
//!
//! So [`split_snapshot`] descends into every `body { … }` block and emits a
//! block per body node too, keyed by its path — `m1` for a top-level node,
//! `m1/d` for a node in `m1`'s body, `outer/inner/p` two bodies deep. An
//! owner's own block then holds only its own properties and the literal
//! `body { … }` framing, not its children's text.
//!
//! Two things fall out. The keys are **exactly**
//! [`EditResult`](crate::text_format::EditResult)'s paths, so a row in the diff
//! and a name in the result are the same string. And they are unambiguous where
//! bare names are not: two bodies may each contain a node called `a`, and
//! `m1/a` / `m2/a` keep them apart.
//!
//! A body's own `output` statement belongs to its **owner** (D5 over there: it
//! writes the parent's `zone_output_arguments`), so it stays in the owner's
//! block rather than going to the "other" bucket, where only the root scope's
//! header, footer and `output` belong.
//!
//! # What is stripped
//!
//! The volatile part is the **footer**, not the header. `serialize` ends with
//! `\n# N nodes`, which churns on every edit that changes the node count — and
//! the count includes body nodes, so a body-only edit churns it too. That line
//! is dropped from both snapshots before diffing, in *both* views. The header
//! `# Network: <name>` changes only on a rename and is worth seeing, and the
//! `description "…"` / `summary "…"` lines that follow it are real,
//! AI-editable statements that must stay in the diff; all three land in the
//! "other" bucket.

use std::collections::BTreeMap;

use similar::{ChangeTag, TextDiff};

use crate::text_format::NamePath;

/// Whether a block was added, removed, or changed.
///
/// There is no `Unchanged` variant: an unchanged block produces no hunk at all
/// (it is counted in [`AiDiff::unchanged_count`]), which is what makes "a
/// `--replace` of an unchanged script yields an empty *By node* diff" the
/// literal statement it reads as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffHunkKind {
    Added,
    Removed,
    Changed,
}

/// One line's role in a hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineTag {
    Same,
    Add,
    Remove,
}

/// One rendered line. The trailing newline is stripped; the panel renders these
/// rather than re-parsing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub tag: DiffLineTag,
    pub text: String,
}

impl DiffLine {
    fn new(tag: DiffLineTag, text: &str) -> Self {
        Self {
            tag,
            text: text
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string(),
        }
    }
}

/// One block's worth of difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub kind: DiffHunkKind,
    /// In *By node* mode: the scoped node path (`m1/d`), the same key
    /// `EditResult` uses, so the *Diff* and *Result* tabs name a node
    /// identically. Empty for the root scope's "other" bucket (header,
    /// `description`, `summary`, `output`).
    ///
    /// In *Text* mode: the unified diff's `@@ … @@` range header.
    pub node_path: String,
    pub lines: Vec<DiffLine>,
}

/// The computed difference between one record's two snapshots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiDiff {
    /// Which view produced this — *By node* when true, *Text* when false.
    pub by_node: bool,
    /// Only the blocks that differ, in path order. Empty means the two
    /// snapshots are equivalent.
    pub hunks: Vec<DiffHunk>,
    /// How many blocks were identical and so were collapsed away. Always `0`
    /// in *Text* mode, which has no notion of a block.
    pub unchanged_count: usize,
}

impl AiDiff {
    /// Whether anything at all differs.
    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }
}

/// The blocks one snapshot splits into.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotBlocks {
    /// `path → block text`, ordered by path. A body's entries sort directly
    /// under their owner, because `["m1"] < ["m1", "d"] < ["m10"]`.
    pub blocks: BTreeMap<NamePath, String>,
    /// The root scope's non-statement lines: the `# Network:` header, the
    /// `description` / `summary` statements, and the network's `output`. The
    /// footer is not here — it is stripped before splitting.
    pub other: String,
}

// ===========================================================================
// The split
// ===========================================================================

/// Split a snapshot into one block per node, keyed by path, plus the root
/// scope's leftovers.
///
/// The recursion is what keeps a body edit local: changing one node inside a
/// body changes exactly one key, even though that body's own topological sort
/// reordered its siblings' lines.
pub fn split_snapshot(text: &str) -> SnapshotBlocks {
    let lines = snapshot_lines(text);
    let mut blocks = BTreeMap::new();
    let mut other = Vec::new();
    split_scope(&lines, &[], &mut blocks, &mut other);
    SnapshotBlocks {
        blocks,
        other: other.join("\n"),
    }
}

/// The snapshot's lines with the volatile `# N nodes` footer, and the blank
/// line the serializer puts before it, removed.
fn snapshot_lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.last().is_some_and(|line| is_footer(line)) {
        lines.pop();
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

/// `# 12 nodes` / `# 1 node` — the serializer's footer, and the one line that
/// churns on every edit that changes the node count.
fn is_footer(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("# ") else {
        return false;
    };
    let Some((count, word)) = rest.split_once(' ') else {
        return false;
    };
    (word == "node" || word == "nodes")
        && !count.is_empty()
        && count.chars().all(|c| c.is_ascii_digit())
}

/// Emit a block for every statement in one scope, recursing into bodies.
///
/// Lines that are not part of a statement — the header, `description`,
/// `summary`, `output`, blank lines, a serializer `# Error:` comment — go to
/// `other`. At the root that is the "other" bucket; inside a body it is the
/// body's `output` statement, which the caller folds back into the *owner's*
/// block because that is whose `zone_output_arguments` it writes.
fn split_scope(
    lines: &[&str],
    prefix: &[String],
    blocks: &mut BTreeMap<NamePath, String>,
    other: &mut Vec<String>,
) {
    let mut state = ScanState::default();
    let mut index = 0;
    while index < lines.len() {
        // The mode guard is what keeps a `foo = bar` line *inside* a
        // triple-quoted description from being mistaken for a statement.
        if state.mode == ScanMode::Code
            && let Some(name) = parse_statement_name(lines[index])
        {
            let end = statement_extent(lines, index);
            let mut path = prefix.to_vec();
            path.push(name);
            let own = split_statement(&lines[index..end], &path, blocks);
            blocks.insert(path, own);
            index = end;
            // A statement is brace-balanced by construction, so the scanner
            // starts the next line clean.
            state = ScanState::default();
            continue;
        }
        state = scan_line(state, lines[index]);
        other.push(lines[index].to_string());
        index += 1;
    }
}

/// Build one node's own block from its statement lines, emitting its body's
/// nodes as separate blocks along the way.
///
/// The owner keeps the literal `body { … }` framing and the body's `output`
/// line; the body's node statements are removed from its text and become their
/// own keys.
fn split_statement(
    statement: &[&str],
    path: &NamePath,
    blocks: &mut BTreeMap<NamePath, String>,
) -> String {
    let Some((open, close)) = find_body_block(statement) else {
        return statement.join("\n");
    };

    let mut body_other = Vec::new();
    split_scope(&statement[open + 1..close], path, blocks, &mut body_other);

    let mut own: Vec<String> = statement[..=open].iter().map(|s| s.to_string()).collect();
    own.extend(body_other);
    own.extend(statement[close..].iter().map(|s| s.to_string()));
    own.join("\n")
}

/// One past the last line of the statement starting at `start`.
///
/// Brace matching, not "one statement per line": a zone-bearing node's
/// statement spans lines, and so does any node carrying a triple-quoted
/// multi-line string. A node with no properties at all carries no braces and
/// ends on its own line, which falls out of the same test.
fn statement_extent(lines: &[&str], start: usize) -> usize {
    let mut state = ScanState::default();
    for (offset, line) in lines[start..].iter().enumerate() {
        state = scan_line(state, line);
        if state.depth <= 0 {
            return start + offset + 1;
        }
    }
    lines.len()
}

/// The `(body {, matching })` line indices within one statement, if it has a
/// body.
///
/// The first `body {` line is always the right one: the block is emitted after
/// every property, and a *nested* body can only appear inside it.
fn find_body_block(statement: &[&str]) -> Option<(usize, usize)> {
    let mut state = ScanState::default();
    for (index, line) in statement.iter().enumerate() {
        if state.mode == ScanMode::Code && line.trim() == "body {" {
            return Some((index, statement_extent(statement, index) - 1));
        }
        state = scan_line(state, line);
    }
    None
}

/// The name a statement assigns to, or `None` when the line is not a statement.
///
/// Identifiers are read with the format's own rules: a backtick-quoted name is
/// one segment, and a `/` inside backticks is not a path separator.
fn parse_statement_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let (name, rest) = match trimmed.strip_prefix('`') {
        Some(after) => {
            let end = after.find('`')?;
            if end == 0 {
                return None;
            }
            (after[..end].to_string(), &after[end + 1..])
        }
        None => {
            let first = trimmed.chars().next()?;
            if !(first.is_alphabetic() || first == '_') {
                return None;
            }
            let end = trimmed
                .char_indices()
                .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
                .map_or(trimmed.len(), |(index, _)| index);
            (trimmed[..end].to_string(), &trimmed[end..])
        }
    };

    let mut rest = rest.trim_start().chars();
    if rest.next()? != '=' {
        return None;
    }
    // `==` is not part of the format, but a property expression could carry one
    // and a future statement form should not be silently swallowed.
    if rest.next() == Some('=') {
        return None;
    }
    Some(name)
}

// ---------------------------------------------------------------------------
// The line scanner
// ---------------------------------------------------------------------------

/// Where in the format's lexical structure the scanner currently is.
///
/// Braces inside a string are not braces, and neither is a `/` inside a
/// backtick-quoted name — so brace matching cannot be done by counting
/// characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ScanMode {
    #[default]
    Code,
    /// Inside a `"…"` string, which never spans a line in this format.
    Str,
    /// Inside a `"""…"""` string, which does.
    TripleStr,
    /// Inside a `` `…` `` quoted identifier.
    Backtick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ScanState {
    depth: i32,
    mode: ScanMode,
}

/// Advance the scanner across one line.
fn scan_line(mut state: ScanState, line: &str) -> ScanState {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        match state.mode {
            ScanMode::Code => match chars[index] {
                // A `#` starts a comment that runs to end of line.
                '#' => break,
                '{' => state.depth += 1,
                '}' => state.depth -= 1,
                '`' => state.mode = ScanMode::Backtick,
                '"' => {
                    if chars[index + 1..].starts_with(&['"', '"']) {
                        state.mode = ScanMode::TripleStr;
                        index += 2;
                    } else {
                        state.mode = ScanMode::Str;
                    }
                }
                _ => {}
            },
            ScanMode::Str => match chars[index] {
                '\\' => index += 1,
                '"' => state.mode = ScanMode::Code,
                _ => {}
            },
            ScanMode::TripleStr => match chars[index] {
                '\\' => index += 1,
                '"' if chars[index + 1..].starts_with(&['"', '"']) => {
                    state.mode = ScanMode::Code;
                    index += 2;
                }
                _ => {}
            },
            ScanMode::Backtick => {
                if chars[index] == '`' {
                    state.mode = ScanMode::Code;
                }
            }
        }
        index += 1;
    }
    // A single-quoted string cannot span lines (the serializer escapes newlines
    // and reaches for the triple-quoted form otherwise), so an unterminated one
    // is a malformed line rather than a continuation. Resetting keeps one bad
    // line from swallowing the rest of the scope.
    if state.mode == ScanMode::Str {
        state.mode = ScanMode::Code;
    }
    state
}

// ===========================================================================
// The two views
// ===========================================================================

/// The *By node* diff (D4): the difference between the two snapshots' **sets**
/// of per-node blocks, in path order.
///
/// Stable against the reordering a topological sort produces, which is the
/// whole reason it is the default view.
pub fn diff_by_node(before_text: &str, after_text: &str) -> AiDiff {
    let before = split_snapshot(before_text);
    let after = split_snapshot(after_text);

    let mut hunks = Vec::new();
    let mut unchanged_count = 0;

    // The root scope's leftovers first: the header, `description`, `summary`
    // and the network's own `output`. An `output` re-point is filed here, and
    // it is the only trace such an edit leaves in the text.
    if before.other != after.other {
        hunks.push(DiffHunk {
            kind: DiffHunkKind::Changed,
            node_path: String::new(),
            lines: line_diff(&before.other, &after.other),
        });
    } else if !before.other.is_empty() {
        unchanged_count += 1;
    }

    let mut paths: Vec<&NamePath> = before.blocks.keys().chain(after.blocks.keys()).collect();
    paths.sort();
    paths.dedup();

    for path in paths {
        let node_path = path.join("/");
        match (before.blocks.get(path), after.blocks.get(path)) {
            (None, Some(text)) => hunks.push(DiffHunk {
                kind: DiffHunkKind::Added,
                node_path,
                lines: tagged_lines(text, DiffLineTag::Add),
            }),
            (Some(text), None) => hunks.push(DiffHunk {
                kind: DiffHunkKind::Removed,
                node_path,
                lines: tagged_lines(text, DiffLineTag::Remove),
            }),
            (Some(old), Some(new)) if old != new => hunks.push(DiffHunk {
                kind: DiffHunkKind::Changed,
                node_path,
                lines: line_diff(old, new),
            }),
            // Identical, so collapsed — this is the ordinary case, and the one
            // that makes a well-behaved edit read as one row rather than a
            // wall of text.
            (Some(_), Some(_)) => unchanged_count += 1,
            (None, None) => unreachable!("path came from one of the two maps"),
        }
    }

    AiDiff {
        by_node: true,
        hunks,
        unchanged_count,
    }
}

/// The *Text* diff: a literal unified line diff over the two snapshots with
/// three lines of context, unchanged runs collapsed. One hunk per `@@` group.
pub fn diff_text(before_text: &str, after_text: &str) -> AiDiff {
    let before = snapshot_lines(before_text).join("\n");
    let after = snapshot_lines(after_text).join("\n");
    let diff = TextDiff::from_lines(&before, &after);

    let mut hunks = Vec::new();
    for group in diff.grouped_ops(3) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_start = first.old_range().start;
        let old_end = last.old_range().end;
        let new_start = first.new_range().start;
        let new_end = last.new_range().end;

        let mut lines = Vec::new();
        for op in &group {
            for change in diff.iter_changes(op) {
                let tag = match change.tag() {
                    ChangeTag::Equal => DiffLineTag::Same,
                    ChangeTag::Delete => DiffLineTag::Remove,
                    ChangeTag::Insert => DiffLineTag::Add,
                };
                lines.push(DiffLine::new(tag, change.value()));
            }
        }

        hunks.push(DiffHunk {
            kind: DiffHunkKind::Changed,
            node_path: format!(
                "@@ -{},{} +{},{} @@",
                old_start + 1,
                old_end - old_start,
                new_start + 1,
                new_end - new_start
            ),
            lines,
        });
    }

    AiDiff {
        by_node: false,
        hunks,
        unchanged_count: 0,
    }
}

/// Every line of `text` under one tag — a whole block added or removed.
fn tagged_lines(text: &str, tag: DiffLineTag) -> Vec<DiffLine> {
    text.lines().map(|line| DiffLine::new(tag, line)).collect()
}

/// A line-level diff *within* one block. Blocks are small, so nothing is
/// grouped or elided here.
fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
    TextDiff::from_lines(before, after)
        .iter_all_changes()
        .map(|change| {
            let tag = match change.tag() {
                ChangeTag::Equal => DiffLineTag::Same,
                ChangeTag::Delete => DiffLineTag::Remove,
                ChangeTag::Insert => DiffLineTag::Add,
            };
            DiffLine::new(tag, change.value())
        })
        .collect()
}
