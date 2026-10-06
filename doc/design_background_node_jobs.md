# Background Node Jobs: `chemisorb` Run Off the UI Thread

## Motivation

Pressing **Run** on a `chemisorb` node freezes the whole application for as
long as the search takes — seconds to minutes. `run_chemisorb` is a
`#[frb(sync)]` call, so the search runs on the Dart UI thread: no input, no
viewport, no repaint. The panel hides this behind a static modal placard
(`runExecuteWithPlacard`'s recipe), which cannot even animate, has no progress
and no way out but waiting or killing the process.

This document moves the search to a worker thread and adds:

1. **A live UI** while the search runs — the user can orbit, select, edit,
   switch tabs.
2. **An in-progress indication** on the property panel, on the node in the
   node network editor, and on the document tab.
3. **Percentage progress** and **cancellation**.

It is designed for `chemisorb` alone, but as a small reusable layer — a
**node job** — so that the next expensive *run-on-demand* node (one that, like
`chemisorb`, computes an expensive result only on an explicit user action and
stores it on its node data) gets all of the above by implementing one
`NodeData` hook and two small traits.

### Scope

- **In:** `chemisorb`'s Run, and the generic node-job layer it is built on.
- **Out:** the right-click **Execute** action
  (`doc/design_node_execution.md`) — it evaluates arbitrary network
  structure, which needs `NodeData: Send + Sync` and a registry snapshot; see
  "Relation to other designs". **Out:** background *evaluation* — expensive
  work that runs in `eval` on every refresh (`relax`), which is
  `doc/design_background_evaluation.md`. **Out:** running `chemisorb`
  searches inside an Execute pass.

## Current state (facts, with references)

- `chemisorb`'s `eval` never searches; it runs the cheap `plan` and outputs a
  stored result **only while the current inputs hash to the fingerprint the
  result was computed from** (`nodes/chemisorb.rs` module doc,
  `ChemisorbData::stored: Option<Arc<StoredSearch>>`, `#[serde(skip)]`, not
  undoable, not saved).
- Run is `StructureDesigner::run_chemisorb` (`chemisorb_ops.rs`). It:
  1. guards (`ensure_active_editable`, top-level scope only, node is a
     `chemisorb`);
  2. evaluates the three inputs through `with_eval_context` — fast;
  3. builds the `ChemisorptionSearch` config and the `input_fingerprint`;
  4. calls `atomcad_crystolecule::chemisorption::search` — **the slow part**;
  5. writes `stored = Some(Arc::new(StoredSearch { fingerprint, report }))`
     through `get_node_network_data_mut_scoped` (which marks the node's data
     changed) and returns a `ChemisorbRunSummary`.
- The api wrapper `run_chemisorb` (`chemisorb_api.rs`) then calls
  `refresh_structure_designer_auto`. `run_chemisorb_node` is the same for the
  CLI's `run` command, by node name, returning text (`format_run_result`).
- `search` = `plan` + `evaluate` (`chemisorption/report.rs`). `evaluate`
  relaxes the no-change reference first, then every hypothesis with
  `plan.hypotheses.par_iter().try_for_each(…)` on the **global** rayon pool,
  keeping each relaxed candidate only while it is among the best `top_n`
  (`keep_best` under a `Mutex`), then applies the energy window. The number of
  relaxations is known once `plan` returns (`plan.stats.to_relax`, plus one
  for the reference). The data crossing rayon's threads (`AtomicStructure`,
  `ChemisorptionSearch`, `Candidate`) is already `Send + Sync` — that is what
  `par_iter` requires.
- **Every `chemisorb` setting is a search setting.** The filters (formed-bond
  count, bond inventory) prune in `plan`; `top_n` and the energy window are
  applied while relaxing. All of them are in `ChemisorptionSearch` and
  therefore in `input_fingerprint`: changing any of them makes a stored result
  stale and needs a new Run. There is no "re-list a stored result" path.
- Each relaxation is one `minimize_with_force_field` call (L-BFGS, up to
  `max_iterations`, ≤ 2000 free atoms by `check_minimize_limits`). It has no
  progress or cancellation hook.
- `CAD_INSTANCE` is an unsynchronized `static mut` (`api/api_common.rs`), and
  `provide_texture` is called from a per-frame callback. This is why
  `doc/design_node_execution.md` rejected worker threads, and why
  `doc/design_background_evaluation.md` needs a `Mutex` around the global.
- The panel (`chemisorb_editor.dart`) shows `_RunRow` — the Run button, the
  plan's hypothesis count, a red stale line — and on Run shows the static
  placard around the sync `model.runChemisorb`.

## Design summary

```
UI thread  ──  prepare  ──►  worker pool  ──  run  ──►  UI thread  ──  install
 (sync FFI: guards,          (pure computation on        (sync FFI poll: write
  evaluate inputs,            owned data; reports         the result into the
  build config)               progress, checks cancel)    node, refresh)
```

- **Prepare** and **install** run on the UI thread inside ordinary sync FFI
  calls, exactly like every other API call today.
- **Run** gets only *owned* data — the evaluated input structures and the
  config — and never touches `CAD_INSTANCE`, the registry, or any node. So
  **no lock is needed**: nothing is shared between the worker and the UI thread
  except the job's own control block and its result slot. The
  `doc/design_background_evaluation.md` prerequisites (Send+Sync
  domain types, a `Mutex` around the global) are not required.
- **The fingerprint makes install unconditionally safe.** Whatever the user
  did while the search ran — edited the inputs, changed a search setting,
  undid something — installing the result can only ever *miss*: `eval` shows
  it only if the node's current inputs and settings hash the same. No
  generation counter, no "was the network edited?" check, no edit lock.
- **Progress and cancel** go through one small control block,
  `JobControl`, defined at the bottom of the crate DAG so the chemistry code
  can report into it without knowing anything about nodes.
- **Flutter polls** a sync `poll_node_jobs` at ~10 Hz while any job runs or
  a finished result still waits to be installed. Finished results are
  installed during that poll.

## Decisions

**D1 — Explicit jobs only.** A node job is started by a user action on one
node and produces a result that is installed on that node. Evaluation-time
work (`relax`) is not a job; Execute is not a job (yet — see "Relation to
other designs").

**D2 — Snapshot in, result out; the worker never sees the global.** This is
what removes the need for a lock and for Send+Sync node data. The compiler
enforces it through the bounds on `JobWork`: `'static` means the work cannot
borrow anything from the designer, the registry or a node — it can only own
copies — and `Send` means what it owns may cross to a worker thread.

**D3 — Edits during a run are allowed and need no handling.** The fingerprint
(`input_fingerprint`) covers every input and every setting — since every
`chemisorb` setting is a search setting, a change to *any* of them during a run
makes the result land stale. A run-on-demand node that adopts this
design **must** key its stored result by such a fingerprint — it is the
correctness argument of this whole document. A node that cannot fingerprint its
inputs is not a candidate for a node job.

**D4 — Jobs run on a dedicated rayon pool.** `evaluate` uses `par_iter`. Run on
the global pool, a search occupies every core, and the UI thread's own
evaluations — which also use the global pool (`batched_implicit_evaluator`,
geo-tree work) — queue behind it: the UI would be live but would stutter on
every edit. A dedicated `rayon::ThreadPool` with
`max(1, available_parallelism − 1)` threads leaves the global pool to
interactive work. `par_iter` called inside a task running on a pool uses that
pool, so `evaluate` needs no change for this.

**D5 — Poll, don't push.** Dart polls a sync `poll_node_jobs(defer_installs)` on a
`Timer.periodic` (100 ms) that exists only while a job runs or a finished
result waits to be installed.
This keeps the existing pull model (`refreshFromKernel`, `takePrintLog`), needs
no `StreamSink` (the codebase has none), and puts install on the UI thread for
free. A poll is a few atomic loads; its cost is negligible. Push can replace it
later without changing the Rust side's job model. The timer lives in the host
widget (`structure_designer.dart`) next to the dependency poll, because the
D11 guard it evaluates is that widget's state.

**D6 — Progress and cancel granularity: one relaxation.** `done/total` counts
relaxations (`total = to_relax + 1` once the plan exists; indeterminate
before). Cancel is checked before each relaxation starts. Latency of a cancel
is therefore at most one in-flight relaxation per pool thread — typically well
under a second. A per-iteration check inside the minimizer is a follow-up only
if measurement shows long single relaxations (open question 1).

**D7 — One job per node; any number of nodes.** Starting a job on a node
that already has one — running, or cancelled but not yet returned — is
refused (`"A job is already running on this node"`; the layer is generic, so
the message is too); the panel shows **Cancel** / "Cancelling…" in that state
anyway. Jobs on different
nodes — in the same or in different documents — run concurrently and share the
job pool.

**D8 — Jobs are session state, owned by `DocumentSet`.** A job outlives a tab
switch, so the job list cannot live on a `StructureDesigner`; it lives on
`DocumentSet`, which already is the session's state across documents. A job's
target is `(DocumentId, network name, scope path, node id)`. Install goes to
the active designer or to a parked one. Jobs are cancelled **inside
`DocumentSet`, at the two functions every affected path already goes
through**, not by the API wrappers:

- `drop_parked` — every close, the active tab's included (`close` activates a
  neighbour and then drops the old designer);
- `renumber_active` — every in-place replacement (`load_in_place`,
  `new_project_in_place`), which also mints the fresh `DocumentId` that makes
  an old job's install miss.

So a future close or replace path cannot forget to cancel.

**D9 — The blocking Run is the same job, run inline, and generic.**
`StructureDesigner::run_node_job_blocking(scope_path, node_id)` is
`prepare → run (on the calling thread, no control) → install`, for any node
with a job. The CLI's `run` command (which reaches the app through the AI
HTTP server, `lib/ai_assistant/http_server.dart`) uses it, so `run` works for
every future run-on-demand node, not just `chemisorb`, and there is exactly
one implementation of what a search *is*.

**D10 — Job state is never node data.** Running, progress and outcome are not
saved, not undoable, and not part of the `.cnnd`. Only the installed result
lands on the node, under the rules it already has (`#[serde(skip)]`,
`inherit_runtime_state`).

**D11 — Install waits for open interactions, Rust's and Flutter's.** A
job that finished **successfully** stays in its slot, and the next poll tries
again, while either side reports an interaction in progress. A failed or
cancelled job has nothing to install and is reported at once. The two sides:

- **Rust-side:** `StructureDesigner::open_interaction` (a node, atom, gadget
  or property drag; a body resize; a comment edit). Installing marks node data
  changed and refreshes; doing that in the middle of a coalesced drag would
  refresh under the drag's `skip_downstream` assumptions.
- **Flutter-side:** the interactions Rust cannot see — a pointer held down
  (any drag, camera drags included), a focused text field, a dialog or menu on
  top, a wire being dragged. An install ends in `refreshFromKernel()`, which
  would rebuild a property field mid-typing or the editor under a wire drag.
  These are exactly the interactions the library-refresh poll already guards
  (`lib/structure_designer/AGENTS.md`, "Change detection triggers"); the job
  poll reuses that guard and passes its verdict as `poll_node_jobs`'
  `defer_installs` argument.

Polls arrive at 10 Hz, so the deferral is invisible. A deferred result keeps
the poll loop alive (`pending_installs`, below) so it is installed as soon as
the interaction ends.

## Architecture

### `JobControl` — `atomcad-util`

```rust
// crates/atomcad-util/src/job_control.rs
/// Progress and cancellation for one long computation, shared between the
/// thread that runs it and the thread that watches it. Cheap to poll; every
/// field is an atomic, except the phase text.
#[derive(Default)]
pub struct JobControl {
    cancel: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,            // 0 = unknown (indeterminate)
    phase: Mutex<String>,        // "Planning", "Relaxing", …
}

impl JobControl {
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
    pub fn set_total(&self, total: u64);
    pub fn advance(&self, n: u64);
    pub fn set_phase(&self, phase: &str);
    pub fn snapshot(&self) -> JobProgress;   // { done, total: Option<u64>, phase }
}
```

It lives in `atomcad-util` because it must be visible to `atomcad-crystolecule`
(which reports into it) and `atomcad-structure-designer` (which owns jobs), and
`util` is the one crate both depend on. It names nothing domain-specific. It
replaces the `SimulationMonitor` trait sketched in
`doc/design_background_evaluation.md`, which can adopt it when that design is
implemented.

All atomics use `Relaxed` ordering: the values are advisory, and the result
itself crosses threads through the job slot's `Mutex`, which carries the
happens-before edge.

### Chemistry side — `atomcad-crystolecule::chemisorption`

```rust
pub fn evaluate(plan: &SearchPlan, config: &ChemisorptionSearch,
                control: Option<&JobControl>) -> Result<SearchReport, ChemisorptionError>;
pub fn search(adsorbate: &AtomicStructure, substrate: &AtomicStructure,
              config: &ChemisorptionSearch,
              control: Option<&JobControl>) -> Result<SearchReport, ChemisorptionError>;
```

- `search` sets phase "Planning" before `plan`, then `set_total(to_relax + 1)`
  and phase "Relaxing".
- `evaluate` checks `is_cancelled()` before the reference relaxation and at the
  top of the `try_for_each` closure, returning
  `Err(ChemisorptionError::Cancelled)`; `try_for_each` short-circuits the
  remaining work, and the candidates kept so far are dropped with the `Mutex`.
  It calls `advance(1)` after each finished relaxation.
- New variant `ChemisorptionError::Cancelled` (`"search cancelled"`).
- `None` keeps today's behaviour byte for byte — every existing caller and test
  passes `None`. Cancellation never produces a partial report.
- `plan` is not made cancellable: it is enumeration only and bounded by
  `budget`. It is, however, run on the worker (in `run`, not `prepare`), so a
  large enumeration does not stall the UI either.

### Node-job layer — `atomcad-structure-designer/src/node_jobs/`

One folder module, not more files in the already-flat crate root:

| File | Holds |
|---|---|
| `mod.rs` | `JobTarget`, the `JobWork` / `JobResult` traits, the crate-level types `JobStatus` / `JobOutcome` / `JobPoll`, re-exports |
| `inputs.rs` | `JobInputs` |
| `designer_ops.rs` | `impl StructureDesigner`: `prepare_node_job`, `install_job_result`, `run_node_job_blocking` |
| `runner.rs` | `JobRunner` and its thread pool |
| `document_ops.rs` | `impl DocumentSet`: `start_job`, `cancel_job`, `poll_jobs`, the cancel hooks of D8 |

The crate gains `rayon = { workspace = true }` (it has no `rayon` dependency
today; `atomcad-crystolecule` and `atomcad-geo-tree` do).

```rust
// node_jobs/mod.rs
/// Where a job's result goes. Identity is by document + network + scope + id;
/// a target that no longer resolves (or resolves to another node type) drops
/// the result. Thanks to D3 a wrong-but-same-type node can only miss.
#[derive(Clone, Debug, PartialEq)]
pub struct JobTarget {
    pub document_id: DocumentId,
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
}

/// The worker half: owned inputs, no access to the designer.
pub trait JobWork: Send + 'static {
    /// What the job is called in the UI ("Chemisorption search").
    fn label(&self) -> String;
    fn run(self: Box<Self>, control: Option<&JobControl>)
        -> Result<Box<dyn JobResult>, String>;
}

/// The UI-thread half: writes the result into the target node.
pub trait JobResult: Send + 'static {
    /// `data` is the target node's data; downcast it. Returns the summary a
    /// caller reports (one line per fact — the CLI prints it, the GUI shows
    /// it in a snackbar).
    fn install(self: Box<Self>, data: &mut dyn NodeData) -> Result<String, String>;
}
```

**`NodeData` hook.** One new defaulted trait method:

```rust
/// A run-on-demand node's explicit action: evaluate what it needs and return
/// the work to do off the UI thread. `None` = this node has no job.
fn prepare_job(&self, inputs: &mut JobInputs) -> Option<Result<Box<dyn JobWork>, String>> {
    None
}
```

`JobInputs` (`inputs.rs`) is a short-lived struct that `StructureDesigner`
builds inside `with_eval_context` and that borrows the evaluator, network
stack, registry, evaluation context and node id. It exposes
`eval_input(pin)`, `eval_input_required(pin)` and `context() ->
&NetworkEvaluationContext` (read-only) — generic accessors, not one getter per
field a particular node happens to need (`chemisorb` reads
`context().use_vdw_cutoff`). Inputs are evaluated **during prepare, on the UI
thread** — it is fast, it is what reads the network, and it is the only part
that must.

**`StructureDesigner` entry points** (`designer_ops.rs`):

```rust
/// Guards, then the node's `prepare_job`. Errors are user-facing messages.
pub fn prepare_node_job(&mut self, scope_path: &[u64], node_id: u64)
    -> Result<(JobTarget, Box<dyn JobWork>), String>;

/// Looks the target up in *this* designer, re-checks editability, installs,
/// marks the node's data changed (so the next refresh re-evaluates it and its
/// downstream cone). Not an undo step, does not dirty the file — same as Run
/// today.
pub fn install_job_result(&mut self, target: &JobTarget, result: Box<dyn JobResult>)
    -> Result<String, String>;

/// D9: prepare → run on the calling thread (no control) → install. For any
/// node with a job; returns the install summary.
pub fn run_node_job_blocking(&mut self, scope_path: &[u64], node_id: u64)
    -> Result<String, String>;
```

The guards in `prepare_node_job` are today's `run_chemisorb` guards, made
generic: `ensure_active_editable`, top-level scope only (a body node's inputs
depend on per-iteration values), node exists, node has a job (`prepare_job`
returned `Some`; otherwise "Node N has no run action"). `install_job_result`
repeats the editability check, because a network can become read-only while a
search runs (a library refresh); the result is then dropped with that reason.

**`chemisorb` adapter** (`chemisorb_ops.rs`):

- `ChemisorbData::prepare_work(&self, &mut JobInputs) -> Result<ChemisorbWork,
  String>` evaluates the three pins, builds the config with `search_config`,
  the `transfer_rules` and the fingerprint, and returns a
  `ChemisorbWork { adsorbate, substrate, config, fingerprint }`. The config
  carries every setting, filters and `top_n` included, so the work needs
  nothing else from the node. `prepare_job` is `prepare_work`, boxed.
- `ChemisorbWork::search(self, control) -> Result<ChemisorbOutcome, String>`
  does the work; `JobWork::run` is that call, boxed. `ChemisorbOutcome {
  stored: StoredSearch, summary: ChemisorbRunSummary }`. The summary is
  computed **on the worker** from the report itself, so it is reported even if
  the result later misses.
- `ChemisorbOutcome::install` downcasts to `ChemisorbData`, sets `stored`, and
  returns `format_run_result`'s text (moved down from the api layer to sit
  beside `ChemisorbRunSummary`).
- `StructureDesigner::run_chemisorb` survives only as the typed convenience
  the existing `chemisorb_node_test.rs` asserts on (`ChemisorbRunSummary`).
  It runs the same guards and `JobInputs` construction as `prepare_node_job`
  (factor them into one private helper), calls `prepare_work` — no downcast
  of a `Box<dyn JobWork>`, which has no `Any` — then `search(None)` and
  `install_job_result`, returning the outcome's typed summary. Built from the
  same pieces as `run_node_job_blocking`, so those tests remain the D9
  regression guard. No production caller uses it.

### Job runner — `node_jobs/runner.rs`

```rust
pub struct JobRunner {
    threads: usize,
    pool: OnceLock<rayon::ThreadPool>,     // built on the first `start`
    next_id: u64,
    jobs: Vec<JobEntry>,
}

struct JobEntry {
    id: u64,
    target: JobTarget,
    label: String,                         // "Chemisorption search"
    control: Arc<JobControl>,
    slot: Arc<Mutex<JobSlot>>,
}

enum JobSlot {
    Running,
    Finished(Result<Box<dyn JobResult>, String>),
}

impl JobRunner {
    /// `threads` for the runner's own pool; the app passes
    /// `default_job_threads()` = max(1, available_parallelism − 1) (D4),
    /// tests pass 1 or 2.
    pub fn new(threads: usize) -> Self;
    /// Refuses a second job on the same target (D7). Spawns `work.run` on the
    /// runner's pool; the closure writes its outcome into the slot. A panic
    /// inside `run` is caught (`catch_unwind`) and becomes
    /// `Err("internal error …")`.
    /// The label comes from `work.label()`.
    pub fn start(&mut self, target: JobTarget, work: Box<dyn JobWork>)
        -> Result<u64, String>;
    pub fn cancel(&self, id: u64);
    pub fn cancel_document(&self, document_id: DocumentId);
    /// Jobs whose worker has not returned yet, cancelling ones included.
    pub fn statuses(&self) -> Vec<JobStatus>;
    /// Successfully finished jobs still waiting in their slot (D11).
    pub fn pending_installs(&self) -> usize;
    /// Removes and returns finished jobs, except successful ones whose target
    /// is `deferred` (D11), which stay in place.
    pub fn take_finished(&mut self, deferred: impl Fn(&JobTarget) -> bool)
        -> Vec<FinishedJob>;
}
```

The runner **owns its pool** rather than sharing a process-wide static: a test
builds a runner with a small private pool and cannot interfere with another
test's jobs, and there is no second hidden global beside `CAD_INSTANCE`. The
pool is built lazily, so the CLI and the many `DocumentSet`s created by tests
that never start a job spawn no threads.

`pool.spawn(...)` runs the job on a pool thread, so the `par_iter` inside
`evaluate` uses the same pool (D4). A cancelled job is removed only when its
worker has actually returned — the panel shows "Cancelling…" in between, and
a new Run on the node stays refused until then, so two searches on one node
never overlap.

**Outcome kinds** are decided by the runner, never by parsing a message:
`Err` from `run` while the control is cancelled → **Cancelled** (whatever the
error text); `Err` otherwise, or a caught panic → **Failed**; `Ok` →
installed by the poll, which yields **Finished** or **Dropped**. An `Ok` that
races a cancel is installed like any other — it is a valid result.

### Session side — `node_jobs/document_ops.rs`

`DocumentSet` gains `jobs: JobRunner` (`JobRunner::new(default_job_threads())`
in `DocumentSet::new`) and the job operations, so routing, deferral and
outcome classification are domain code, tested in the crate:

```rust
impl DocumentSet {
    /// `prepare_node_job` on the active designer, then `jobs.start` with the
    /// active document's id in the target.
    pub fn start_job(&mut self, active: &mut StructureDesigner,
                     scope_path: &[u64], node_id: u64) -> Result<u64, String>;
    pub fn cancel_job(&self, job_id: u64);
    /// The install site.
    pub fn poll_jobs(&mut self, active: &mut StructureDesigner,
                     defer_installs: bool) -> JobPoll;
}

pub struct JobPoll {
    pub running: Vec<JobStatus>,
    pub finished: Vec<JobOutcome>,      // kind: Finished | Cancelled | Failed | Dropped
    pub pending_installs: usize,
    /// An install touched the active designer; the caller refreshes once.
    pub active_changed: bool,
}
```

`JobStatus` and `JobOutcome` carry the same fields as their API twins
(`APINodeJobStatus`, `APINodeJobOutcome`, below), with `DocumentId` and a
`JobOutcomeKind` enum in place of the plain API types.

`poll_jobs`:

1. `take_finished`, deferring every successful job when `defer_installs`
   is set, and otherwise those whose target designer reports an open
   interaction (D11).
2. For each finished job, **first** resolve its document: unknown →
   `Dropped("the document was closed")`, whatever the job's kind (a job
   cancelled by a close or an in-place load ends here). Otherwise a Cancelled
   or Failed job is reported as such, and a successful one is installed with
   `install_job_result` into `active` or `parked_mut(id)`: `Ok(summary)` →
   `Finished(summary)`; any error → `Dropped(error)` — "the node no longer
   exists", wrong node type, read-only network.
3. Return running statuses, outcomes, `pending_installs` and `active_changed`.
   Outcomes are delivered **once**.

It does not refresh: refreshing needs the renderer and is the API layer's job,
as for every other `DocumentSet` operation (`activate` returns a report and
the caller refreshes).

A result installed into a *parked* designer only marks the node data changed;
the refresh that activation already performs evaluates it —
`DocumentSet::swap_in` calls `mark_full_refresh()` on the incoming designer,
so no extra refresh is needed (a Phase 2 test pins this).

**Cancellation hooks (D8):** `drop_parked` calls `jobs.cancel_document(id)`
for the dropped id; `renumber_active` calls it for the id being retired. No
API wrapper cancels anything.

### API layer — `rust/src/api/structure_designer/node_jobs_api.rs`

Thin: each function is one `DocumentSet` call plus type conversion.
`CADInstance` is unchanged — the runner lives in `cad_instance.documents`.

```rust
#[frb(sync)] pub fn start_node_job(scope_path: Vec<u64>, node_id: u64) -> Result<u64, String>;
#[frb(sync)] pub fn cancel_node_job(job_id: u64);
/// `defer_installs`: Flutter's half of D11 — true while an interaction only
/// Flutter knows about is in progress; finished jobs then stay in their slots.
/// `documents.poll_jobs`, then `refresh_structure_designer_auto` once if
/// `active_changed`, then conversion.
#[frb(sync)] pub fn poll_node_jobs(defer_installs: bool) -> APINodeJobPoll;
/// The CLI's `run` / the AI HTTP server: `run_node_job_blocking` on the node
/// named `node_identifier` in the active network, then a refresh. Replaces
/// `run_chemisorb_node`.
#[frb(sync)] pub fn run_node_job_by_name(node_identifier: String) -> Result<String, String>;

pub struct APINodeJobPoll {
    pub running: Vec<APINodeJobStatus>,
    pub finished: Vec<APINodeJobOutcome>,
    /// Finished jobs held back by D11 (either side's interaction). Non-zero
    /// keeps Dart's poll timer alive, so a deferred result is not stranded.
    pub pending_installs: u32,
    /// True when an install touched the active document — Dart then runs
    /// `refreshFromKernel()`.
    pub active_changed: bool,
}
/// A job's node is identified by document + network + scope + node id:
/// node ids are unique only within one network, so without `network_name` a
/// job would be attributed to the same-id node of whatever network is shown.
pub struct APINodeJobStatus {
    pub job_id: u64,
    pub document_id: u64,
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub label: String,
    pub phase: String,
    pub done: u64,
    pub total: Option<u64>,
    pub cancelling: bool,
}
pub struct APINodeJobOutcome {
    pub job_id: u64,
    pub document_id: u64,
    pub network_name: String,
    pub node_id: u64,
    pub label: String,
    pub kind: APINodeJobOutcomeKind,   // Finished | Cancelled | Failed | Dropped
    pub message: String,               // summary, error, or why it was dropped
}
```

The `chemisorb`-specific FFI goes: `run_chemisorb_node` is replaced by
`run_node_job_by_name` in Phase 3 (`lib/ai_assistant/http_server.dart`
updated); `run_chemisorb` (the panel's, together with `APIChemisorbRunResult`
and `model.runChemisorb`) is replaced by `start_node_job` in Phase 4, when its
only caller goes. `chemisorb_api.rs` then keeps only the panel's read-side
calls.

### Flutter

**Model** (`structure_designer_model.dart`) — state and FFI calls only, no
timer:

- `final ValueNotifier<List<APINodeJobStatus>> nodeJobs` — a dedicated
  notifier, **not** `notifyListeners()`: a 10 Hz progress tick must rebuild the
  three widgets that show progress, not the whole editor. Same idiom as
  `refreshProfile`.
- `startNodeJob(BigInt nodeId)` → `start_node_job` with
  `propertyEditorScopeChain`; returns the error message, if any. On success
  it calls `VoidCallback? onNodeJobStarted`, which the host sets to its
  poller's `start`, and appends a placeholder status (empty label and
  phase) to `nodeJobs` so the panel shows Cancel on the next frame rather
  than after the first poll — otherwise a double click reaches D7's
  refusal. It must not poll instead: a poll can carry outcomes, and only
  the host's poller reports those.
- `cancelNodeJob(BigInt jobId)`.
- `APINodeJobPoll pollNodeJobs(bool deferInstalls)` — calls `poll_node_jobs`,
  updates `nodeJobs.value`, runs `refreshFromKernel()` if `activeChanged`, and
  returns the poll so the caller can report outcomes.
- `jobFor(scopeChain, nodeId)` — `findNodeJob(nodeJobs.value, <the model's
  active document id>, <its active network name>, scopeChain, nodeId)`:
  matches the **active** document id
  **and the active network name**, then scope and node id. Matching on the
  document alone would put a job's badge and Cancel button on the same-id node
  of another network in that document.

**`lib/structure_designer/node_jobs.dart`** — the widget-free parts, so they
are unit-tested in `test/` (see "Testing strategy"):

- `findNodeJob(...)` — the pure lookup above.
- `NodeJobPoller` — the timer and its stop rule, over plain callbacks
  (`poll(bool defer) → APINodeJobPoll`, `interactionOpen() → bool`,
  `onOutcome(APINodeJobOutcome)`). `start()` begins `Timer.periodic(100 ms)`
  if it is not already running; it stops only when a poll returns no running
  jobs, no outcomes **and `pendingInstalls == 0`** — a result deferred by D11
  keeps it alive.

**Host** (`structure_designer.dart`) — owns a `NodeJobPoller`, beside
`_dependencyPoll`, because the D11 guard is its state:

- The library-refresh guard (pointer counter, focused text field,
  `ModalRoute.isCurrent`, wire drag — today inline around l.151–158) becomes
  one method, `_flutterInteractionOpen()`, used by both polls.
- The poller is wired as `poll: model.pollNodeJobs`,
  `interactionOpen: _flutterInteractionOpen`, `onOutcome:` the snackbars
  below; `model.onNodeJobStarted = poller.start`. The host cancels the
  poller in `dispose`.
- Outcomes are reported here because the host owns a `ScaffoldMessenger`
  that outlives the property panel. Finished → `showTransientSnackBar` with
  the summary's first line; Failed → `showErrorSnackBarOn`; Cancelled →
  transient "Search cancelled."; Dropped → transient, with the reason. The
  panel cannot show these itself: the user may have selected another node or
  tab long before the search ends.

**Property panel** (`chemisorb_editor.dart`, `_RunRow`), via
`ValueListenableBuilder` on `model.nodeJobs`. `_RunRow` itself takes the
job status (or none) and `onRun` / `onCancel` callbacks, not the model:

- Idle: today's Run button and status text.
- Running: a `LinearProgressIndicator` (determinate once `total` is known,
  indeterminate during planning), the text "Relaxing 37 / 121 (31 %)", and a
  **Cancel** button in place of Run. Cancelling: the text "Cancelling…" and a
  disabled button.
- The placard, its `endOfFrame` yield and the navigator capture are deleted.
- The settings stay editable during a run (D3). Changing a search setting
  shows the stale line as soon as the result lands; that is correct and needs
  no special wording.

**Node in the network editor** (`node_widget.dart`): while `jobFor` returns a
status, the title bar shows a 14 px `CircularProgressIndicator` (determinate
when `total` is known) with a tooltip "Chemisorption search — 31 %". Scoped
lookup via the node's `scopeChain`, like every other per-node lookup; `jobFor`
also requires the job's network to be the one displayed, so a job never badges
the same-id node of another network.

**Document tab** (`document_tabs.dart`): a tab whose document has a running job
shows a small spinner. This is the only cue for a search running in a parked
document.

A global "jobs" list (status bar / viewport corner) is **not** part of this
design: with Run limited to the top level of a network, the node, the panel and
the tab cover every place a running search can be.

## Interactions & edge cases

| Situation | Behaviour |
|---|---|
| User edits inputs or search settings during the run | Allowed. The result installs and shows stale. (D3) |
| Node deleted during the run | Install finds no node → `Dropped`. |
| Node deleted, then undone, during the run | Same id, same type → installs; the fingerprint decides whether it shows. |
| A different `chemisorb` node later gets the same id | Installs into it; can only miss (D3). |
| Undo / redo during the run | Unaffected; Run is not an undo step. |
| Tab switch during the run | Job continues; installs into the parked designer; tab shows a spinner. |
| Document closed | Its jobs are cancelled (`drop_parked`); when the worker returns, the outcome is `Dropped("the document was closed")`, even if the work itself succeeded. |
| Load / New in place | Fresh `DocumentId`; jobs cancelled (`renumber_active`); outcome as for a close. |
| Network became read-only (library refresh) | Install refused → `Dropped` with the reason. |
| Drag or other open interaction when the job succeeds | Install deferred to the next poll (D11); the poll keeps running while `pending_installs > 0`. |
| Typing in a property field, a dialog or menu open, a wire or camera drag when the job succeeds | Flutter passes `defer_installs = true`; install deferred until the interaction ends (D11). |
| User views another network of the same document during the run | No badge or panel state on that network's nodes (`jobFor` matches the network); the tab spinner still shows. |
| Run pressed again on the same node | Refused while a job runs or is cancelling (D7). |
| Cancel pressed just as the search succeeds | The result is valid and installs (`Finished`). |
| CLI `run` on the same node while a GUI job runs | Allowed (the blocking path does not consult the runner). The CLI stores its result; the GUI job's later install overwrites it. Both are fingerprint-keyed, so the display stays correct. |
| Search fails (unknown tag, invalid config, relaxation error) | `Failed` with the kernel's message; nothing stored. |
| Panic inside the search | Caught on the worker → `Failed("internal error: …")`; the app keeps running. |
| App exits mid-run | Pool threads die with the process; nothing is persisted. |

## Adding the next run-on-demand node

A checklist. It is the reason this design is a layer rather than a
`chemisorb` patch:

1. Follow the `chemisorb` shape (`nodes/AGENTS.md`, "Run-on-demand search"):
   `eval` never does the expensive part; the result is stored on the node data
   `#[serde(skip)]` with an **input fingerprint**; `eval` outputs it only on a
   match; `inherit_runtime_state` carries it through settings edits.
2. Implement `NodeData::prepare_job`: evaluate the inputs through `JobInputs`
   and return owned data in a `JobWork`.
3. Implement `JobWork::run` over that data, and make the domain loop report to
   the `JobControl`: `set_total` once the amount of work is known,
   `advance` per unit, `is_cancelled` at the same granularity.
4. Implement `JobResult::install`: downcast, store, return a summary.
5. The panel's action button uses `model.startNodeJob` and the shared running
   state. The node badge, tab spinner, snackbars, cancel and the CLI's `run`
   (`run_node_job_blocking`) come for free.

Nothing in the runner, `DocumentSet`'s job operations, the API, the poll loop
or the node widget is `chemisorb`-specific.

## Relation to other designs

- **`doc/design_node_execution.md` (Execute).** Still synchronous with a
  placard. Background Execute would reuse `JobControl`, `JobRunner`, the poll
  API and the UI surfaces unchanged, but its `run` half evaluates the network
  itself, so it needs a cloned `NodeTypeRegistry` on the worker, and that needs
  `NodeData: Send + Sync`. Deliberately not designed here.
- **`doc/design_background_evaluation.md`.** Untouched and still
  unimplemented. That design moves *implicit* evaluation off the UI thread and
  needs a lock; this one never shares state, so it needs none. Its Phase 5
  progress/cancel hook should adopt `JobControl` rather than add a second
  mechanism, and its busy-badge UI should reuse this design's node badge.

## Testing strategy

The design's claims are mostly about *threads and timing*: a result lands
whenever the worker finishes, a cancel takes effect "soon", a poll defers.
Tests of such claims go flaky unless the test, not the scheduler, decides
when things happen. Rules for every test below:

- **No sleeps for correctness, no "large enough" fixtures.** The worker side
  is driven by `GatedWork`, a test-only `JobWork`
  (`crates/atomcad-structure-designer/tests/structure_designer/node_jobs_support.rs`): it sets
  `total` / `done` / `phase` on its control as told, then blocks on a channel
  until the test releases it with an outcome (`Ok(GatedResult)`, `Err(msg)`,
  or a panic), returning `Err("cancelled")` early if the control is
  cancelled while it waits. `GatedResult::install` accepts any `NodeData` and
  records that it ran. Runner, routing, deferral, cancel and outcome tests
  therefore need no chemistry and finish in milliseconds.
- **Every wait is bounded.** A helper `wait_until(|| cond, 10 s)` polls
  (`thread::yield_now` between checks) and fails with a message instead of
  hanging the suite. `JobRunner`s in tests use 1–2 threads (the suite runs
  under `cargo test -j 4`; many runners with all cores each would
  oversubscribe the machine).
- **Real chemistry only where the claim is about chemistry:** `search` with
  a control (Phase 1) and prepare → run → install on a real `chemisorb` node
  (Phase 2), on the small fixtures `chemisorption_test.rs` and
  `chemisorb_node_test.rs` already have.
- **Mid-search cancellation is tested with a wide margin, not a tight
  race.** The search runs inside a 1-thread pool (`pool.install(|| search(…))`);
  a watcher thread waits until `snapshot().done >= 1` and cancels. With one
  worker the search advances one relaxation at a time — but the watcher is
  an OS thread the scheduler may delay, so the fixture's relaxations must be
  slow compared to that delay, not merely few. A small fixture is *not*
  enough: the 63-hypothesis hexapod over frozen silyls finishes in 40 ms in a
  debug build, inside one scheduler hiccup under `cargo test -j 4`. Phase 1
  uses the Si(100) ethanediyl fixture (23 relaxations of a ~350-atom slab),
  where the cancel lands at 1 of 23.
- **Flutter logic is pure Dart, tested in `test/`.** The poll loop and the
  job lookup are extracted from the widgets so they take plain callbacks and
  generated API data classes (`NodeJobPoller`, `findNodeJob`; Phase 4), like
  `DocumentSwitcher` in `test/document_switch_test.dart`. These need no Rust
  library, run in well under a second and are agent-runnable (unlike
  `integration_test/`). Timers are tested with `fakeAsync` /
  `tester.pump(duration)`, never real time.
- **What stays manual:** that the UI really stays responsive during a search
  (D4), the look of the badge / spinner / progress row, and the end-to-end
  feel — the Phase 4 walkthrough.

New test files are registered in their harness's `#[path]` list
(`crates/atomcad-structure-designer/tests/structure_designer.rs`,
`crates/atomcad-util/tests/util.rs`,
`crates/atomcad-crystolecule/tests/crystolecule.rs`,
`rust/tests/structure_designer_api.rs`). Run the API harness explicitly
(`cargo test --test structure_designer_api`), since `cargo test --workspace`
stops at the first failing harness.

## Implementation phases

Each phase is independently mergeable and leaves the application working.

### Phase 1 — `JobControl` and a cancellable, reporting search

1. `atomcad-util`: `job_control.rs` (`JobControl`, `JobProgress`).
2. `atomcad-crystolecule`: `control: Option<&JobControl>` on `evaluate` and
   `search`; phase/total/advance reporting; cancel check before each
   relaxation; `ChemisorptionError::Cancelled`. All existing callers pass
   `None`.
3. **Tests.**
   - `crates/atomcad-util/tests/util/job_control_test.rs`: a fresh control
     snapshots as `done 0, total None, phase ""`; `set_total(0)` stays
     indeterminate; `advance` from N threads × M calls sums to N·M; `cancel`
     is sticky.
   - `crates/atomcad-crystolecule/tests/crystolecule/chemisorption_test.rs`:
     - with a control: phase "Planning" then "Relaxing";
       `total == to_relax + 1` after planning; `done == total` at the end;
     - **the control changes nothing:** `search(None)` and
       `search(Some(&control))` give the same candidates in the same order
       with bit-identical energies (the existing tests, all passing `None`,
       guard today's behaviour);
     - cancelled before the call → `Cancelled`, `done == 0`;
     - cancelled mid-search (the recipe above) → `Cancelled`,
       `done < total`, no partial report.

### Phase 2 — the node-job layer, the runner, the session side, `chemisorb` on it

1. `node_jobs/` folder module: `mod.rs` (`JobTarget`, `JobWork`, `JobResult`,
   `JobStatus` / `JobOutcome` / `JobPoll`), `inputs.rs` (`JobInputs` with
   `context()`), `designer_ops.rs` (`prepare_node_job`, `install_job_result`,
   `run_node_job_blocking`); `NodeData::prepare_job` default; `rayon` added
   to the crate's `Cargo.toml`.
2. `chemisorb`: `prepare_job`, `ChemisorbWork` (`search` + `JobWork::run`),
   `ChemisorbOutcome`; `format_run_result` moved down; `run_chemisorb` kept as
   the typed test convenience over the same pieces (D9).
3. `node_jobs/runner.rs`: `JobRunner::new(threads)` owning a lazily built
   pool, `catch_unwind`, one job per target.
4. `node_jobs/document_ops.rs`: `DocumentSet.jobs`, `start_job`, `cancel_job`,
   `poll_jobs` (routing, both deferrals, outcome kinds); `cancel_document`
   called from `drop_parked` and `renumber_active` (D8).
5. **Tests** — `GatedWork`, `GatedResult` and `wait_until` in
   `tests/structure_designer/node_jobs_support.rs`; runner and routing tests
   in `node_jobs_test.rs`; real-node tests in `chemisorb_node_test.rs`.
   - **`JobRunner`** (`GatedWork`, 1–2 threads):
     - a status mirrors the control (`done` / `total` / `phase`) while the
       job runs;
     - D7: a second `start` on the same target is refused (with the generic
       message), on another target
       accepted; after `cancel` the status says `cancelling` and the target
       stays refused; once the worker has returned `Err("cancelled")` and
       the entry is taken, the target is free again;
     - `take_finished` honours the deferral predicate, and
       `pending_installs` counts what it left behind;
     - a panicking work becomes `Err("internal error …")`, and the next job
       on the same runner still runs;
     - the pool is not built before the first `start`;
     - D4: code inside a job sees `rayon::current_num_threads()` equal to
       the runner's thread count, so a nested `par_iter` uses the job pool.
   - **`DocumentSet::poll_jobs` routing** (`GatedWork` started directly on
     `documents.jobs`, targeting a real node):
     - an install into the active designer sets `active_changed`; one into a
       parked designer does not, and activating that document afterwards
       outputs the result (`swap_in`'s full refresh);
     - closing the document cancels the job (`drop_parked`), and its late
       outcome is `Dropped("the document was closed")` — both when the work
       then returns `Err` and when it returns `Ok`;
       `load_in_place` and `new_project_in_place` cancel it
       (`renumber_active`);
     - D11: a Rust-side open interaction (a node drag begun through the
       existing API) and `defer_installs = true` each hold a successful
       result — `pending_installs == 1`, no outcome — and the next
       undeferred poll installs it; a failed job is reported at once even
       under `defer_installs = true`;
     - outcome kinds: `Err` after `cancel` → Cancelled (whatever the text);
       `Err` without cancel → Failed; `Ok` after `cancel` → installed,
       Finished;
     - every outcome is delivered by exactly one poll;
     - a status carries the target's `network_name`; jobs on the same node
       id in two networks coexist and route to their own nodes.
   - **Real `chemisorb` node:**
     - D9: the existing tests stay green unchanged; `run_node_job_blocking`
       gives the same outputs and summary text as `run_chemisorb`, and
       refuses a node without a job ("no run action");
     - prepare refuses a body node, a non-job node and a read-only network
       with today's messages;
     - **D3, the design's correctness argument**, each as prepare → edit →
       run → install → evaluate: input pose moved → installs, outputs stale;
       `top_n` changed → stale; `top_n` changed and the edit undone → the
       result **is** shown (the fingerprint matches again and
       `inherit_runtime_state` carried it); node deleted → `Dropped`; node
       deleted and the deletion undone → installs and is shown; network made
       read-only before install → `Dropped` with the reason;
     - **D10:** an install is not an undo step (history length unchanged;
       the next undo still undoes the user's last edit), does not mark the
       document dirty, and is absent from the saved `.cnnd` (save, reload:
       no stored result, the node outputs the plan).

### Phase 3 — API

1. `node_jobs_api.rs`: `start_node_job`, `cancel_node_job`, `poll_node_jobs`
   (one refresh when `active_changed`), `run_node_job_by_name` and the API
   types — conversion only; module added to `flutter_rust_bridge.yaml`'s
   `rust_input`; `flutter_rust_bridge_codegen generate`.
2. Replace `run_chemisorb_node` with `run_node_job_by_name` and switch
   `lib/ai_assistant/http_server.dart` (the CLI `run`) to it. The panel's
   `run_chemisorb` FFI stays until Phase 4 replaces its only caller, so the
   app keeps working between the phases.
3. **Tests** (`rust/tests/structure_designer_api/node_jobs_api_test.rs`):
   converting a `JobPoll` carries every field (`total: None` included) and
   maps each outcome kind; `run_node_job_by_name` on a `chemisorb` node gives
   the same text `run_chemisorb_node` gave (the CLI's output does not
   change), and fails on an unknown name and on a node without a job.
   Routing is Phase 2's and is not re-tested here; the `#[frb(sync)]`
   functions themselves touch `CAD_INSTANCE` and stay untested, as in every
   other API module.

### Phase 4 — Flutter

1. Model: `nodeJobs` notifier, `startNodeJob` / `cancelNodeJob` /
   `pollNodeJobs` / network-aware `jobFor`; no timer. Delete
   `model.runChemisorb`, and in Rust the `run_chemisorb` FFI and
   `APIChemisorbRunResult` (`chemisorb_api_test.rs` moves off it); regenerate
   the bindings.
2. `lib/structure_designer/node_jobs.dart`: `NodeJobPoller` — the 100 ms
   timer and its stop rule, taking `poll(bool defer)`, `interactionOpen()`
   and `onOutcome` callbacks — and `findNodeJob(statuses, documentId,
   networkName, scopeChain, nodeId)`, which `model.jobFor` calls. Host
   (`structure_designer.dart`): factor the dependency poll's guard into
   `_flutterInteractionOpen()`; own a `NodeJobPoller` beside
   `_dependencyPoll`; outcome snackbars.
3. `_RunRow`: progress bar, percentage, Cancel; delete the placard. It takes
   the job status and `onRun` / `onCancel` callbacks rather than the model,
   so a widget test can pump it.
4. Node title-bar badge; document-tab spinner.
5. Docs: `doc/reference_guide/nodes/atomic.md` (`chemisorb`: Run runs in the
   background, progress, Cancel, editing during a run, the stale line);
   `lib/structure_designer/node_data/AGENTS.md` (`chemisorb_editor.dart` entry:
   no placard, the job state); `lib/structure_designer/AGENTS.md` (a short
   "Node jobs" section: the poll loop in the host and why, `node_jobs.dart`
   kept widget-free for `test/node_jobs_test.dart`, the shared interaction
   guard, the dedicated notifier, the outcome snackbars);
   `rust/crates/atomcad-structure-designer/src/nodes/AGENTS.md`
   ("Run-on-demand search": the job hook as part of the shape to copy);
   `src/AGENTS.md` directory map (the `node_jobs/` module; `DocumentSet`
   owns the jobs and cancels them in `drop_parked` / `renumber_active`).
6. **Tests** (`test/node_jobs_test.dart`; agent-runnable, no Rust library —
   the API data classes are constructed directly):
   - `NodeJobPoller` under `fakeAsync`: no poll before `start`; a poll every
     100 ms after; `interactionOpen()`'s value is passed as `defer`; polling
     continues while jobs run, while outcomes arrive, and **while
     `pendingInstalls > 0` with nothing running** (the stranded-result
     case); it stops on the first fully idle poll; `start` after a stop
     resumes; each outcome reaches `onOutcome` once.
   - `findNodeJob`: matches document + network + scope + node id; a job on
     the same node id in another network or another document is not found.
   - `_RunRow` widget test: idle shows Run; running with `total: null` shows
     an indeterminate bar; with a total, "Relaxing 37 / 121 (31 %)" and
     Cancel; cancelling shows "Cancelling…" and a disabled button; the
     callbacks fire.
7. `dart format`, `flutter analyze`, `flutter test test/`. **Manual
   walkthrough** (the maintainer's; agents do not run the Flutter smoke
   test):
   - Run a search of a few hundred hypotheses: the viewport orbits, the
     progress bar and node badge advance, the snackbar reports the result,
     the outputs update.
   - Cancel mid-run: "Cancelling…", then "Search cancelled.", outputs
     unchanged, Run available again.
   - Edit an input pose mid-run: the result lands stale.
   - Change `top_n` (or a filter) mid-run: the result lands stale, and the
     panel asks for a new Run.
   - Start a run, switch tabs: the tab spinner shows; switch back after it
     finishes: the result is there.
   - Start a run, delete the node: "the node no longer exists".
   - Run two `chemisorb` nodes at once: both progress, both land.
   - Let a search finish while typing into a property field (and, separately,
     while dragging a wire): nothing rebuilds under the interaction; the
     result lands as soon as it ends.
   - Start a run, open another network of the same document: no node there
     shows the badge or a Cancel button; the tab spinner shows.
   - While a search runs, drag a slider on an expensive node and edit
     geometry: the UI stays responsive (the D4 check).

## Open questions

1. **Cancellation inside a relaxation.** D6 checks per relaxation. If a single
   relaxation on realistic inputs takes more than ~1 s, add an optional stop
   check to `minimize_with_force_field` (a `&dyn Fn() -> bool` argument or a
   field on `MinimizationConfig`) and pass `is_cancelled` through. **Measured
   in Phase 1:** ethanediyl over the Si(100)-2×1 test slab (352 atoms, 2000
   iterations max, all converged), one thread, release build — **34 ms per
   relaxation**. Far below the threshold; not needed unless a realistic input
   is an order of magnitude larger (the limit is 2000 free atoms).
2. **Job pool size and priority.** `cores − 1` leaves one core for the UI
   thread but the UI's own rayon work still competes with the job pool.
   Lowering the job threads' OS priority would help further; it is
   platform-specific and is left until the Phase 4 walkthrough shows a need.
3. **Concurrency limit.** Concurrent jobs share the pool and each one slows
   the others proportionally. No limit is proposed; revisit if users start
   many at once.
