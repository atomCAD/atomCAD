# Design: AI edit history

**Status:** first draft for review. Motivated by the need to evaluate
`doc/design_incremental_layout.md` before and after it lands.

**Depended on `doc/design_hof_body_text_format.md`, which has landed** (all five
phases). Before it, the text format did not project HOF zone bodies at all, so a
snapshot was blind to the 141 body nodes in the corpus and a body edit was
invisible in the diff. Two consequences for this design, now in force:

- a zone-bearing node's statement is **multi-line** (that design's D14), so the
  *By node* splitter below must brace-match rather than assume one statement per
  line;
- `EditResult` reports **full paths** (`m1/a`, not `a`), which is what the
  *By node* diff must key on — two bodies may each contain a node named `a`.

A third consequence is about what this design is *not* for. That design's
Phase 5 made `ai_edit_network` push a `TextEditNetworkCommand`, so an AI edit is
now one undo step and the "unrecoverable" half of the problem below is solved.
The log's job is unchanged and unduplicated: undo restores *state*, while this
design records *what was submitted, when, with which flags, and what came back*
— including the entries an undo deliberately does not erase (D6).

Both are recorded here so the diff viewer is built against the final format
rather than retrofitted. Nothing else in this design changes.

**Problem.** The AI edit surface (`ai_edit_network`, reached as
`atomcad-cli edit [--replace]`) is the only mutation path in the application
that leaves **no trace**. After a session of AI editing, the maintainer can see
the *current* network and nothing else: not what the AI submitted, not what the
network looked like before, not whether `--replace` was used, not what warnings
or errors the CLI handed back, and — the reason this document exists now — not
which nodes the layout pass moved.

That last one is the immediate driver. `design_incremental_layout.md`'s entire
success criterion is a negative: *nodes outside the delta must not move*. There
is currently no way to observe that. "Did that edit disturb my drawing?" is
answered today by looking at the canvas and remembering what it looked like.

---

## Is this worth building?

Yes, and for more than the layout work. Four distinct uses, each of which
currently has no instrument:

1. **Validating incremental layout.** Per-edit "N nodes moved, max displacement
   D" is exactly the measurement the design's claims are stated in. Without it,
   Phase 4's corpus regression test is the *only* evidence, and it cannot cover
   real AI behaviour on real drawings.
2. **Refining the `atomcad` skill.** The skill document is a prompt, and the
   only honest way to improve a prompt is to read what the model actually did
   with it. The submitted script plus the returned errors and warnings is that
   record.
3. **Refining the CLI and the text format.** A warning the AI ignored, an error
   it retried three times, a `--replace` where a merge would have done — all
   invisible today, all obvious in a log.
4. **Tracking model improvement over time.** Same task, same skill, different
   model; the exported logs are directly comparable.

The cost is small: the recording is a couple of string captures at one call
site, the storage is in memory only, and the UI reuses a panel template the
application already has twice.

---

## Where it goes

**Recording:** in Rust, inside `ai_edit_network`
(`rust/src/api/structure_designer/ai_assistant_api.rs`).

**Presentation:** a bottom-docked **AI History** panel, the third sibling of
`console_panel.dart` and `profiler_panel.dart`, with a *View* menu entry and the
same collapse-to-zero-height behaviour. Master–detail: entry list on the left,
diff and payloads on the right.

Both choices are argued below (D1, D11).

---

## Design decisions

**D1 — Record in Rust, at `ai_edit_network`.** It is the single choke point.
Every AI edit arrives there regardless of transport — the HTTP server's `/edit`
handler, the CLI REPL's `edit`/`replace` modes, and any future direct FFI caller
all funnel through it. Recording in the Dart HTTP layer would miss anything that
does not go over HTTP and would need a second capture round-trip
(`aiQueryNetwork` before and after) to get the snapshots at all.

Recording covers **every** exit path, including the three early returns that
never touch the network: no active network, network not found, and
`is_cli_write_locked`. Those produce a failed entry with an empty diff. A
rejected edit is precisely the kind of AI-facing feedback this log exists to
surface, and it is the one the AI is most likely to have handled badly.

**D2 — The record is a pair of snapshots in the AI text format, not a delta.**

> **Which serializer.** Throughout this document, "snapshot" means the output of
> `atomcad_structure_designer::text_format::serialize_network` — the
> **human-readable AI text format**, the one `ai_query_network` returns and
> `atomcad-cli query` prints. It is **not** the `.cnnd` file format, which is
> written by a different function in a different module
> (`node_networks_serialization::save_node_networks_to_file`, reached via
> `StructureDesigner::save_node_networks_as`) and is never touched by this
> design. `serialize_network` is the only function of that name in the tree, but
> the names are close enough to be worth pinning down once, here.

`text_format::serialize_network` is called immediately before the edit and
immediately after validation. This one decision is what makes the whole feature
work in `--replace` mode: replace destroys node identity entirely, so any
identity-based delta degenerates to "everything was deleted and everything was
created", but the *text* is comparable regardless of how the edit was performed.
One representation covers both modes, and the diff the maintainer reads is the
same diff in both.

It also means the diff is in the AI's own dialect: a snapshot is exactly what
`query` would have returned at that moment, so an entry's "before" text can be
fed straight back to `edit --replace`, and any gap between what the AI meant and
what the network became is visible in the same syntax the skill document is
written in. A `.cnnd` JSON diff would be unreadable for that purpose.

The snapshot is deterministic: `NetworkSerializer::topological_sort` seeds its
DFS from node ids sorted ascending, and the dependency and entry orderings are
sorted too, so the same network serializes byte-identically across runs. Without
that property a diff would be pure noise; it happens to already hold.

**D3 — The AI text format carries no positions, so it cannot show layout.**
Verified: `text_format`'s serializer emits no node coordinates (the `.cnnd`
format does, which is another reason not to reach for it here). That is a
feature — the text diff is a clean semantic record of *what the AI changed*,
with zero layout noise — but it means layout must be recorded on a separate axis
(D9). The two questions "what did the AI change?" and "what did the layout do to
my drawing?" are orthogonal, and the panel answers them in separate tabs.

**D4 — Two diff views: *By node* (default) and *Text*.** A plain line diff
over a text-format snapshot has a known false-positive mode: the output is
topologically sorted, so inserting one upstream node can shift every downstream
statement, and the line diff reports a large move as a large change.

*By node* avoids it. Every statement in the text format begins
`name = type { … }`, so the output splits cleanly into per-node blocks keyed by
name. Diffing the **set** of blocks — added / removed / changed / unchanged,
listed in name order — is stable against reordering and is the view that answers
"what did this edit do". *Text* is kept as the literal unified diff, because
sometimes the literal truth is what is wanted (and because header, footer and
`output` statements live outside any node block).

**D5 — Diffs are computed on demand, not at record time.** The record stores
snapshots; `ai_history_diff(seq, mode)` computes when the panel asks. Keeps the
record a pure capture, keeps the recording path cheap, and lets the two views be
added independently.

**D6 — The log is append-only and is never rewritten by undo.** Undoing an AI
edit does not remove its entry. This is a log of what the AI did, not a
representation of document state; an entry that was later undone is still a
thing that happened, and often the most interesting thing that happened.

**D7 — Divergence between entries is detected and shown.** When recording, the
new `before_text` is compared against the `after_text` of the most recent entry
*for the same network*. If they differ, the entry is flagged `diverged` and the
panel draws a marker between the rows: *"this network changed outside `edit`
since the previous entry."*

The wording is deliberate. `ai_edit_network` is the only choke point for **text
edits**, but `/networks/rename`, `/load` and `/new`
(`ai_assistant/http_server.dart:931, 1008, 1118`) also mutate — and a rename
changes the `# Network:` header line, so the next edit would trip the marker
about something the CLI itself did. Phase 1 therefore records those three
endpoints as well (they are single call sites), and the marker claims only what
it can prove: the state changed between two `edit` calls.

This is what makes the log honest without logging GUI edits. Human drags, GUI
node edits, undos and file loads all show up as a gap in the right place, at the
cost of one string comparison. It also directly serves the maintainer's stated
workflow: knowing whether the AI or the human made a given change.

**D8 — Request and response are recorded verbatim.** The exact `code` string
submitted, the `replace` flag, and the full `EditResult` (created / updated /
deleted node names, connections made, errors, warnings). No summarizing at
record time — a summary written now cannot answer a question thought of later.
This is the raw material for uses 2 and 3 above.

**D9 — The layout outcome is recorded per edit.** A `LayoutOutcome` alongside
the snapshots:

- which path ran — `None` / `FullReflow` / `Incremental`;
- total node count, number of nodes whose position changed, maximum
  displacement;
- the list of moved nodes, by **name**, with before and after positions;
- once `design_incremental_layout.md` Phase 1 lands, the `EditDelta` counts
  (added / modified / removed, added and removed wires), so a moved node can be
  read against whether the edit had any business touching it.

This is the instrument for use 1, and it is the field that cannot be
reconstructed from anything else — neither the text diff (D3) nor the
`EditResult` mentions positions. Bounding the list by *moved* nodes keeps it
small: on a well-behaved incremental edit it is nearly empty, which is exactly
the signal being looked for. On a full reflow it is every node, which is also
exactly the signal.

**D10 — Memory only, capped, exportable.** Not written to `.cnnd`, per the
requirement. A ring buffer with two limits — entry count and total snapshot
bytes — evicting oldest-first. Export to a file is how a session leaves the
process; it is the feature, not an afterthought, because every downstream use
(refine the skill, compare models) happens outside the application.

**D11 — A bottom-docked panel, not a dialog.** The Console and Profiler panels
already establish the pattern — state on `StructureDesignerModel`, a *View*
menu toggle, zero height when hidden — and a third one costs nothing to learn.
The panel is full-width, which is what a master–detail diff view wants; height
is the scarce dimension, so the detail pane gets an **expand** button that opens
the same view in a large dialog for deep reading.

**D12 — Summaries and payloads travel separately over FFI.** `ai_history_list`
returns lightweight rows (seq, timestamp, network, flags, counts) with no
snapshot text; code, results and diffs are fetched per selected entry. Shipping
every snapshot on every refresh would put megabytes through FFI on a path that
`design_eval_profiling.md` D8a exists to keep cheap.

Even the list is not free: it allocates up to 200 structs of several heap
`String`s each, and the cost grows with session length. So `refreshFromKernel`
calls a cheap `ai_history_version() -> u64` and re-fetches the list only when
that changed **and** the panel is visible — the pull-in-`build` pattern
`profiler_panel.dart` already uses. Nothing else is safe on that path.

**D13 — A session label, set by the user.** The application cannot know which
model is driving the CLI. A free-text field on the panel toolbar ("Opus 5 /
skill v3"), stamped into exports, is one line of UI and solves it. A protocol
change — a `X-Client-Label` header from `atomcad-cli` — is strictly better and
strictly later; see open question 5.

---

## Data model

In `atomcad-structure-designer` (the log is domain state, not API state), stored
on `StructureDesigner` next to `print_log`:

```rust
pub struct AiEditRecord {
    pub seq: u64,                 // monotonic, session-scoped
    pub timestamp_ms: i64,
    pub network_name: String,     // may be empty on the "no active network" path
    pub replace: bool,

    /// Exactly what the AI submitted.
    pub code: String,

    /// Exactly what it got back.
    pub success: bool,
    pub nodes_created: Vec<String>,
    pub nodes_updated: Vec<String>,
    pub nodes_deleted: Vec<String>,
    pub connections_made: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,

    /// AI text-format snapshots (D2), NOT `.cnnd`.
    /// Empty on the early-return paths.
    pub before_text: String,
    pub after_text: String,

    /// The previous entry for this network ended in a different state (D7).
    pub diverged: bool,

    pub layout: LayoutOutcome,
}

pub enum LayoutPath { None, FullReflow, Incremental }

pub struct LayoutOutcome {
    pub path: LayoutPath,
    pub node_count: usize,
    pub moved: Vec<MovedNode>,     // only nodes whose position changed
    pub max_displacement: f64,
    /// Populated once `EditDelta` exists (design_incremental_layout.md P1).
    pub delta: Option<DeltaCounts>,
}

pub struct MovedNode { pub name: String, pub before: DVec2, pub after: DVec2 }

pub struct AiEditLog {
    records: VecDeque<AiEditRecord>,
    next_seq: u64,
    session_label: String,
    bytes: usize,
}
```

`AiEditLog` enforces the D10 caps on push. Suggested defaults: **200 entries**
and **32 MB** of snapshot text, whichever binds first. Both are internal
constants.

### Recording sequence inside `ai_edit_network`

```
1. resolve network name        → on failure: push a failed record, return
2. check the CLI write lock    → on failure: push a failed record, return
3. before_text = text_format::serialize_network(network, registry, name)
   before_positions = {custom_name → position}
4. (existing) remove from registry, text_edit_network, reinsert, validate
5. (existing) layout pass, if enabled
6. after_text = text_format::serialize_network(...)
   after_positions = {custom_name → position}   → LayoutOutcome
7. push the record
8. (existing) mark_full_refresh, set_dirty, refresh
```

Steps 3 and 6 each borrow the registry twice immutably — the network and the
registry it lives in — which is exactly what `ai_query_network` already does.
The snapshots bracket validation and layout, so a partially-applied edit (some
statements succeeded, a later one failed) shows its real partial effect rather
than nothing.

**Positions are keyed by name, never by node id.** `clear_network`
(`text_format/network_editor.rs:220`) removes every node without resetting
`next_node_id`, so a `--replace` rebuild mints fresh, higher ids for everything
and an id-keyed comparison matches nothing — reporting "nothing moved" on the
single most layout-destructive kind of edit. Name keying is the same match
`design_incremental_layout.md` specifies for replace mode; a name present only
after the edit is a new node, not a moved one.

Two `text_format::serialize_network` calls per edit is the entire runtime cost,
on a path that already runs `validate_network`, a layout pass and a full
refresh.

---

## Diff computation

Both views live in the domain crate and are computed on demand (D5).

**By node.** Split each snapshot into `(name → statement block)` maps by
matching the leading `name =` of each statement, keeping non-statement lines
(header comment, `output …`, footer) in a small "preamble/other" bucket. Then:

- `name` in after only → **added**
- `name` in before only → **removed**
- present in both, block text differs → **changed**, with a line diff *within*
  the block
- present in both, identical → **unchanged**, collapsed

Ordering is by name, ascending, so the view is stable across edits.

**Text.** A unified line diff over the two snapshots with the standard three
lines of context, unchanged runs collapsed.

Both use the `similar` crate. It is already in `rust/Cargo.lock` at 2.7.0 (as an
`insta` dependency), so promoting it to `[workspace.dependencies]` and taking it
as a direct dependency of `atomcad-structure-designer` adds no new code to the
tree.

**What to strip, precisely.** The volatile part is the **footer**, not the
header: `serialize` ends with `
# N nodes` (`network_serializer.rs:113-117`),
which churns on every edit that changes the node count. That line is dropped
from both snapshots before diffing. The **header** is `# Network: <name>`
(`network_serializer.rs:59-61`), which changes only on a rename and is worth
seeing; and the `description "…"` / `summary "…"` lines that follow it are real,
AI-editable statements that must stay **in** the diff. In *By node* the header
and footer land in the "other" bucket; the `description` and `summary`
statements are diffed there too.

---

## API surface

A new FRB module `rust/src/api/structure_designer/ai_history_api.rs`.

> **It must be added to `flutter_rust_bridge.yaml`'s `rust_input` list.** A new
> API module that is not listed generates nothing and fails silently — the same
> trap `project_atom_tags` hit.

```rust
#[frb(sync)] pub fn ai_history_list() -> Vec<APIAiEditSummary>;
#[frb(sync)] pub fn ai_history_detail(seq: u64) -> Option<APIAiEditDetail>;
#[frb(sync)] pub fn ai_history_diff(seq: u64, by_node: bool) -> Option<APIAiDiff>;
#[frb(sync)] pub fn ai_history_export_json() -> String;
#[frb(sync)] pub fn ai_history_export_markdown() -> String;
#[frb(sync)] pub fn ai_history_clear();
#[frb(sync)] pub fn ai_history_set_session_label(label: String);
#[frb(sync)] pub fn ai_history_get_session_label() -> String;
```

`APIAiEditSummary` carries what a list row needs and nothing more: `seq`,
`timestamp_ms`, `network_name`, `replace`, `success`, counts of
created/updated/deleted, error and warning counts, `diverged`, `moved_count`
(D12).

`APIAiDiff` is a list of hunks, each `{ kind, node_name, lines }` where a line
is `{ tag: Same|Add|Remove, text }` — rendered, not re-parsed, by Dart.

Export writes through the standard file-save path so it picks up the
last-directory behaviour from `project_issue_420_last_directories`. JSON is the
canonical form (machine-comparable across sessions and models); Markdown is a
convenience for pasting a session into a skill-refinement conversation.

---

## UI

### Panel

```
┌ AI History ──────────────────────────────── [label: ______] [Export] [Clear] [x] ┐
│ #14 14:07:31 ✔ beam        +3 ~1 −0   ⌂2                │  ┌ Diff │ Request │ Result │ Layout ┐ │
│ ── changed outside the CLI ──────────────               │  │ (•) By node  ( ) Text     [⤢]  │ │
│ #13 14:05:02 ✖ beam  REPL  ⚠1 ✖1                        │  │ + tip1 = sphere { … }          │ │
│ #12 14:03:22 ✔ tool_holder +9 ~0 −2   ⌂9                │  │ ~ union1 = union { shapes: … } │ │
└──────────────────────────────────────────────────────────┴────────────────────────────────────┘
```

- **Row:** sequence, time, success glyph, network name, a **REPL** badge when
  `replace` was used (unmissable, per the requirement), created/updated/deleted
  counts, a `⌂` moved-node count, and error/warning badges. Failed entries are
  tinted.
- **Divergence marker** between rows when `diverged` (D7).
- **Detail tabs:** *Diff* (By node / Text toggle, `⤢` expands to a dialog),
  *Request* (the submitted script, monospace, selectable), *Result* (errors,
  warnings, connections made), *Layout* (path, moved count, max displacement,
  the moved-node table).
- Error and warning text follows `project_issue_359_copyable_errors`:
  persistent text is selectable.
- Toolbar: session label field (D13), Export, Clear.

### Model and menu wiring

On `StructureDesignerModel`: `aiHistory: List<APIAiEditSummary>`,
`aiHistoryPanelVisible`, `selectedAiHistorySeq`, `unreadAiEditCount`.

`refreshFromKernel()` calls `ai_history_list()` — cheap by D12 — and bumps
`unreadAiEditCount` when the panel is hidden, mirroring the Console panel's
unread dot. That refresh already fires after every AI edit via
`main.dart`'s `_aiServer.onNetworkEdited`, so the panel updates live while the
AI works.

*View > Show/Hide AI History*, next to the Console and Profiler entries in
`structure_designer.dart`. No new global keyboard shortcut (open question 4).

---

## Phases

### Phase 1 — Recording core
`AiEditRecord` / `AiEditLog` on `StructureDesigner`, the ring-buffer caps, the
snapshot capture and `LayoutOutcome` computation in `ai_edit_network`, including
all three early-return paths and the divergence flag.

*Tests:* a merge edit records both snapshots and the submitted code verbatim; a
`--replace` edit records `replace = true` and two comparable snapshots; a
write-locked edit records a failed entry with the lock message and empty
snapshots; a syntactically broken script records the errors *and* whatever
partial state resulted; an edit that follows a GUI change sets `diverged`, and
two consecutive AI edits do not; the entry-count cap evicts oldest-first; the
byte cap evicts on a large snapshot; the log is absent from a saved `.cnnd` and
survives a load (open question 3 decides which); `LayoutOutcome.moved` is empty
when `auto_layout_after_edit` is off and non-empty for a reflow that moved
something.

### Phase 2 — Diff engine and API
The *By node* and *Text* diffs, `similar` promoted to a workspace dependency,
the `ai_history_api.rs` module and its `flutter_rust_bridge.yaml` entry.

*Tests:* a one-node addition yields exactly one added block in *By node*; an
edit that inserts an upstream node and reorders the topological output shows
**one** changed block in *By node* while *Text* shows the move (this is the test
that would fail on a naive line diff); a `--replace` of an unchanged script
yields an empty *By node* diff — the direct analogue of
`design_incremental_layout.md`'s name-match test; a property change shows a
line-level diff inside the block; a deleted node appears as removed; the
serializer header never appears as a change.

### Phase 3 — Panel
The panel, the list, the four detail tabs, the *View* menu entry, the unread
dot, the expand dialog, the model fields.

### Phase 4 — Export and session label
JSON and Markdown export through the file-save path, the session label field,
`ai_history_clear`. Reference guide: `doc/reference_guide/ui.md` for the panel
and its menu entry, `doc/reference_guide/headless_cli.md` for the note that CLI
edits are recorded and how to export them.

*Manual verification* (per `feedback_manual_test_for_editor_ui`): run an AI
session against a hand-drawn network with the panel open; confirm merge vs.
replace are distinguishable at a glance; make a GUI edit between two AI edits
and confirm the divergence marker; trigger a CLI error and read it in the panel;
export and re-read the JSON.

### Phase 5 (later) — Wider capture
Log non-edit CLI traffic (`/query`, `/screenshot`, `/networks/*`, `/load`,
`/save`) as lightweight timeline entries via a single hook in
`_handleRequest`, so the log shows what the AI *looked at* before it edited.
Optional `X-Client-Label` header from `atomcad-cli` to replace the manual
session label.

---

## Interaction with other subsystems

**Incremental layout (`design_incremental_layout.md`).** This design is that
one's measurement instrument, and it should land first, or at least alongside
Phase 1 of it. `LayoutOutcome.path` distinguishes the two algorithms, and
`LayoutOutcome.delta` — populated from `EditDelta` once it exists — turns
"7 nodes moved" into "7 nodes moved, of which 2 were in the delta", which is the
sentence the whole design is trying to make true. The recording is deliberately
written so that it works before that design lands (`path: FullReflow`,
`delta: None`) and gains fidelity when it does.

**Undo.** None. The log is not document state and takes no undo command (D6).
It is exempt from `feedback_persisted_mutations_must_be_undoable` for the same
reason `print_log` is: nothing about it is persisted.

**CLI write lock.** Rejected edits are recorded (D1). Worth stating in
`headless_cli.md`: locking a network does not hide the attempts.

**Console / Profiler panels.** A third bottom-docked panel stacked in the same
`Column`. Three collapsed panels cost three zero-height widgets; if a fourth
ever appears, the stack should become a tabbed dock — noted, not done here.

---

## Open questions

1. **What are the right caps?** 200 entries / 32 MB is a guess. The failure mode
   is losing the start of a long session, which is the part most worth keeping
   — an alternative is to cap only bytes and evict by dropping *snapshots* from
   old entries while keeping their code and results, which are tiny.
2. **Should snapshots be deduplicated?** `after_text` of entry N is usually
   byte-identical to `before_text` of entry N+1, so a hash-keyed arena would
   roughly halve storage. Recommendation: not in v1 — the caps make it a
   non-problem, and D7's comparison is clearer against plain owned strings.
3. **Does the log survive `File > New` and `File > Open`?** Recommendation: yes,
   with a divider entry naming the loaded file. It is a session log, not a
   document log, and an AI that loads a file mid-session is doing something the
   log should show rather than forget.
4. **A keyboard shortcut?** Console holds Ctrl+`. Recommendation: menu entry
   only for now.
5. **Should `atomcad-cli` identify itself?** A `X-Client-Label` header would
   make the session label automatic and per-request rather than per-session, and
   would let a single log distinguish two models editing in turn. Deferred to
   Phase 5 because it touches the CLI, the skill and the server.
6. **Should an entry offer "revert to this snapshot"?** Recommendation: no. It
   would turn a diagnostic into an editing tool, it interacts badly with undo,
   and `edit --replace` with the exported text already does it explicitly.
7. **Should GUI edits be logged too?** D7 detects them without recording them,
   which is most of the value for a fraction of the work. A full "everything
   that changed this network" log is a different, larger feature.
