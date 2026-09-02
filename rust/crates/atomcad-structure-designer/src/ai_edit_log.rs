//! Session log of AI edits — Phase 1 of `doc/design_ai_edit_history.md`.
//!
//! The AI edit surface (`ai_edit_network`, reached as `atomcad-cli edit
//! [--replace]`) used to leave no record of itself. It leaves an *undo step*
//! since Phase 5 of `doc/design_hof_body_text_format.md`, but an undo step is a
//! way back, not a way to look: after a session of AI editing the maintainer
//! can see the current network and nothing else — not what was submitted, not
//! what the network looked like before, not whether `--replace` was used, not
//! what came back, and not which nodes the layout pass moved.
//!
//! This module is the recording half. Four things about its shape are load
//! bearing:
//!
//! - **A pair of text-format snapshots, not a delta** (D2). `--replace` mints a
//!   fresh node id for every node, so an id-based delta degenerates to
//!   "everything was deleted and everything was created"; the *text* is
//!   comparable in both modes. The snapshots are
//!   [`serialize_network`](crate::text_format::serialize_network) output — the
//!   human-readable AI dialect, **not** `.cnnd`.
//! - **Two verdicts, not one** (D8). `applied` is the editor's own — the
//!   statements parsed and landed — while `success` folds in a *whole-network*
//!   validation verdict. A network that was already blocking-invalid makes
//!   every later edit report `success: false` however clean it was, so a single
//!   glyph would answer "is the network valid now" instead of "did this edit
//!   work".
//! - **Every key is a path** (D9). Body names are unique *per scope*, so `m1/d`
//!   and `m2/d` are different nodes and a bare-name key silently merges them.
//!   The position map comes from
//!   [`snapshot_node_positions`](crate::text_format::snapshot_node_positions),
//!   the same walk the editor uses for its identity match.
//! - **Session-only** (D10). Never written to `.cnnd`, never undoable — exempt
//!   from `feedback_persisted_mutations_must_be_undoable` for the same reason
//!   `print_log` is. Undoing an AI edit leaves its entry standing (D6): this is
//!   a log of what happened, not a view of document state.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use glam::DVec2;

use crate::node_network::NodeNetwork;
use crate::text_format::{NamePath, PositionSnapshot};

/// Description carried by the undo command `ai_edit_network` pushes.
///
/// Shared rather than spelled twice: D7 words the divergence marker
/// *"— edit #N undone —"* by recognising this exact string on the undo stack,
/// and a drifted copy would silently downgrade every such marker to the
/// general "changed outside the CLI" wording.
pub const AI_EDIT_COMMAND_DESCRIPTION: &str = "AI edit network";

/// How many entries the ring retains before evicting oldest-first (D10).
pub const AI_EDIT_LOG_MAX_ENTRIES: usize = 200;

/// How many bytes of snapshot text the ring retains before evicting
/// oldest-first (D10). Whichever cap binds first wins.
pub const AI_EDIT_LOG_MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;

/// The serializer's only failure channel: a wire cycle aborts serialization and
/// leaves this comment in place of the remaining statements (root scope, which
/// also drops the footer) or of a body's whole contents.
const SERIALIZER_ERROR_MARKER: &str = "# Error:";

/// Whether a snapshot is a faithful, replayable rendering of the network.
///
/// `false` when the serializer aborted on a wire cycle, so the text is
/// truncated (root scope) or has a gutted body block (D2). Both failures are
/// silent by construction and both would be *misread*: a truncated snapshot
/// makes a diff report a mass deletion that never happened, and — because a
/// body block is total — feeding it back to `edit --replace` would empty the
/// body rather than restore it.
pub fn snapshot_is_complete(text: &str) -> bool {
    !text.contains(SERIALIZER_ERROR_MARKER)
}

/// Milliseconds since the Unix epoch, for [`AiEditRecord::timestamp_ms`].
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Which layout algorithm ran over the **root scope** after the edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutPath {
    /// No layout pass ran — `auto_layout_after_edit` is off, or the edit did
    /// not apply. Emphatically **not** "nothing moved": see [`LayoutOutcome`].
    #[default]
    None,
    /// The whole root scope was reflowed by `layout::layout_network`.
    FullReflow,
    /// Only the edit's delta was relaid out. Reserved for
    /// `doc/design_incremental_layout.md`; nothing produces it yet.
    Incremental,
}

/// Per-edit counts from `design_incremental_layout.md`'s `EditDelta`, so a
/// moved node can be read against whether the edit had any business touching
/// it. `None` until that design lands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeltaCounts {
    pub nodes_added: usize,
    pub nodes_modified: usize,
    pub nodes_removed: usize,
    pub wires_added: usize,
    pub wires_removed: usize,
}

/// One node that changed position across the edit.
///
/// `path` is the scoped node path — `["m1", "d"]`, rendered `m1/d`. Never a
/// bare name: body names are unique per scope only (D9). Same key and same
/// type as the editor's [`NamePath`].
#[derive(Debug, Clone, PartialEq)]
pub struct MovedNode {
    pub path: NamePath,
    pub before: DVec2,
    pub after: DVec2,
}

impl MovedNode {
    /// The path rendered the way `EditResult` and the diff render it: `m1/d`.
    pub fn path_string(&self) -> String {
        self.path.join("/")
    }

    /// How far the node travelled.
    pub fn displacement(&self) -> f64 {
        (self.after - self.before).length()
    }
}

/// What layout did to the drawing — the one field of a record that cannot be
/// reconstructed from anything else, since neither the text snapshots (the
/// format carries no coordinates, D3) nor `EditResult` mention positions.
#[derive(Debug, Clone, Default)]
pub struct LayoutOutcome {
    /// What the layout pass did to the **root scope**.
    ///
    /// `None` here is compatible with a non-empty [`moved`](Self::moved):
    /// `layout::layout_network` iterates `network.nodes` and does not descend
    /// into `Node.zone`, so **body nodes are never reflowed**. They still move
    /// — a body node whose name does not match the pre-edit snapshot is placed
    /// by `auto_layout::calculate_new_node_position` at creation time. So a
    /// body edit can produce a long `moved` list with `path: None`, and an
    /// unqualified reading of this field would call that "nothing was
    /// disturbed" (D9).
    pub path: LayoutPath,
    /// Every node in the network, bodies included.
    pub node_count: usize,
    /// Only the nodes whose position changed, sorted by path. Bounding the
    /// list by *moved* keeps it small: on a well-behaved incremental edit it is
    /// nearly empty, which is exactly the signal being looked for.
    pub moved: Vec<MovedNode>,
    /// The largest distance any node travelled, `0.0` when nothing moved.
    pub max_displacement: f64,
    /// Populated once `EditDelta` exists.
    pub delta: Option<DeltaCounts>,
}

impl LayoutOutcome {
    /// Diff two position snapshots taken by
    /// [`snapshot_node_positions`](crate::text_format::snapshot_node_positions).
    ///
    /// A path present only *after* the edit is a new node, not a moved one, and
    /// a path present only before was deleted; neither is reported.
    pub fn compute(
        path: LayoutPath,
        network: &NodeNetwork,
        before: &PositionSnapshot,
        after: &PositionSnapshot,
    ) -> Self {
        let mut moved: Vec<MovedNode> = after
            .iter()
            .filter_map(|(name_path, after_pos)| {
                let before_pos = *before.get(name_path)?;
                (before_pos != *after_pos).then(|| MovedNode {
                    path: name_path.clone(),
                    before: before_pos,
                    after: *after_pos,
                })
            })
            .collect();
        moved.sort_by(|a, b| a.path.cmp(&b.path));

        let max_displacement = moved
            .iter()
            .map(MovedNode::displacement)
            .fold(0.0f64, f64::max);

        Self {
            path,
            node_count: count_nodes_deep(network),
            moved,
            max_displacement,
            delta: None,
        }
    }
}

/// Total number of nodes in `network`, including every node nested in a zone
/// body at any depth — the same count the serializer's footer reports.
pub fn count_nodes_deep(network: &NodeNetwork) -> usize {
    network
        .nodes
        .values()
        .map(|node| {
            1 + node
                .zone
                .as_ref()
                .map_or(0, |body| count_nodes_deep(body.as_ref()))
        })
        .sum()
}

/// One AI edit, recorded verbatim.
///
/// Nothing is summarized at record time (D8): a summary written now cannot
/// answer a question thought of later, and refining the skill document or the
/// text format means reading what the model actually submitted and what it
/// actually got back.
#[derive(Debug, Clone)]
pub struct AiEditRecord {
    /// Monotonic within the session. Assigned by [`AiEditLog::push`].
    pub seq: u64,
    pub timestamp_ms: i64,
    /// May be empty on the "no active network" path.
    pub network_name: String,
    pub replace: bool,

    /// Exactly what the AI submitted.
    pub code: String,

    /// The editor's own verdict: the statements parsed and were applied. This
    /// is the AI's performance, and the one worth reading when refining the
    /// skill or the format.
    pub applied: bool,
    /// The network validates afterwards — `applied` folded with a
    /// *whole-network* validation verdict (D15 of
    /// `doc/design_hof_body_text_format.md`). This is the state of the drawing,
    /// and the one that says whether it is safe to stop.
    ///
    /// `applied && !success` is ordinary rather than exceptional: an HOF node
    /// created without a body and without a wired `f:` is blocking-invalid by
    /// construction, so the usual two-step of creating a `map` and then filling
    /// its body produces it on the first step every time.
    pub success: bool,

    /// Node **paths** (`m1/a`), not bare names — D10 over in the HOF body
    /// design.
    pub nodes_created: Vec<String>,
    pub nodes_updated: Vec<String>,
    pub nodes_deleted: Vec<String>,
    pub connections_made: Vec<String>,
    /// The three `EditResult` fields it is easy to drop on the floor.
    /// `output_set` is how "the AI re-pointed the network's return node" is
    /// visible at all.
    pub description_set: Option<String>,
    pub summary_set: Option<String>,
    pub output_set: Option<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,

    /// AI text-format snapshots (D2), **not** `.cnnd`. Empty on the
    /// early-return paths, which never reach the network.
    pub before_text: String,
    pub after_text: String,
    /// False when the serializer aborted on a wire cycle — see
    /// [`snapshot_is_complete`].
    pub before_complete: bool,
    pub after_complete: bool,

    /// The previous entry for this network ended in a different state (D7):
    /// something changed it between two `edit` calls — a GUI edit, a file load,
    /// a rename, a `node-policy` change, or an undo.
    pub diverged: bool,
    /// Set when that gap is explained by an undo or redo of an
    /// [`AI_EDIT_COMMAND_DESCRIPTION`] command, which is the common case now
    /// that AI edits are undoable. Changes the marker's wording, not its
    /// detection — `diverged` is still set by the one string comparison.
    pub diverged_by_undo: bool,

    pub layout: LayoutOutcome,
}

impl AiEditRecord {
    /// A record for an edit that never reached the network: no active network,
    /// network not found, or the CLI write lock (D1).
    ///
    /// Recorded rather than dropped: a rejected edit is precisely the kind of
    /// AI-facing feedback this log exists to surface, and the one the AI is
    /// most likely to have handled badly. The snapshots are empty, which also
    /// keeps such an entry out of the divergence comparison at both ends — it
    /// says nothing about what state the network was left in.
    pub fn rejected(
        network_name: String,
        code: String,
        replace: bool,
        errors: Vec<String>,
    ) -> Self {
        Self {
            seq: 0,
            timestamp_ms: now_ms(),
            network_name,
            replace,
            code,
            applied: false,
            success: false,
            nodes_created: Vec::new(),
            nodes_updated: Vec::new(),
            nodes_deleted: Vec::new(),
            connections_made: Vec::new(),
            description_set: None,
            summary_set: None,
            output_set: None,
            errors,
            warnings: Vec::new(),
            before_text: String::new(),
            after_text: String::new(),
            before_complete: true,
            after_complete: true,
            diverged: false,
            diverged_by_undo: false,
            layout: LayoutOutcome::default(),
        }
    }

    /// A record for an edit that reached the network, from the two snapshots
    /// and the `EditResult` fields the caller has unpacked. The two verdicts
    /// are passed separately on purpose (D8).
    #[allow(clippy::too_many_arguments)]
    pub fn applied_edit(
        network_name: String,
        code: String,
        replace: bool,
        applied: bool,
        before_text: String,
        after_text: String,
        layout: LayoutOutcome,
    ) -> Self {
        Self {
            seq: 0,
            timestamp_ms: now_ms(),
            network_name,
            replace,
            code,
            applied,
            success: false,
            nodes_created: Vec::new(),
            nodes_updated: Vec::new(),
            nodes_deleted: Vec::new(),
            connections_made: Vec::new(),
            description_set: None,
            summary_set: None,
            output_set: None,
            errors: Vec::new(),
            warnings: Vec::new(),
            before_complete: snapshot_is_complete(&before_text),
            after_complete: snapshot_is_complete(&after_text),
            before_text,
            after_text,
            diverged: false,
            diverged_by_undo: false,
            layout,
        }
    }

    /// Bytes of snapshot text this record holds, for the byte cap (D10).
    fn snapshot_bytes(&self) -> usize {
        self.before_text.len() + self.after_text.len()
    }

    /// True when the record captured real snapshots, i.e. it reached the
    /// network at all. The early-return paths do not, and must not take part in
    /// divergence detection at either end.
    fn has_snapshots(&self) -> bool {
        !self.before_text.is_empty() || !self.after_text.is_empty()
    }
}

/// The session's append-only ring of [`AiEditRecord`]s.
///
/// Append-only in the strong sense (D6): undo never rewrites it. An entry that
/// was later undone is still a thing that happened, and often the most
/// interesting thing that happened.
#[derive(Debug, Default)]
pub struct AiEditLog {
    records: VecDeque<AiEditRecord>,
    next_seq: u64,
    session_label: String,
    bytes: usize,
    /// Bumped on every push and on `clear`, so the UI's refresh path can decide
    /// whether to re-fetch the list with a `u64` compare instead of shipping
    /// every summary through FFI on every refresh (D12).
    version: u64,
    /// Set when an undo or redo of an AI edit command happened; consumed and
    /// cleared by the next [`push`](Self::push), which is where it becomes
    /// [`AiEditRecord::diverged_by_undo`].
    ai_undo_since_last_record: bool,
}

impl AiEditLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a record, assigning its `seq` and deriving the two divergence
    /// flags, then enforce the caps.
    pub fn push(&mut self, mut record: AiEditRecord) {
        record.seq = self.next_seq;
        self.next_seq += 1;

        // D7: the network changed between two `edit` calls if the state this
        // one found is not the state the previous one left. One string
        // comparison, per network — which is why `/networks/activate` cannot
        // trip it, and why every mutating route that is *not* `edit` (a rename,
        // a load, a `new`, a `node-policy` change that rewrites every
        // `visible:` property) is caught without being instrumented, along with
        // every GUI edit and every undo.
        if record.has_snapshots()
            && let Some(previous) = self.last_after_text(&record.network_name)
        {
            record.diverged = previous != record.before_text;
        }
        record.diverged_by_undo = record.diverged && self.ai_undo_since_last_record;
        self.ai_undo_since_last_record = false;

        self.bytes += record.snapshot_bytes();
        self.records.push_back(record);
        self.enforce_caps();
        self.version += 1;
    }

    /// Note that an undo or redo of an AI edit command has happened, so the
    /// next entry's divergence marker can say so instead of reporting an
    /// unknown outside change (D7).
    pub fn note_ai_edit_undo(&mut self) {
        self.ai_undo_since_last_record = true;
    }

    /// The state the most recent entry *for this network* left behind, ignoring
    /// entries that never reached the network.
    fn last_after_text(&self, network_name: &str) -> Option<&str> {
        self.records
            .iter()
            .rev()
            .find(|r| r.network_name == network_name && r.has_snapshots())
            .map(|r| r.after_text.as_str())
    }

    /// Evict oldest-first until both caps hold. The record just pushed is never
    /// evicted, however large: a log that dropped the entry it was asked to
    /// keep would be worse than one that overshoots its byte cap once.
    fn enforce_caps(&mut self) {
        while self.records.len() > AI_EDIT_LOG_MAX_ENTRIES
            || (self.bytes > AI_EDIT_LOG_MAX_SNAPSHOT_BYTES && self.records.len() > 1)
        {
            match self.records.pop_front() {
                Some(evicted) => self.bytes -= evicted.snapshot_bytes(),
                None => break,
            }
        }
    }

    /// Every retained record, oldest first.
    pub fn records(&self) -> impl Iterator<Item = &AiEditRecord> {
        self.records.iter()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn get(&self, seq: u64) -> Option<&AiEditRecord> {
        self.records.iter().find(|r| r.seq == seq)
    }

    /// The most recent record, whatever network it belongs to.
    pub fn last(&self) -> Option<&AiEditRecord> {
        self.records.back()
    }

    /// Total snapshot bytes currently retained.
    pub fn snapshot_bytes(&self) -> usize {
        self.bytes
    }

    /// The refresh gate of D12.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Drop every entry. `seq` keeps counting: it identifies an edit within the
    /// session, and reusing numbers would make an exported log ambiguous.
    pub fn clear(&mut self) {
        self.records.clear();
        self.bytes = 0;
        self.version += 1;
    }

    /// The user-set session label (D13) — "Opus 5 / skill v3". The application
    /// cannot know which model is driving the CLI, so it is asked.
    pub fn session_label(&self) -> &str {
        &self.session_label
    }

    pub fn set_session_label(&mut self, label: String) {
        self.session_label = label;
        self.version += 1;
    }
}
