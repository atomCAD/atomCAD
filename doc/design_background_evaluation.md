# Background Network Evaluation Design

## Overview

Today every node-network evaluation runs synchronously on the Flutter UI
thread, inside a `#[frb(sync)]` FFI call. A slow node — `relax` on a large
molecule is the canonical case — freezes the entire application (input,
animations, *and* the 3D viewport, which is rendered by a per-frame sync FFI
call) for as long as the evaluation takes.

This document designs a **background evaluation** architecture: mutations
commit synchronously on the UI thread as they do today, but expensive
evaluation passes run on a dedicated worker thread against a snapshot of the
network state. The UI stays live, shows the last-good scene plus a busy
indicator, and gains progress reporting and cancellation.

The design supersedes the "Why not async (worker thread) FFI" section of
`doc/design_node_execution.md`, which rejected worker threads *given the
codebase as it was* (unsynchronized `static mut CAD_INSTANCE`) and explicitly
sketched the prerequisite adopted here: a real `Mutex` around the global plus
`try_lock` in `provide_texture`.

**Goals**

1. A long-running evaluation must not block input handling or viewport
   rendering.
2. Data-race freedom must be **compiler-enforced**, not discipline-enforced.
   After Phase 2 there is no `unsafe` on the state-access path; the type
   system proves that no unsynchronized sharing exists.
3. Progress reporting and user-initiated cancellation for long evaluations
   (starting with the UFF minimizer).
4. Fast edits stay as snappy as today — no added latency, flicker, or
   "stale" badge churn for the common case.

**Non-goals (follow-up work, not this design)**

- Parallel evaluation of independent nodes (rayon inside one pass). The
  worker runs one evaluation pass at a time, exactly as the evaluator runs
  today.
- Memoization of expensive node results (worth doing — `relax` currently
  re-runs once per displayed downstream cone per pass because the evaluator
  does not memoize pin results — but it is an orthogonal change).
- Background `execute_node` / CLI runs (listed as follow-ups in Phase 6).
- Web/wasm threading (see Risks).

## Current Architecture (facts, with references)

- `pub static mut CAD_INSTANCE: Option<CADInstance>` (`api/api_common.rs:286`)
  holds `StructureDesigner` + `Renderer` in one struct. The four accessors
  `with_[mut_]cad_instance[_or]` (`api_common.rs:309–422`) are raw
  `addr_of!`/`addr_of_mut!` derefs behind `unsafe` — **zero synchronization**.
  (Their doc comments claiming thread-safety are wrong and must be deleted.)
  ~402 call sites, all in `rust/src/api/`; one direct access in
  `initialize_cad_instance_async` (`common_api.rs:110`).
- The entire real API surface (404 functions) is `#[frb(sync)]`: every call
  executes on the Dart UI thread. Safety today rests entirely on that.
- Every mutator (~215 functions) ends with `refresh_structure_designer_auto`
  (`api_common.rs:484`), which runs **evaluate → tessellate → GPU upload**
  synchronously before the FFI call returns:
  `structure_designer.refresh(changes)` → `tessellate_scene_content(...)` →
  `renderer.update_all_gpu_meshes(...)` (+ background mesh). Undo/redo goes
  through the same tail.
- `RefreshMode` (`structure_designer_changes.rs`): `Lightweight` (no eval —
  gadget/selection/camera), `Partial` (changed cones only; `skip_downstream`
  during interactive drags), `Full`.
- The viewport renders via `provide_texture` (`common_api.rs:164`,
  `#[frb(sync)]`), called from a persistent frame callback in
  `lib/common/cad_viewport.dart` — also under `with_mut_cad_instance`. This
  is why a modal dialog does not make worker threads safe today: the frame
  callback fires regardless of dialogs.
- Evaluation (`evaluator/network_evaluator.rs`) is **read-only over the
  network**: `generate_scene` per displayed node, writes only to the per-pass
  `NetworkEvaluationContext` (node errors, output strings, eval caches,
  print buffer) and produces `NodeSceneData` stored in
  `StructureDesignerScene.node_data`. Nodes are never structurally mutated
  during evaluation.
- Getters copy owned data across FFI (`NodeNetworkView` etc.) — nothing
  borrows across the boundary.
- The `relax` node is a pure function of its input + one preference flag. The
  minimizer (`crystolecule/simulation/minimize.rs`) is a plain loop
  (≤500 L-BFGS iterations, ≤2000 free atoms) with **no yield, progress, or
  cancellation**.
- There is no Rust→Dart push channel (zero `StreamSink` uses); progress
  today is poll-based (`take_print_log`, `get_relax_message`) and cannot work
  during an evaluation because evaluation blocks the polling thread.
- Already thread-friendly: `UndoCommand: Send + Sync` (JSON snapshots),
  wgpu resources are `Send + Sync`, `tokio`/`rayon` are dependencies, FRB's
  worker pool is compiled in, and the evaluator deliberately kept `Arc` over
  `Rc` "for forward-compat with multi-threaded evaluation"
  (`iterator_walker.rs:127–131`).

## Design Principles

1. **One choke point.** All ~215 mutators funnel through
   `refresh_structure_designer_auto`. The sync/async split happens inside
   that one function (and its callee). No per-mutator rewrites, no Dart-side
   API-signature changes.
2. **Compiler-enforced safety.** Replace the `static mut` with a `Mutex` and
   delete the `unsafe` accessors. From then on, any attempt to share
   non-thread-safe state across threads is a compile error — including in
   future code written by someone unaware of this design.
3. **Never hold the lock across an evaluation.** The worker evaluates a
   *snapshot* (clone) of the network state. The lock is held only for
   microsecond-scale commits and a short merge. This is what keeps
   `provide_texture` and the sync getters live during a background pass.
4. **The Dart pull model is unchanged.** `refreshFromKernel()` and the ~130
   sync getters stay exactly as they are. Background completion is signaled
   by a single new `StreamSink` event that triggers the existing
   `refreshFromKernel()` + `scheduleFrame()`.
5. **Interactive paths stay synchronous.** `Lightweight` refreshes and
   `skip_downstream` drag refreshes never go to the worker. Fast `Partial`/
   `Full` passes also run inline (see the heuristic) so the common case has
   zero added latency.
6. **Stale results are discarded, never merged.** A generation counter makes
   "evaluation result for a network the user has since edited" a
   non-event: the merge is skipped and a newer job is already queued.
7. **At most one running job + one pending job.** Rapid edits coalesce; the
   pending slot is overwritten, and superseded running jobs are cancelled.

## Locking Model

One coarse lock:

```rust
// api/api_common.rs
pub struct CADInstance {
    pub structure_designer: StructureDesigner,
    pub renderer: Renderer,
}

static CAD_INSTANCE: Mutex<Option<CADInstance>> = Mutex::new(None);

pub fn with_mut_cad_instance<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut CADInstance) -> R,
{
    CAD_INSTANCE.lock().unwrap().as_mut().map(f)
}
// ... same shape for the other three helpers; all four lose `unsafe`.
```

- All 402 call sites keep their exact shape (minus the `unsafe` blocks, which
  become no-ops to remove mechanically).
- `Mutex<Option<CADInstance>>` as a `static` requires `CADInstance: Send` —
  and moving snapshots/results to the worker requires `Send + Sync`
  transitively (every `Arc<T>` needs `T: Send + Sync` to cross threads).
  This is why Send/Sync-ification (Phase 1) precedes the lock swap.
- **`provide_texture` uses `try_lock` and skips the frame on contention**
  (returns the previous frame's timing; the Flutter `Texture` widget keeps
  showing the last pushed buffer). Contention windows are the short commit
  and merge sections only, so skipped frames are rare and invisible.
- **Deadlock rule (the one invariant the compiler cannot check):** the
  CAD lock is never held while (a) evaluating, (b) blocking on a channel, or
  (c) calling out through FFI (the texture-plugin callback). With a single
  lock and this rule there is no lock ordering to get wrong. The renderer's
  internal `render_mutex` (`renderer.rs:102`) is fine — it is a leaf lock,
  only ever taken while already holding the CAD lock, never the reverse.
- `RwLock` is a deliberate non-choice for now: sync API calls hold the lock
  for microseconds, so reader concurrency buys nothing, and `RwLock` adds
  writer-starvation and poisoning subtleties. Revisit only with profiling
  evidence.

## The Refresh Split

`refresh_structure_designer_auto` becomes a scheduling decision instead of an
unconditional inline pass:

```rust
pub fn refresh_structure_designer_auto(cad_instance: &mut CADInstance) {
    let changes = cad_instance.structure_designer.get_pending_changes();
    match eval_dispatch(&cad_instance.structure_designer, &changes) {
        Dispatch::Inline => refresh_structure_designer(cad_instance, &changes),
        Dispatch::Background => schedule_background_eval(cad_instance, changes),
    }
}
```

**Dispatch heuristic** (`Dispatch::Background` when either holds, else inline):

1. **Static heavy hint:** any node in the dirty evaluation cone has
   `NodeType::heavy == true`. Initially set for `relax` and any other node
   wrapping the minimizer or surface reconstruction. This catches the first
   evaluation of a heavy cone, before any duration has been measured.
2. **Measured duration:** the last completed evaluation pass for this network
   took longer than `BACKGROUND_EVAL_THRESHOLD_MS` (50 ms). The designer
   records per-network last-pass duration; unknown ⇒ 0 ⇒ inline.

`Lightweight` and `skip_downstream` passes are always inline (they are fast
by construction and interactive tools depend on their immediacy —
`atom_edit` drag coalescing, gadget updates).

Inline dispatch is byte-for-byte today's behavior. This means Phases 1–3
ship with zero user-visible change.

**Commit vs. evaluate.** Note what already happened *before* this function
runs: the mutation itself, validation, undo recording, dirty marking. All of
that stays synchronous under the lock — the authoritative model state is
always current. Only the derived artifacts (scene data, meshes) lag while a
background pass runs.

## Evaluation Snapshot

`schedule_background_eval` builds an `EvalSnapshot` under the lock (cheap,
clone-based) and hands it to the scheduler:

```rust
// structure_designer/background_eval/snapshot.rs
pub struct EvalSnapshot {
    pub registry: NodeTypeRegistry,       // clone — includes all node_networks
    pub active_network_name: Option<String>,
    pub displayed_nodes: Vec<(u64, NodeDisplayState)>,
    pub changes: StructureDesignerChanges,
    pub preferences: StructureDesignerPreferences,
    pub selected_node_id: Option<u64>,
    pub camera: Camera,                    // copy, for tessellation
    pub generation: u64,
}
```

- Networks live in `NodeTypeRegistry::node_networks`
  (`node_type_registry.rs:308`) and evaluation of the active network can
  reach any other network through custom-node instances, so the snapshot
  clones the **whole registry**. `NodeNetwork` is deep-cloneable today
  (`NodeData::clone_box`, used by the clipboard); zone bodies are `Arc` CoW,
  so body-heavy networks clone cheaply.
- Clone cost must be **measured** in Phase 3 on real projects. Networks are
  typically small (nodes + wires + parameters); the known heavy payloads are
  imported structures stored in node data (`import_xyz`/`import_cif`) and
  `atom_edit` diffs. If profiling shows a problem, the mitigation is to move
  those payloads behind `Arc` (immutable-after-construction data — cheap to
  share once everything is `Send + Sync`). Do not build the `Arc` machinery
  speculatively.
- The worker constructs its own `NetworkEvaluator` and
  `NetworkEvaluationContext` (same code path as
  `StructureDesigner::with_eval_context`), evaluates every entry in
  `displayed_nodes` exactly as `refresh_full`/`refresh_partial` do, and
  collects:

```rust
pub struct EvalOutcome {
    pub generation: u64,
    pub node_scene_data: Vec<(u64, NodeSceneData)>,
    pub gadget: Option<Box<dyn NodeNetworkGadget>>,
    pub unit_cell: Option<UnitCellStruct>,
    pub print_entries: Vec<PrintLogEntry>,
    pub duration: Duration,               // feeds the dispatch heuristic
    pub meshes: TessellatedMeshes,        // CPU meshes, built off-lock
}
```

- **Tessellation runs on the worker, outside the lock**, against the
  worker-side scene (previous retained `NodeSceneData` for unaffected nodes —
  see merge — plus the fresh results) and the snapshot camera. A slightly
  stale camera is acceptable: meshes are geometry, and the renderer applies
  the *current* camera every frame. (First implementation may tessellate on
  merge under the lock instead — see Phase 4 step 6 — and move off-lock as a
  fast follow.)

**Partial-pass subtlety.** `refresh_partial` retains `NodeSceneData` for
unaffected displayed nodes. The worker needs those retained entries to
tessellate a complete scene. `NodeSceneData` is not `Clone`
(`Box<dyn Any>` eval cache), so the snapshot cannot copy them. Resolution:
the snapshot carries only the *dirty* node set; at merge time the fresh
results are installed into the live scene (which still holds the retained
entries), and tessellation of the full scene happens at merge. If moving
tessellation off-lock later, make `NodeSceneData`'s heavy members
(`NodeOutput` meshes/atoms) `Arc`-shared so a scene view can be assembled
cheaply for the worker; that is an optimization, not required for
correctness.

## Scheduler: Generations, Coalescing, Cancellation

```rust
// api/background_eval.rs (owns the thread; api layer, since it locks the global)
pub struct EvalScheduler {
    request_tx: Sender<EvalRequest>,       // worker owns the rx
    running: Option<RunningJob>,           // generation + cancel flag
    pending: Option<EvalRequest>,          // coalesced: newest wins
}

pub struct RunningJob {
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
}
```

- `StructureDesigner.eval_generation: u64` increments on every mutation that
  reaches `refresh_structure_designer_auto` (inline or background — inline
  passes also stamp the scene, so a background result can never overwrite a
  newer inline result).
- **Schedule:** if a job is running, set its cancel flag and put the new
  request in `pending` (overwriting any older pending request). Otherwise
  start it immediately.
- **Completion:** the worker locks the CAD instance briefly and merges
  **iff `outcome.generation == structure_designer.eval_generation`**;
  otherwise the outcome is dropped on the floor. Then the scheduler starts
  `pending` if present.
- **Cancellation** is cooperative via `Arc<AtomicBool>`:
  - the evaluator checks it between displayed-node evaluations and inside
    walker `next()` loops (`NetworkEvaluationContext.cancel`);
  - the minimizer checks it once per L-BFGS iteration (see below).
  A cancelled job simply abandons — no partial merge, no new
  `NetworkResult` variant. The last-good scene stays; the newer queued job
  supersedes it.
- The worker is one dedicated `std::thread` spawned at `init_app` — not the
  FRB pool, not tokio. One thread, one job at a time, clear lifetime,
  trivially testable. (rayon inside a pass keeps working as today —
  `batched_implicit_evaluator` fans out and joins within the pass.)

### Minimizer cancellation/progress hook

`crystolecule` must not depend on `structure_designer`, so the hook is a
trait defined in the simulation layer:

```rust
// crystolecule/simulation/mod.rs
pub trait SimulationMonitor: Sync {
    fn should_cancel(&self) -> bool { false }
    fn report_progress(&self, iteration: u32, max_iterations: u32, rms_gradient: f64) {}
}

pub fn minimize_energy(
    atoms: &mut AtomicStructure,
    vdw_mode: VdwMode,
    monitor: Option<&dyn SimulationMonitor>,
) -> Result<RelaxResult, SimulationError>
```

`minimize_with_force_field` checks `should_cancel()` at the top of each
iteration and returns a `SimulationError::Cancelled`. The `relax` node
adapts the eval context's cancel flag + progress channel into a monitor.
Existing callers (`atom_edit` interactive minimization, tests) pass `None`.

## Result Delivery: StreamSink Events

One new **non-sync** API function, called once from Dart at startup:

```rust
// api/structure_designer/eval_events_api.rs
pub fn evaluation_events(sink: StreamSink<APIEvalEvent>) { /* store sink */ }

pub enum APIEvalEvent {
    Started { generation: u64 },
    Progress { generation: u64, message: String, fraction: Option<f32> },
    Finished { generation: u64 },
    Cancelled { generation: u64 },
    Failed { generation: u64, error: String },
}
```

Dart side (`StructureDesignerModel`):

- `Started` → set `isEvaluating = true`, `notifyListeners()` (busy badge).
- `Progress` → update a progress string/fraction (throttled Rust-side to
  ~10 Hz so the UI isn't spammed).
- `Finished` → `refreshFromKernel()` + `renderingNeeded()` — the existing
  pull path does everything else; `isEvaluating = false`.
- `Cancelled`/`Failed` → clear busy state, surface the error like existing
  node errors.

Because the UI thread is never blocked anymore, the busy indicator can be a
real animated spinner, and the static-placard workaround in
`design_node_execution.md` becomes unnecessary for evaluation (it remains
for `execute_node` until Phase 6).

### UI presentation of a pending evaluation

- The viewport keeps rendering the **last-good scene** (meshes unchanged
  until merge).
- The node editor shows the authoritative *model* state immediately (the
  mutation already committed): wires, positions, properties are current.
  Node **output**-derived decorations (output strings, eval errors) are
  stale until merge; nodes in the dirty cone get a subtle "evaluating"
  marker (`NodeView` gains an `evaluating: bool`, true when a background
  job's dirty set contains the node).
- A viewport-corner badge shows progress ("relax: iteration 240/500") with a
  cancel button → new sync API `cancel_evaluation()` → sets the running
  job's cancel flag.

## What Stays Synchronous (unchanged)

- All getters, selection, clipboard, navigation, camera ops.
- `Lightweight` refreshes and `skip_downstream` drag refreshes.
- Mutation commit + validation + undo recording (always, by design).
- Inline `Partial`/`Full` passes under the 50 ms heuristic — the common case.
- `run_cli_single`/`run_cli_batch` (headless: no UI to keep live; they call
  the evaluator directly and bypass the scheduler).
- `edit_atom` (deprecated) and `atom_edit` interactive tooling.
- Undo/redo: the state restoration is synchronous; the follow-up evaluation
  goes through the same dispatch as any mutation.

## Send/Sync-ification (Phase 1 detail)

Target invariant, enforced by compile-time asserts in
`rust/tests/structure_designer/send_sync_test.rs`:

```rust
fn assert_send_sync<T: Send + Sync>() {}
#[test]
fn domain_types_are_send_sync() {
    assert_send_sync::<StructureDesigner>();
    assert_send_sync::<NodeNetwork>();
    assert_send_sync::<NetworkResult>();
    assert_send_sync::<StructureDesignerScene>();
    assert_send_sync::<CADInstance>();
}
```

`Send + Sync` (not just `Send`) because `Arc<T>: Send` requires
`T: Send + Sync`, and `Arc` is pervasive (zone bodies, walkers, closures).

Known blockers (complete list from the audit; the compiler will confirm):

| Fix | Site |
|---|---|
| `trait NodeData: Any + AsAny + Send + Sync` | `node_data.rs:116` — also fixes `nodes/AGENTS.md`, which already (wrongly) documents this bound as existing |
| `trait NodeNetworkGadget: Gadget + Send + Sync` (and `Gadget`/`Tessellatable`) | `node_network_gadget.rs:4`, `display/gadget.rs:4` |
| `Box<dyn Any>` → `Box<dyn Any + Send + Sync>` | `NetworkEvaluationContext.selected_node_eval_cache`, `structure_designer_scene.rs:92` |
| `RefCell<Vec<(String, i16)>>` → `Mutex` | `nodes/materialize.rs:56`, `nodes/motif_sub.rs:33` |
| `RefCell<Option<CompatibilityReport>>` → `Mutex` | `nodes/patch_latticefill.rs:120` |
| `Cell<bool>` + `RefCell<Vec<…>>` → `AtomicBool` + `Mutex` | `nodes/atom_edit/atom_edit_gadget.rs:39–41` |
| `RefCell<Vec<VdwParams>>` + `Cell<u32>` → `Mutex` + `AtomicU32` | `crystolecule/simulation/uff/mod.rs:54–55` |
| Remove `#[allow(clippy::arc_with_non_send_sync)]` (now vestigial) | `network_evaluator.rs:107–118`, `iterator_walker.rs:127–131` |

Any `Box<dyn Fn…>` in walkers/closures gains `+ Send + Sync`. This phase is
compiler-driven: add the trait bounds and the asserts, then fix every error.
It is a behavior-preserving refactor — `Mutex`/atomics on these
uncontended-single-thread caches cost nothing measurable.

## Interactions & Edge Cases

- **Mutation during a running job:** commit succeeds immediately (lock is
  free), generation bumps, running job's cancel flag is set, request
  coalesces into `pending`. The authoritative state is never stale — only
  derived scene data is.
- **Undo of a not-yet-evaluated mutation:** same as any mutation — undo
  commits, generation bumps, in-flight job discarded on merge.
- **`get_relax_message` / eval-cache-dependent panels:** these read
  `NodeSceneData` eval caches, which update at merge; until then they show
  the previous value, consistent with the stale-scene principle.
- **`print_log`:** worker collects print entries in the outcome; merged into
  `StructureDesigner.print_log` under the lock, then drained by the existing
  `take_print_log` polling in `refreshFromKernel`. (Ordering across inline
  and background passes is by merge time, which matches user perception.)
- **Save while a job runs:** saving reads the authoritative model (networks),
  not the scene — always current. No interaction.
- **Load / new document:** bump generation (discards in-flight), cancel
  running job, then proceed; the post-load `Full` refresh dispatches
  normally.
- **Tests:** the entire `rust/tests` suite constructs `StructureDesigner`
  directly and never touches `CAD_INSTANCE` — unaffected. New scheduler
  tests drive `EvalScheduler` with a test-only registered `slow` node type
  (a `NodeData` whose `eval` sleeps in small cancellable increments).

## Risks & Open Questions

1. **Registry clone cost.** Unmeasured. Phase 3 adds a timing probe; if a
   large imported structure makes cloning slow, `Arc` the immutable payloads
   (post-Phase-1 this is trivial). Decision deferred to data — see
   `feedback_avoid_speculative_caching`.
2. **Merge duration under the lock.** Merge = install scene data (+
   tessellate + GPU upload in the first implementation). Tessellation of a
   huge scene can reach tens/hundreds of ms; during that window UI-thread
   API calls block (frames are safe — `provide_texture` skips via
   `try_lock`). Mitigation path if it matters: `Arc` the meshes inside
   `NodeSceneData` and tessellate off-lock (sketched above). Ship simple
   first, measure.
3. **Heuristic misses.** A cone that is fast 99 times and slow the 100th
   (e.g. parameter change explodes an array) will freeze once, then be
   background thereafter. Acceptable; the static `heavy` hint covers the
   known offenders.
4. **wasm/web target.** Threads on wasm need SharedArrayBuffer + cross-origin
   isolation, and FRB's threading story differs there. The scheduler must be
   `cfg`-gated to inline-always on wasm until investigated. (Native desktop
   is the target of this design.)
5. **Two long-eval entry points remain synchronous** until Phase 6:
   `execute_node` (has its placard workaround) and `capture_screenshot`.
6. **Lock poisoning.** A panicking evaluation on the worker must not poison
   the CAD lock: the worker evaluates *outside* the lock, and merge wraps in
   `catch_unwind` (or we accept `parking_lot::Mutex`, which does not poison,
   as a dependency — decide in Phase 2 review).

## Implementation Phases

Each phase is independently shippable; Phases 1–3 are zero-visible-change.

### Phase 1: Send + Sync domain types

1. Add `Send + Sync` bounds to `NodeData`, `NodeNetworkGadget`, `Gadget`,
   `Tessellatable`; widen the two `dyn Any` caches; convert the seven
   `RefCell`/`Cell` sites per the table above; bound walker/closure `dyn Fn`s.
2. Remove the vestigial `arc_with_non_send_sync` clippy allows; fix the
   `nodes/AGENTS.md` claim to match reality.
3. Add `send_sync_test.rs` compile-time asserts (the durable regression
   guard — any future `Rc`/`RefCell` in the domain fails this test).
4. **Test scope:** existing suite green (behavior-preserving); the new
   asserts.
5. **Manual smoke:** none needed — no behavior change.

### Phase 2: Real lock around the global

1. `static CAD_INSTANCE: Mutex<Option<CADInstance>>`; de-`unsafe` the four
   helpers (call sites shed their `unsafe` blocks mechanically — ~402 sites,
   mostly `sed`-shaped); rewrite `initialize_cad_instance_async`'s direct
   access; delete the false thread-safety doc comments.
2. `provide_texture`: `try_lock`, skip-frame on contention.
3. Decide std `Mutex` + `catch_unwind` merge vs `parking_lot`.
4. **Test scope:** existing suite; a threaded smoke test (spawn a thread
   doing `with_cad_instance` reads while the main thread mutates — must not
   deadlock or corrupt; this is now *safe code*).
5. **Manual smoke:** full app walkthrough (every subsystem crosses these
   helpers); verify no frame hitches from lock overhead (there will be none
   — uncontended locks are nanoseconds).

### Phase 3: Snapshot evaluation + generation counter (still inline)

1. `EvalSnapshot` construction (`structure_designer/background_eval/`);
   `eval_generation` on `StructureDesigner`; per-network duration recording.
2. A snapshot-driven evaluation function producing `EvalOutcome`, and a
   merge function — both run **inline on the UI thread** this phase, wired
   behind a debug flag.
3. Timing probe for snapshot clone cost; log on large projects.
4. **Test scope:** equivalence test — for a corpus of fixture networks,
   snapshot-eval + merge produces a scene identical to direct
   `refresh_full`/`refresh_partial` (compare `NodeSceneData` node sets,
   output strings, errors); generation-mismatch merge is a no-op.
5. **Manual smoke:** flag on: app behaves identically.

### Phase 4: The worker thread

1. `EvalScheduler` + dedicated worker thread (spawned in `init_app`),
   channels, coalescing, cancel-on-supersede.
2. Dispatch heuristic in `refresh_structure_designer_auto`
   (`NodeType::heavy` hint on `relax`; 50 ms measured threshold;
   `Lightweight`/`skip_downstream` always inline). `cfg`-gate to
   inline-always on wasm.
3. `evaluation_events` StreamSink API + `APIEvalEvent`; regenerate FRB
   bindings.
4. Dart: subscribe in model init; `Started`/`Finished`/`Failed` handling;
   `isEvaluating` busy badge; `Finished` → `refreshFromKernel()` +
   `renderingNeeded()`.
5. `NodeView.evaluating` marker for dirty-cone nodes.
6. Merge under the lock: install `NodeSceneData` + gadget + unit cell +
   print entries, tessellate, upload GPU meshes (first implementation
   on-lock; off-lock tessellation as fast-follow if profiling demands).
7. **Test scope:** scheduler unit tests with the test `slow` node —
   coalescing (N rapid edits ⇒ ≤2 evaluations), generation discard,
   supersede-cancel, merge correctness; a lock-liveness test (UI-thread-role
   thread acquires the lock in <1 ms while a background job runs).
8. **Manual smoke:** relax on a large molecule — UI stays interactive,
   viewport orbits during evaluation, busy badge shows, result appears on
   completion; rapid parameter scrubbing on a heavy cone coalesces instead
   of queueing.

### Phase 5: Cancellation + progress

1. `SimulationMonitor` trait; thread cancel/progress through
   `NetworkEvaluationContext` → relax's monitor adapter; minimizer checks
   per iteration; evaluator checks between nodes and in walker loops.
2. `Progress` events (throttled ~10 Hz): "relax: iteration i/max,
   RMS gradient g".
3. `cancel_evaluation()` sync API + cancel button on the busy badge.
4. **Test scope:** cancellation stops a long minimization promptly (test
   monitor counts iterations after cancel ≤ 1); cancelled jobs never merge;
   progress callbacks fire monotonically.
5. **Manual smoke:** start a heavy relax, cancel mid-way — scene stays
   last-good, no error spam; re-edit triggers a fresh evaluation.

### Phase 6: Follow-ups (separate efforts, unblocked by this design)

- Background `execute_node` (replaces the static-placard recipe in
  `design_node_execution.md`) and `capture_screenshot`.
- Off-lock tessellation via `Arc`-shared `NodeSceneData` meshes (if Phase 4
  profiling shows merge-lock pressure).
- `Arc`-shared heavy node payloads if Phase 3 shows snapshot-clone cost.
- Memoization of expensive node results (separate design doc — eliminates
  redundant relax re-runs entirely; complementary, not competing).
- Parallel evaluation of independent displayed cones on the worker (rayon) —
  becomes safe and easy once everything is `Send + Sync`.

## Effort Estimate

| Phase | Estimate |
|---|---|
| 1 — Send + Sync | 1–2 weeks (compiler-driven, mechanical) |
| 2 — Mutex | 2–3 days |
| 3 — Snapshot + generation | ~1 week |
| 4 — Worker + events + UI | 2–3 weeks |
| 5 — Cancel + progress | ~1 week |

Total: roughly 5–8 weeks of focused work, with a hard safety milestone
(no `unsafe` state access, compiler-verified) reachable in the first
~2 weeks.
