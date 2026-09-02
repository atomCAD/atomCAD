//! Flutter-facing surface of the AI edit log — Phase 2 of
//! `doc/design_ai_edit_history.md`.
//!
//! The authoritative types live in `atomcad_structure_designer::ai_edit_log`
//! and `…::ai_edit_diff`; these are their Dart-facing twins, per the FRB rule
//! that a `pub use` re-export from a lower crate is invisible to codegen (see
//! `rust/AGENTS.md`). Adding this module to `flutter_rust_bridge.yaml`'s
//! `rust_input` is part of the same change — a missing entry is not a build
//! error, it silently produces an opaque handle on the Dart side.
//!
//! **Summaries and payloads travel separately** (D12). [`ai_history_list`]
//! returns lightweight rows with no snapshot text; code, results and diffs are
//! fetched per selected entry by [`ai_history_detail`] and
//! [`ai_history_diff`]. Shipping every snapshot on every refresh would put
//! megabytes through FFI on a path `doc/design_eval_profiling.md` D8a exists to
//! keep cheap.
//!
//! Even the list is not free — it allocates up to 200 structs of several heap
//! `String`s each — so `refreshFromKernel` gates it on [`ai_history_version`],
//! a `u64` compare, and re-fetches only when that changed *and* the panel is
//! visible.
//!
//! **Diffs are computed here, on demand** (D5), never stored: the record stays
//! a pure capture and the recording path stays cheap.
//!
//! **The timeline has two kinds of row** since Phase 5. Edits come from
//! [`ai_history_list`]; every other CLI request — `query`, `screenshot`,
//! `networks/*`, `load`, `save` — comes from [`ai_history_activity_list`], and
//! the two are merged by `seq`, which both rings draw from. They are separate
//! calls rather than one union type because an activity row carries none of an
//! edit row's fifteen fields and has no detail pane behind it: it is a marker
//! saying what the AI was doing between edits.
//!
//! The recording direction is the other half. [`ai_history_record_activity`] is
//! called by the Dart HTTP server from its single `_handleRequest` hook — the
//! transport is the only layer that knows a request happened, and `/edit`
//! records itself far below, inside `ai_text_edit`.

use crate::api::api_common::{with_cad_instance_or, with_mut_cad_instance_or};
use crate::api::common_api_types::APIVec2;
use atomcad_structure_designer::ai_edit_diff::{
    AiDiff, DiffHunk, DiffHunkKind, DiffLine, DiffLineTag, diff_by_node, diff_text,
};
use atomcad_structure_designer::ai_edit_export::{export_json, export_markdown};
use atomcad_structure_designer::ai_edit_log::{
    AiActivityRecord, AiEditRecord, DeltaCounts, LayoutOutcome, LayoutPath, MovedNode,
};

// ===========================================================================
// Dart-facing twins
// ===========================================================================

/// Flutter-facing mirror of [`LayoutPath`]: which layout algorithm ran over the
/// **root scope** after the edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APILayoutPath {
    None,
    FullReflow,
    Incremental,
}

impl From<LayoutPath> for APILayoutPath {
    fn from(path: LayoutPath) -> Self {
        match path {
            LayoutPath::None => APILayoutPath::None,
            LayoutPath::FullReflow => APILayoutPath::FullReflow,
            LayoutPath::Incremental => APILayoutPath::Incremental,
        }
    }
}

/// Flutter-facing mirror of [`MovedNode`].
// `APIVec2` derives nothing, so neither can the structs that embed it. Nothing
// here needs `Debug` or `Clone`: each value is built fresh at the FFI boundary.
pub struct APIMovedNode {
    /// The scoped node path, rendered `m1/d`. Never a bare name: body names are
    /// unique per scope only, so `m1/d` and `m2/d` are different nodes (D9).
    pub path: String,
    /// How many scopes deep the node sits — `0` for the root scope. The panel's
    /// Layout tab groups by this, because a move inside a body comes from
    /// creation-time placement rather than from the layout pass.
    pub depth: u32,
    pub before: APIVec2,
    pub after: APIVec2,
    pub displacement: f64,
}

impl From<&MovedNode> for APIMovedNode {
    fn from(moved: &MovedNode) -> Self {
        Self {
            path: moved.path_string(),
            depth: moved.path.len().saturating_sub(1) as u32,
            before: APIVec2 {
                x: moved.before.x,
                y: moved.before.y,
            },
            after: APIVec2 {
                x: moved.after.x,
                y: moved.after.y,
            },
            displacement: moved.displacement(),
        }
    }
}

/// Flutter-facing mirror of [`DeltaCounts`]. `None` on the record until
/// `doc/design_incremental_layout.md` Phase 1 lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct APIDeltaCounts {
    pub nodes_added: u32,
    pub nodes_modified: u32,
    pub nodes_removed: u32,
    pub wires_added: u32,
    pub wires_removed: u32,
}

impl From<DeltaCounts> for APIDeltaCounts {
    fn from(counts: DeltaCounts) -> Self {
        Self {
            nodes_added: counts.nodes_added as u32,
            nodes_modified: counts.nodes_modified as u32,
            nodes_removed: counts.nodes_removed as u32,
            wires_added: counts.wires_added as u32,
            wires_removed: counts.wires_removed as u32,
        }
    }
}

/// Flutter-facing mirror of [`LayoutOutcome`]: what layout did to the drawing.
///
/// `path` describes what the layout pass did to the **root scope** only, so
/// `APILayoutPath::None` with a non-empty `moved` is correct and ordinary — a
/// body node is never reflowed but is still re-placed at creation time. The
/// panel must say which, rather than reading `None` as "nothing was disturbed"
/// (D9).
pub struct APILayoutOutcome {
    pub path: APILayoutPath,
    /// Every node in the network, bodies included.
    pub node_count: u32,
    pub moved: Vec<APIMovedNode>,
    pub max_displacement: f64,
    pub delta: Option<APIDeltaCounts>,
}

impl From<&LayoutOutcome> for APILayoutOutcome {
    fn from(outcome: &LayoutOutcome) -> Self {
        Self {
            path: outcome.path.into(),
            node_count: outcome.node_count as u32,
            moved: outcome.moved.iter().map(APIMovedNode::from).collect(),
            max_displacement: outcome.max_displacement,
            delta: outcome.delta.map(APIDeltaCounts::from),
        }
    }
}

/// One list row: what the panel's master pane needs and nothing more (D12).
#[derive(Debug, Clone)]
pub struct APIAiEditSummary {
    pub seq: u64,
    pub timestamp_ms: i64,
    /// May be empty on the "no active network" rejection path.
    pub network_name: String,
    /// `--replace` was used. The row badges this unmissably: it is the
    /// difference between a merge and a whole-network rebuild.
    pub replace: bool,
    /// The editor's own verdict — the statements parsed and landed. This is the
    /// row's primary glyph, and the one that answers "did this edit work".
    pub applied: bool,
    /// The network validates afterwards. A *whole-network* verdict, so
    /// `applied && !success` is ordinary rather than exceptional: creating an
    /// HOF node before filling its body produces it every time (D8).
    pub success: bool,
    pub created_count: u32,
    pub updated_count: u32,
    pub deleted_count: u32,
    pub error_count: u32,
    pub warning_count: u32,
    /// The network changed between this edit and the previous one for the same
    /// network (D7).
    pub diverged: bool,
    /// …and the gap is explained by an undo or redo of an AI edit, so the
    /// marker reads "edit #N undone" rather than the general wording.
    pub diverged_by_undo: bool,
    /// Both snapshots are faithful. False means the serializer aborted on a
    /// wire cycle, so the diff is untrustworthy and the text is not valid
    /// `edit --replace` input (D2).
    pub snapshots_complete: bool,
    pub moved_count: u32,
}

impl From<&AiEditRecord> for APIAiEditSummary {
    fn from(record: &AiEditRecord) -> Self {
        Self {
            seq: record.seq,
            timestamp_ms: record.timestamp_ms,
            network_name: record.network_name.clone(),
            replace: record.replace,
            applied: record.applied,
            success: record.success,
            created_count: record.nodes_created.len() as u32,
            updated_count: record.nodes_updated.len() as u32,
            deleted_count: record.nodes_deleted.len() as u32,
            error_count: record.errors.len() as u32,
            warning_count: record.warnings.len() as u32,
            diverged: record.diverged,
            diverged_by_undo: record.diverged_by_undo,
            snapshots_complete: record.before_complete && record.after_complete,
            moved_count: record.layout.moved.len() as u32,
        }
    }
}

/// One entry in full: the submitted script, the whole `EditResult`, both
/// snapshots and the layout outcome. Fetched for the selected row only.
pub struct APIAiEditDetail {
    pub seq: u64,
    pub timestamp_ms: i64,
    pub network_name: String,
    pub replace: bool,
    /// Exactly what the AI submitted.
    pub code: String,
    pub applied: bool,
    pub success: bool,
    /// Node **paths** (`m1/a`), the same key the diff uses.
    pub nodes_created: Vec<String>,
    pub nodes_updated: Vec<String>,
    pub nodes_deleted: Vec<String>,
    pub connections_made: Vec<String>,
    /// The three `EditResult` fields it is easy to drop on the floor.
    /// `output_set` is how "the AI re-pointed the network's return node" is
    /// visible at all (D8).
    pub description_set: Option<String>,
    pub summary_set: Option<String>,
    pub output_set: Option<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// AI text-format snapshots (D2), **not** `.cnnd`. Empty on the rejection
    /// paths, which never reached the network.
    pub before_text: String,
    pub after_text: String,
    pub before_complete: bool,
    pub after_complete: bool,
    pub diverged: bool,
    pub diverged_by_undo: bool,
    /// From the `X-Client-Label` header the CLI sends (Phase 5); empty when the
    /// caller did not identify itself. This is what the manual session label
    /// (D13) exists to substitute for, so the detail pane shows it when it is
    /// there and falls back to the label otherwise.
    pub client_label: String,
    pub layout: APILayoutOutcome,
}

impl From<&AiEditRecord> for APIAiEditDetail {
    fn from(record: &AiEditRecord) -> Self {
        Self {
            seq: record.seq,
            timestamp_ms: record.timestamp_ms,
            network_name: record.network_name.clone(),
            replace: record.replace,
            code: record.code.clone(),
            applied: record.applied,
            success: record.success,
            nodes_created: record.nodes_created.clone(),
            nodes_updated: record.nodes_updated.clone(),
            nodes_deleted: record.nodes_deleted.clone(),
            connections_made: record.connections_made.clone(),
            description_set: record.description_set.clone(),
            summary_set: record.summary_set.clone(),
            output_set: record.output_set.clone(),
            errors: record.errors.clone(),
            warnings: record.warnings.clone(),
            before_text: record.before_text.clone(),
            after_text: record.after_text.clone(),
            before_complete: record.before_complete,
            after_complete: record.after_complete,
            diverged: record.diverged,
            diverged_by_undo: record.diverged_by_undo,
            client_label: record.client_label.clone(),
            layout: APILayoutOutcome::from(&record.layout),
        }
    }
}

/// One non-edit CLI request as a timeline row (Phase 5).
///
/// Everything a row renders and nothing more — there is no detail pane behind
/// an activity entry, so `requestLine` and `detail` are the whole record as far
/// as Dart is concerned.
#[derive(Debug, Clone)]
pub struct APIAiActivitySummary {
    /// Position on the timeline **shared** with the edit rows, which is how the
    /// panel merges the two lists into one ordered list.
    pub seq: u64,
    pub timestamp_ms: i64,
    /// `GET /query?verbose=true`, pre-rendered: Dart displays it, never parses
    /// it.
    pub request_line: String,
    pub method: String,
    pub path: String,
    /// A short note the handler attached — the network a rename targeted, the
    /// file a load opened. Empty when the query string said everything.
    pub detail: String,
    pub status: u32,
    /// `2xx`/`3xx`. Pre-computed so the row's tint has one definition.
    pub ok: bool,
    pub duration_ms: u32,
    pub client_label: String,
}

impl From<&AiActivityRecord> for APIAiActivitySummary {
    fn from(record: &AiActivityRecord) -> Self {
        Self {
            seq: record.seq,
            timestamp_ms: record.timestamp_ms,
            request_line: record.request_line(),
            method: record.method.clone(),
            path: record.path.clone(),
            detail: record.detail.clone(),
            status: record.status as u32,
            ok: record.ok(),
            duration_ms: record.duration_ms,
            client_label: record.client_label.clone(),
        }
    }
}

/// Flutter-facing mirror of [`DiffHunkKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIDiffHunkKind {
    Added,
    Removed,
    Changed,
}

impl From<DiffHunkKind> for APIDiffHunkKind {
    fn from(kind: DiffHunkKind) -> Self {
        match kind {
            DiffHunkKind::Added => APIDiffHunkKind::Added,
            DiffHunkKind::Removed => APIDiffHunkKind::Removed,
            DiffHunkKind::Changed => APIDiffHunkKind::Changed,
        }
    }
}

/// Flutter-facing mirror of [`DiffLineTag`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIDiffLineTag {
    Same,
    Add,
    Remove,
}

impl From<DiffLineTag> for APIDiffLineTag {
    fn from(tag: DiffLineTag) -> Self {
        match tag {
            DiffLineTag::Same => APIDiffLineTag::Same,
            DiffLineTag::Add => APIDiffLineTag::Add,
            DiffLineTag::Remove => APIDiffLineTag::Remove,
        }
    }
}

/// One rendered diff line. Rendered, not re-parsed, by Dart.
#[derive(Debug, Clone)]
pub struct APIDiffLine {
    pub tag: APIDiffLineTag,
    pub text: String,
}

impl From<&DiffLine> for APIDiffLine {
    fn from(line: &DiffLine) -> Self {
        Self {
            tag: line.tag.into(),
            text: line.text.clone(),
        }
    }
}

/// One block's worth of difference.
#[derive(Debug, Clone)]
pub struct APIDiffHunk {
    pub kind: APIDiffHunkKind,
    /// *By node*: the scoped path (`m1/d`), the same key `EditResult` uses, so
    /// the *Diff* and *Result* tabs name a node identically. Empty for the root
    /// scope's header / `description` / `summary` / `output` bucket.
    /// *Text*: the unified diff's `@@ … @@` range header.
    pub node_path: String,
    pub lines: Vec<APIDiffLine>,
}

impl From<&DiffHunk> for APIDiffHunk {
    fn from(hunk: &DiffHunk) -> Self {
        Self {
            kind: hunk.kind.into(),
            node_path: hunk.node_path.clone(),
            lines: hunk.lines.iter().map(APIDiffLine::from).collect(),
        }
    }
}

/// One entry's computed diff.
#[derive(Debug, Clone)]
pub struct APIAiDiff {
    pub by_node: bool,
    /// Only the blocks that differ. Empty means the two snapshots are
    /// equivalent — which is exactly what a `--replace` of an unchanged script
    /// produces, because identities and positions survive one.
    pub hunks: Vec<APIDiffHunk>,
    /// How many blocks were identical and so were collapsed away.
    pub unchanged_count: u32,
    /// Both snapshots are faithful. When false the panel banners the entry and
    /// greys the diff: a truncated `after_text` would otherwise fill the
    /// *removed* column with nodes that still exist (D2).
    pub snapshots_complete: bool,
}

impl APIAiDiff {
    fn build(diff: &AiDiff, snapshots_complete: bool) -> Self {
        Self {
            by_node: diff.by_node,
            hunks: diff.hunks.iter().map(APIDiffHunk::from).collect(),
            unchanged_count: diff.unchanged_count as u32,
            snapshots_complete,
        }
    }
}

// ===========================================================================
// FFI functions
// ===========================================================================

/// Bumped on every push, on `clear`, and on a session-label change.
///
/// The refresh gate of D12: without it `refreshFromKernel` has no cheap way to
/// know the list is unchanged, and would marshal every summary on every tick.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_version() -> u64 {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.ai_edit_log.version(),
            0,
        )
    }
}

/// Every retained entry as a lightweight row, oldest first.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_list() -> Vec<APIAiEditSummary> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .ai_edit_log
                    .records()
                    .map(APIAiEditSummary::from)
                    .collect()
            },
            Vec::new(),
        )
    }
}

/// Every retained non-edit CLI request as a timeline row, oldest first
/// (Phase 5).
///
/// Separate from [`ai_history_list`] and merged in Dart by `seq`: the two rings
/// have different caps and an activity row shares almost no fields with an edit
/// row, so a union type would be mostly-null either way.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_activity_list() -> Vec<APIAiActivitySummary> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .ai_edit_log
                    .activity()
                    .map(APIAiActivitySummary::from)
                    .collect()
            },
            Vec::new(),
        )
    }
}

/// Record one non-edit CLI request (Phase 5).
///
/// Called from the Dart HTTP server's single `_handleRequest` hook, which is
/// the only place that knows a request happened at all — `/edit` is *not*
/// routed here, because it records itself with full fidelity down in
/// `ai_text_edit`, and `/health` is skipped as pure polling noise.
///
/// Bumps the log version, so the panel picks the entry up on the next refresh
/// exactly as it picks up an edit.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_record_activity(
    method: String,
    path: String,
    query: String,
    detail: String,
    status: u32,
    duration_ms: u32,
) {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .ai_edit_log
                    .push_activity(AiActivityRecord::new(
                        method,
                        path,
                        query,
                        detail,
                        status as u16,
                        duration_ms,
                    ))
            },
            (),
        )
    }
}

/// Announce which client is calling, from a request's `X-Client-Label` header
/// (Phase 5, open question 5).
///
/// Set once per request, *before* the handler runs, so that the edit record
/// `ai_text_edit` pushes from deep inside the domain crate carries it too. It
/// deliberately does **not** bump the log version: a header is invisible until
/// a record carries it, and bumping here would make the panel's refresh gate
/// fire on every request.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_set_client_label(label: String) {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .ai_edit_log
                    .set_client_label(label)
            },
            (),
        )
    }
}

/// Every distinct client label seen this session, oldest first.
///
/// The panel offers the newest as the session label's placeholder: when the CLI
/// identifies itself there is nothing left for the maintainer to type.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_client_labels() -> Vec<String> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.ai_edit_log.client_labels(),
            Vec::new(),
        )
    }
}

/// One entry in full, or `None` when it has been evicted by the ring's caps.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_detail(seq: u64) -> Option<APIAiEditDetail> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .ai_edit_log
                    .get(seq)
                    .map(APIAiEditDetail::from)
            },
            None,
        )
    }
}

/// One entry's diff, computed on demand (D5).
///
/// `by_node` selects the default *By node* view — the set difference over
/// per-node blocks, stable against the reordering a topological sort produces
/// — over the literal unified *Text* diff.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_diff(seq: u64, by_node: bool) -> Option<APIAiDiff> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let record = cad_instance.structure_designer.ai_edit_log.get(seq)?;
                let diff = if by_node {
                    diff_by_node(&record.before_text, &record.after_text)
                } else {
                    diff_text(&record.before_text, &record.after_text)
                };
                Some(APIAiDiff::build(
                    &diff,
                    record.before_complete && record.after_complete,
                ))
            },
            None,
        )
    }
}

/// The whole session as JSON — the canonical export form, machine-comparable
/// across sessions and models (D10). The caller writes it through the standard
/// file-save path so it picks up the last-directory behaviour.
///
/// The formatting itself lives in
/// [`atomcad_structure_designer::ai_edit_export`]: the log is domain state, and
/// a formatter reachable only through the global `CAD_INSTANCE` could not be
/// tested.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_export_json() -> String {
    unsafe {
        with_cad_instance_or(
            |cad_instance| export_json(&cad_instance.structure_designer.ai_edit_log),
            String::new(),
        )
    }
}

/// The whole session as Markdown — a convenience for pasting into a
/// skill-refinement conversation, where JSON would be unreadable. Omits the
/// snapshots for the same reason.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_export_markdown() -> String {
    unsafe {
        with_cad_instance_or(
            |cad_instance| export_markdown(&cad_instance.structure_designer.ai_edit_log),
            String::new(),
        )
    }
}

/// Drop every entry. `seq` keeps counting, so an exported log stays
/// unambiguous.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_clear() {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.ai_edit_log.clear(),
            (),
        )
    }
}

/// Set the free-text session label (D13) — "Opus 5 / skill v3". The application
/// cannot know which model is driving the CLI, so it is asked; the label is
/// stamped into exports.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_set_session_label(label: String) {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .ai_edit_log
                    .set_session_label(label)
            },
            (),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn ai_history_get_session_label() -> String {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .ai_edit_log
                    .session_label()
                    .to_string()
            },
            String::new(),
        )
    }
}
