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
stores it on its node data) gets all of the above by implementing two small
traits.

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
  `plan.hypotheses.par_iter()` on the **global** rayon pool, collects in plan
  order and ranks. The number of relaxations is known once `plan` returns
  (`plan.stats.to_relax`, plus one for the reference). The data crossing
  rayon's threads (`AtomicStructure`, `ChemisorptionSearch`, `Candidate`) is
  already `Send + Sync` — that is what `par_iter` requires.
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
- **Flutter polls** a sync `poll_node_jobs` at ~10 Hz while any job exists.
  Finished results are installed during that poll.

## Decisions

**D1 — Explicit jobs only.** A node job is started by a user action on one
node and produces a result that is installed on that node. Evaluation-time
work (`relax`) is not a job; Execute is not a job (yet — see "Relation to
other designs").

**D2 — Snapshot in, result out; the worker never sees the global.** This is
what removes the need for a lock and for Send+Sync node data. The rule the
compiler enforces for us: everything a job's `run` half owns must be `Send`,
and nothing reachable from `CAD_INSTANCE` is.

**D3 — Edits during a run are allowed and need no handling.** The fingerprint
(`input_fingerprint`) covers every input and every *search* setting; listing
filters are excluded on purpose, so changing `top_n` while a search runs simply
re-lists the result when it lands. A run-on-demand node that adopts this
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

**D5 — Poll, don't push.** Dart polls a sync `poll_node_jobs()` on a
`Timer.periodic` (100 ms) that exists only while at least one job is known.
This keeps the existing pull model (`refreshFromKernel`, `takePrintLog`), needs
no `StreamSink` (the codebase has none), and puts install on the UI thread for
free. A poll is a few atomic loads; its cost is negligible. Push can replace it
later without changing the Rust side's job model.

**D6 — Progress and cancel granularity: one relaxation.** `done/total` counts
relaxations (`total = to_relax + 1` once the plan exists; indeterminate
before). Cancel is checked before each relaxation starts. Latency of a cancel
is therefore at most one in-flight relaxation per pool thread — typically well
under a second. A per-iteration check inside the minimizer is a follow-up only
if measurement shows long single relaxations (open question 1).

**D7 — One job per node; any number of nodes.** Starting Run on a node that
already has a running job is refused (`"A search is already running on this
node"`); the panel shows **Cancel** in that state anyway. Jobs on different
nodes — in the same or in different documents — run concurrently and share the
job pool.

**D8 — Jobs are session state, addressed by document.** The job list lives on
`CADInstance` (not on a `StructureDesigner`), because a job outlives a tab
switch. A job's target is `(DocumentId, network name, scope path, node id)`.
Install goes to the active designer or to a parked one
(`DocumentSet::parked_mut`). Closing a document cancels its jobs;
`load_in_place` / `new_project_in_place` mint a fresh `DocumentId`, so a job
started before them can never install into the replacement content (and is
cancelled as well, to save the CPU).

**D9 — The synchronous Run is the same job, run inline.**
`StructureDesigner::run_chemisorb` becomes `prepare → run (on the calling
thread, no control) → install`. The CLI's `run`, the AI HTTP server and the
tests keep a blocking path, and there is exactly one implementation of what a
search *is*.

**D10 — Job state is never node data.** Running, progress and outcome are not
saved, not undoable, and not part of the `.cnnd`. Only the installed result
lands on the node, under the rules it already has (`#[serde(skip)]`,
`inherit_runtime_state`).

**D11 — Install waits for open interactions.** If
`StructureDesigner::open_interaction` reports an interaction in progress
(a node, atom, gadget or property drag; a body resize; a comment edit), a
finished job stays in its slot and the next poll tries again. Installing marks
node data changed and refreshes; doing that in the middle of a coalesced drag
would refresh under the drag's `skip_downstream` assumptions. Polls arrive at
10 Hz, so the deferral is invisible.

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
  top of the `par_iter` closure, returning
  `Err(ChemisorptionError::Cancelled)`; `collect::<Result<…>>` short-circuits
  the remaining work. It calls `advance(1)` after each finished relaxation.
- New variant `ChemisorptionError::Cancelled` (`"search cancelled"`).
- `None` keeps today's behaviour byte for byte — every existing caller and test
  passes `None`. Cancellation never produces a partial report.
- `plan` is not made cancellable: it is enumeration only and bounded by
  `budget`. It is, however, run on the worker (in `run`, not `prepare`), so a
  large enumeration does not stall the UI either.

### Node-job layer — `atomcad-structure-designer`

New module `node_jobs.rs`:

```rust
/// Where a job's result goes. Identity is by document + scope + id; a target
/// that no longer resolves (or resolves to another node type) drops the
/// result. Thanks to D3 a wrong-but-same-type node can only miss.
#[derive(Clone, Debug, PartialEq)]
pub struct JobTarget {
    pub document_id: DocumentId,
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
}

/// The worker half: owned inputs, no access to the designer.
pub trait JobWork: Send + 'static {
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

`JobInputs` is a short-lived struct that `StructureDesigner` builds inside
`with_eval_context` and that borrows the evaluator, network stack, registry,
evaluation context and node id. It exposes exactly what `run_chemisorb` uses
today: `eval_input(pin)`, `eval_input_required(pin)` and
`use_vdw_cutoff()`. Inputs are evaluated **during prepare, on the UI thread** —
it is fast, it is what reads the network, and it is the only part that must.

**`StructureDesigner` entry points:**

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
```

The guards in `prepare_node_job` are today's `run_chemisorb` guards, made
generic: `ensure_active_editable`, top-level scope only (a body node's inputs
depend on per-iteration values), node exists. `install_job_result` repeats the
editability check, because a network can become read-only while a search runs
(a library refresh); the result is then dropped with that reason.

**`chemisorb` adapter** (`chemisorb_ops.rs`):

- `ChemisorbData::prepare_job` evaluates the three pins, builds the config with
  `search_config`, the `transfer_rules` and the fingerprint, and returns a
  `ChemisorbWork { adsorbate, substrate, config, fingerprint, listing }`.
- `ChemisorbWork::run` calls `search(…, control)` and returns a
  `ChemisorbOutcome { stored: StoredSearch, summary: ChemisorbRunSummary }`.
  The summary is computed **on the worker** from the job's own `listing`
  snapshot, so it is reported even if the result later misses.
- `ChemisorbOutcome::install` downcasts to `ChemisorbData`, sets `stored`, and
  returns `format_run_result`'s text (moved down from the api layer to sit
  beside `ChemisorbRunSummary`).
- `StructureDesigner::run_chemisorb` keeps its signature and becomes
  prepare → `run(None)` → install (D9).

### Job runner — `atomcad-structure-designer`

New module `job_runner.rs`, independent of the global so it is testable:

```rust
pub struct JobRunner {
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
    Finished(Result<Box<dyn JobResult>, String>),   // Err includes "cancelled"
}

impl JobRunner {
    /// Refuses a second job on the same target (D7). Spawns `work.run` on the
    /// job pool; the closure writes its outcome into the slot. A panic inside
    /// `run` is caught (`catch_unwind`) and becomes `Err("internal error …")`.
    pub fn start(&mut self, target: JobTarget, label: String,
                 work: Box<dyn JobWork>) -> Result<u64, String>;
    pub fn cancel(&self, id: u64);
    pub fn cancel_document(&self, document_id: DocumentId);
    pub fn statuses(&self) -> Vec<JobStatus>;          // running jobs only
    /// Removes and returns finished jobs, leaving those whose target is
    /// `deferred` (D11) in place.
    pub fn take_finished(&mut self, deferred: impl Fn(&JobTarget) -> bool)
        -> Vec<FinishedJob>;
}

static JOB_POOL: LazyLock<rayon::ThreadPool> = /* max(1, cores − 1) threads,
                                                 thread name "atomcad-job-N" */;
```

`JOB_POOL.spawn(...)` runs the job on a pool thread, so the `par_iter` inside
`evaluate` uses the same pool (D4). A cancelled job is removed only when its
worker has actually returned (`Finished(Err("cancelled"))`) — the panel shows
"Cancelling…" in between, and a new Run on the node stays refused until then,
so two searches on one node never overlap.

### API layer — `rust/src/api/structure_designer/node_jobs_api.rs`

`CADInstance` gains `pub jobs: JobRunner`.

```rust
#[frb(sync)] pub fn start_node_job(scope_path: Vec<u64>, node_id: u64) -> Result<u64, String>;
#[frb(sync)] pub fn cancel_node_job(job_id: u64);
#[frb(sync)] pub fn poll_node_jobs() -> APINodeJobPoll;

pub struct APINodeJobPoll {
    pub running: Vec<APINodeJobStatus>,
    pub finished: Vec<APINodeJobOutcome>,
    /// True when an install touched the active document — Dart then runs
    /// `refreshFromKernel()`.
    pub active_changed: bool,
}
pub struct APINodeJobStatus {
    pub job_id: u64,
    pub document_id: u64,
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
    pub node_id: u64,
    pub label: String,
    pub kind: APINodeJobOutcomeKind,   // Finished | Cancelled | Failed | Dropped
    pub message: String,               // summary, error, or why it was dropped
}
```

`poll_node_jobs` is the install site:

1. `take_finished`, deferring targets whose designer reports an open
   interaction (D11).
2. For each finished job: route by `document_id` to
   `cad_instance.structure_designer` (active) or `documents.parked_mut(id)`;
   unknown document → `Dropped("the document was closed")`. Call
   `install_job_result`; a missing node → `Dropped("the node no longer
   exists")`.
3. If anything was installed into the active designer,
   `refresh_structure_designer_auto` once (not per job).
4. Return running statuses and outcomes. Outcomes are delivered **once**.

A result installed into a *parked* designer only marks the node data changed;
the refresh that activation already performs evaluates it. (Phase 3 must
verify that activation re-evaluates changed node data in the parked designer,
and add a refresh there if it does not.)

The document-closing and in-place-replacing wrappers (`close_document`,
`load_node_networks`, `new_project*`) call `jobs.cancel_document(id)` first.

The existing `run_chemisorb` / `run_chemisorb_node` sync FFI stay for the CLI
and the AI HTTP server (D9). The panel stops using `run_chemisorb`; it is
removed from Dart use, and kept in Rust only if the CLI still needs it.

### Flutter

**Model** (`structure_designer_model.dart`):

- `final ValueNotifier<List<APINodeJobStatus>> nodeJobs` — a dedicated
  notifier, **not** `notifyListeners()`: a 10 Hz progress tick must rebuild the
  three widgets that show progress, not the whole editor. Same idiom as
  `refreshProfile`.
- `startNodeJob(BigInt nodeId)` → `start_node_job` with
  `propertyEditorScopeChain`; starts the poll timer; returns the error
  message, if any.
- `cancelNodeJob(BigInt jobId)`.
- `jobFor(scopeChain, nodeId)` — matches against the **active** document id.
- The poll timer: `Timer.periodic(100 ms)`, created on the first job, cancelled
  when a poll returns no running jobs and no outcomes. Each tick: update
  `nodeJobs.value`; if `activeChanged`, `refreshFromKernel()`; forward
  outcomes to `onNodeJobOutcome`.
- `void Function(APINodeJobOutcome)? onNodeJobOutcome` — registered by the host
  (`structure_designer.dart`), which owns a `ScaffoldMessenger` that outlives
  the property panel. Finished → `showTransientSnackBar` with the summary's
  first line; Failed → `showErrorSnackBarOn`; Cancelled → transient
  "Search cancelled."; Dropped → transient, with the reason. The panel cannot
  show these itself: the user may have selected another node or tab long
  before the search ends.

**Property panel** (`chemisorb_editor.dart`, `_RunRow`), via
`ValueListenableBuilder` on `model.nodeJobs`:

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
lookup via the node's `scopeChain`, like every other per-node lookup.

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
| User edits listing filters during the run | Allowed. The result installs and is listed with the new filters. |
| Node deleted during the run | Install finds no node → `Dropped`. |
| Node deleted, then undone, during the run | Same id, same type → installs; the fingerprint decides whether it shows. |
| A different `chemisorb` node later gets the same id | Installs into it; can only miss (D3). |
| Undo / redo during the run | Unaffected; Run is not an undo step. |
| Tab switch during the run | Job continues; installs into the parked designer; tab shows a spinner. |
| Document closed | Its jobs are cancelled; a result that lands anyway is dropped. |
| Load / New in place | Fresh `DocumentId`; jobs cancelled. |
| Network became read-only (library refresh) | Install refused → `Dropped` with the reason. |
| Drag or other open interaction when the job finishes | Install deferred to the next poll (D11). |
| Run pressed again on the same node | Refused while a job runs or is cancelling (D7). |
| CLI `run` on the same node while a GUI job runs | The CLI runs synchronously and stores its result; the GUI job's later install overwrites it. Both are fingerprint-keyed, so the display stays correct. |
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
   state. The node badge, tab spinner, snackbars and cancel come for free.

Nothing in the runner, the API, the poll loop or the node widget is
`chemisorb`-specific.

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

## Implementation phases

Each phase is independently mergeable and leaves the application working.

### Phase 1 — `JobControl` and a cancellable, reporting search

1. `atomcad-util`: `job_control.rs` (`JobControl`, `JobProgress`).
2. `atomcad-crystolecule`: `control: Option<&JobControl>` on `evaluate` and
   `search`; phase/total/advance reporting; cancel check before each
   relaxation; `ChemisorptionError::Cancelled`. All existing callers pass
   `None`.
3. **Tests** (`crates/atomcad-util/tests/`, `chemisorption_test.rs`):
   `JobControl` snapshot semantics; a search with a control reports
   `total == to_relax + 1` and ends at `done == total`; a control cancelled
   before the search returns `Cancelled` with zero relaxations; a control
   cancelled from another thread mid-search returns `Cancelled` (use a fixture
   large enough to have work left); `None` gives a report identical to today's
   (same candidates, same order).

### Phase 2 — the node-job layer and `chemisorb` on it

1. `node_jobs.rs` (`JobTarget`, `JobWork`, `JobResult`, `JobInputs`),
   `NodeData::prepare_job` default, `StructureDesigner::prepare_node_job` /
   `install_job_result`.
2. `chemisorb`: `prepare_job`, `ChemisorbWork`, `ChemisorbOutcome`;
   `format_run_result` moved down; `run_chemisorb` rewritten as prepare → run
   inline → install (D9).
3. `job_runner.rs` (`JobRunner`, `JOB_POOL`, `catch_unwind`, one job per
   target).
4. **Tests** (`tests/structure_designer/node_jobs_test.rs`): the existing
   `chemisorb_node_test.rs` stays green unchanged (D9 regression guard);
   prepare refuses a body node, a non-`chemisorb` node and a read-only network
   with today's messages; prepare + run + install produces the same output as
   the sync Run; a job whose inputs were edited before install installs and
   evaluates as stale; install into a deleted node is dropped; `JobRunner`
   with a test-only `JobWork` (sleeps in small cancellable increments,
   registered only in tests) — refuses a second job on one target, cancel ends
   it with `cancelled`, `take_finished` honours the deferral predicate, a
   panicking work becomes `Failed`.

### Phase 3 — API

1. `CADInstance.jobs`; `node_jobs_api.rs` with `start_node_job`,
   `cancel_node_job`, `poll_node_jobs` and the API types; module added to
   `flutter_rust_bridge.yaml`'s `rust_input`; `flutter_rust_bridge_codegen
   generate`.
2. Routing of installs to active / parked designers; single refresh per poll;
   cancel on close / load / new.
3. Verify (and if needed add) re-evaluation of changed node data on tab
   activation.
4. **Tests** (`rust/tests/structure_designer_api/`): the routing logic as an
   `#[frb(ignore)]` function taking an explicit `JobRunner` +
   `StructureDesigner` + `DocumentSet` (the `chemisorb_api` /
   `documents_api` pattern), covering active install, parked install, closed
   document → dropped, deferred install during an open interaction.

### Phase 4 — Flutter

1. Model: `nodeJobs` notifier, poll timer, `startNodeJob` / `cancelNodeJob` /
   `jobFor`, `onNodeJobOutcome`; host registers the snackbar handler.
2. `_RunRow`: progress bar, percentage, Cancel; delete the placard.
3. Node title-bar badge; document-tab spinner.
4. Docs: `doc/reference_guide/nodes/atomic.md` (`chemisorb`: Run runs in the
   background, progress, Cancel, editing during a run, the stale line);
   `lib/structure_designer/node_data/AGENTS.md` (`chemisorb_editor.dart` entry:
   no placard, the job state); `lib/structure_designer/AGENTS.md` (a short
   "Node jobs" section: the poll loop, the dedicated notifier, the outcome
   handler); `rust/crates/atomcad-structure-designer/src/nodes/AGENTS.md`
   ("Run-on-demand search": the job hook as part of the shape to copy);
   `src/AGENTS.md` directory map (`node_jobs.rs`, `job_runner.rs`).
5. `dart format`, `flutter analyze`. **Manual walkthrough** (the maintainer's;
   agents do not run the Flutter smoke test):
   - Run a search of a few hundred hypotheses: the viewport orbits, the
     progress bar and node badge advance, the snackbar reports the result,
     the outputs update.
   - Cancel mid-run: "Cancelling…", then "Search cancelled.", outputs
     unchanged, Run available again.
   - Edit an input pose mid-run: the result lands stale.
   - Change `top_n` mid-run: the result lands listed with the new value.
   - Start a run, switch tabs: the tab spinner shows; switch back after it
     finishes: the result is there.
   - Start a run, delete the node: "the node no longer exists".
   - Run two `chemisorb` nodes at once: both progress, both land.
   - While a search runs, drag a slider on an expensive node and edit
     geometry: the UI stays responsive (the D4 check).

## Open questions

1. **Cancellation inside a relaxation.** D6 checks per relaxation. If a single
   relaxation on realistic inputs takes more than ~1 s, add an optional stop
   check to `minimize_with_force_field` (a `&dyn Fn() -> bool` argument or a
   field on `MinimizationConfig`) and pass `is_cancelled` through. Measure in
   Phase 1 before deciding.
2. **Job pool size and priority.** `cores − 1` leaves one core for the UI
   thread but the UI's own rayon work still competes with the job pool.
   Lowering the job threads' OS priority would help further; it is
   platform-specific and is left until the Phase 4 walkthrough shows a need.
3. **Concurrency limit.** Concurrent jobs share the pool and each one slows
   the others proportionally. No limit is proposed; revisit if users start
   many at once.
