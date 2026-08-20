# Design: Refresh & Evaluation Profiling

A refresh of a real design can take seconds, and nothing in the
application says *where* the time went. This design adds a cheap,
always-on breakdown of a refresh into its phases, and an opt-in
per-node evaluation profiler that attributes self time by node and by
node type — and, in its final phase, measures **evaluation redundancy**.

Division of labour with `doc/design_eval_memoization.md`: that document
owns the *evaluation environment model* (what a node's result depends
on, and why); this one owns the *mechanism* that computes it and the
numbers it produces. It must not start until this document's Phase 3
has produced those numbers. `doc/design_background_evaluation.md` is
orthogonal — it changes where the time is spent, not how it is
attributed.

## Motivation

- **The redundancy is real.** A static replay of the evaluator's call
  structure over `SPM-tip-with-tool_2026-08-20_12-52-CET.cnnd`
  (network `0_pecursor_edit_sequence_H&Cl`, 61 nodes) — walking every
  displayed root's upstream cone, recursing into custom networks and
  resolving `parameter` nodes back through the caller's argument wires
  the way `nodes/parameter.rs` does — yields **482 node evaluations over
  ~77 distinct nodes**. Displaying one additional node (a
  `structure_move`) takes it to **845**: `materialize` 6 → 11
  evaluations, `atom_edit` 14 → 25, the whole `geo.1-precursor_proxy`
  subnetwork 6 → 11. That doubling is the reported symptom.
- **But a static count cannot produce a trustworthy ratio.** It
  over-counts lazy paths (`if_else` pulls only the taken branch —
  `if_else.rs:15`; iterators pull on demand) and under-counts
  higher-order functions (a body node was counted once, not once per
  element). Worse, a body node evaluated N times over N elements runs in
  **N different environments** — not redundancy at all. Distinct-
  environment counting is inherently dynamic. Phase 3 is the real
  measurement; the numbers above are directional only.
- **Evaluation may not even be the dominant phase.** Nobody has measured
  tessellation or the ~15 sync FFI calls `refreshFromKernel()` fires
  after every refresh. Concluding that the evaluator is the bottleneck
  without measuring would be exactly the speculative optimization the
  project rules forbid.

## Current state (analysis)

- **No timing infrastructure exists.** The only helper is
  `atomcad_util::timer::Timer` — a `Drop`-printing stopwatch — with ~20
  commented-out call sites, including one at the top of
  `refresh_structure_designer` (`rust/src/api/api_common.rs:444`) for
  exactly this purpose. `println!` to a detached console is not a usable
  answer for a GUI application.
- **The refresh pipeline has four cost centres**, three Rust-side and
  one Dart-side:
  1. Evaluation — `StructureDesigner::refresh_full` / `refresh_partial`
     (`structure_designer.rs:1363`, `:1443`), whose displayed-roots loop
     runs inside `with_eval_context` (`:307`).
  2. Tessellation — `scene_tessellator::tessellate_scene_content`
     (`api_common.rs:473`).
  3. GPU upload — `renderer.update_all_gpu_meshes` (`api_common.rs:487`)
     plus the background mesh rebuild below it.
  4. View building / FFI marshalling — the `refreshFromKernel()` block
     (`lib/structure_designer/structure_designer_model.dart:3259`),
     which includes a full `getNodeNetworkView()` rebuild.
- **Node evaluation has exactly two entry points.** `evaluate`
  (`network_evaluator.rs:2127`) and `evaluate_all_outputs` (`:1892`) are
  the only functions that dispatch to `NodeData::eval` (`:2237` and
  `:1956` respectively); across the crate `evaluate` has 7 call sites and
  `evaluate_all_outputs` 2 (`:733`, `:1985`). Neither delegates to the
  other, so instrumenting both double-counts nothing. Both are already
  recursive and already maintain a per-frame bracket (the re-entrancy
  guard), so they are the natural instrumentation point.
- **Frame fingerprinting already exists.** `eval_frame_key`
  (`network_evaluator.rs:246-272`) hashes the network stack for the
  re-entrancy guard, and its doc comment explains why `NodeRef` is not a
  frame identity. Phase 3 reuses that reasoning and that shape, but adds
  a **sibling** function rather than extending it: the guard's key is
  alive only while its frames are, whereas Phase 3's key is retained, and
  that difference changes what a frame may be identified by (D9). The
  environment reasoning is in `design_eval_memoization.md` §"The
  evaluation environment".
- **Existing per-pass plumbing to mirror.** `with_eval_context`
  (`structure_designer.rs:307`) constructs `NetworkEvaluationContext`,
  runs the pass, and **drains** `print_buffer` into
  `StructureDesigner.print_log` regardless of how the closure returned.
  **At most one** `with_eval_context` call runs per refresh — `:1400`
  for Full, `:1618` for Partial, and **none at all** for Lightweight
  (the other three callers, `:7944`, `:8055`, `:8118`, are CLI and
  Execute paths, not refreshes). So a refresh pass is one context —
  *almost*.
- **A pass is not one `NetworkEvaluationContext`.** The lazy walkers
  (`map`/`filter`) run bodies against the caller's context, but the
  **eager** HOFs do not: `apply` (`nodes/apply.rs:252`), `fold`
  (`nodes/fold.rs:129`) and `foreach` (`nodes/foreach.rs:131`) each build
  a body context with `fresh_inner_for_eager_body`
  (`network_evaluator.rs:406`) and merge it back with
  `drain_inner_context` (`:437`) — which today merges **`print_buffer`
  and nothing else**. Any per-pass state a new feature parks on the
  context is therefore silently dropped for eager-HOF bodies unless it is
  either kept off the context entirely or added to that drain. This
  single fact drives D4, D6 and D9 below; getting it wrong does not
  produce a missing row, it produces a *wrong* row (see D4).
- **Existing UI to mirror.** The Console panel
  (`lib/structure_designer/console_panel.dart`) is a bottom-docked strip
  whose state lives on `StructureDesignerModel`, toggled from a *View*
  menu entry and a shortcut. The Profiler panel is the same shape. There
  is currently **no status bar** in the shell
  (`structure_designer.dart`); Phase 1 adds one.

## Non-goals

- **Making anything faster.** This design measures; it does not
  optimize.
- **A sampling profiler or flame graph.** Node-boundary instrumentation
  answers "which node type, which node, how many times". Sub-node hot
  spots stay a job for an external profiler.
- **Renderer / GPU frame timing.** The per-frame `provide_texture` path
  is not part of a refresh; only the CPU-side mesh upload is timed.
- **Persisting measurements.** Reports live in memory for the session —
  nothing touches `.cnnd` or preferences, so no undo command and no
  file-format change.
- **A CLI surface.** The report type is not CLI-specific, but none is
  designed here.

## Design decisions

### D1. Two clocks with different lifetimes and different costs

**Phase timing is always on.** One `Instant::now()` per phase boundary —
a handful per refresh — is not measurable against a refresh measured in
milliseconds, and always-on
means a regression surfaces during ordinary work rather than only during
a deliberate profiling session.

**Per-node profiling is opt-in.** It adds a thread-local access, two
clock reads and a hash-map update per node evaluation — microseconds at
~10³ evaluations per pass, but not inside a `map` body over 10⁵ elements,
and a profiler that inflates the numbers it reports is worse than
useless. Switched off, the cost is one thread-local `bool` read per
evaluation and no guard at all (D4).

Two switches, not one. Phase timing has no off state.

### D2. The per-node toggle is a runtime flag, not a cargo feature

`flutter run` loads the **release** DLL, so a `debug_assertions`- or
feature-gated profiler would be invisible in the build the maintainer
actually runs, and rebuilding with a feature flag is a multi-minute
round trip on this machine.

The flag is session state on `StructureDesigner` (mirrored on
`StructureDesignerModel`), toggled from the *View* menu — the same
lifetime and plumbing as `consolePanelVisible`. Deliberately **not** a
persisted preference: it would silently stay on across sessions and skew
later measurements.

### D3. Instrument `evaluate` and `evaluate_all_outputs`

These two cover every evaluation path uniformly — the wire seam, the
displayed-root evaluation in `generate_scene_scoped` (`:733`), the
`parameter` default-pin path, capture pre-evaluation, and the
zone-output pull — and keep the hook count at two. A profiler that
silently omits categories of evaluation produces numbers that do not add
up to the phase total, which destroys trust in the whole table.

The memo uses the same two hooks (`design_eval_memoization.md` D7), so
the two features agree by construction on what counts as an evaluation.

### D4. Self time via a child-accumulator stack, released by an RAII guard

On entry: record `start`, push a zero child-accumulator. On exit:
`total = start.elapsed()`, `self = total − children_acc`, pop, add
`total` to the parent's accumulator. This is what makes **time spent
evaluating upstream dependencies not charged to the consumer**.

**The accumulator stack lives in a thread-local, not on
`NetworkEvaluationContext`.** Two independent reasons, either sufficient:

- *Borrowck.* A guard that held `&mut NetworkEvaluationContext` for the
  duration of the frame would freeze the one thing the whole function
  body needs — `context` is threaded into every recursive `evaluate`
  call. Interior mutability is unavoidable, and the profiler is not
  otherwise part of the context's data flow.
- *The eager-HOF context split.* An HOF's own guard frame is created
  while the outer context is current, but its body evaluations run
  against a `fresh_inner_for_eager_body` one (see "Current state"). With a
  context-owned stack the body's `total` would never reach the HOF's
  child accumulator, so `fold`/`foreach`/`apply` would be charged the
  **entire body cost as self time** — the precise misattribution this
  decision exists to prevent — and the body's own records would vanish
  from the report. A thread-local is per *pass*, which is the correct
  scope: it spans every context a pass constructs.

`with_eval_context` installs a fresh profiler into the thread-local and
takes it back at end of pass (D6), so the lifetime still has exactly one
owner and one seam.

The bookkeeping **must** be released by a guard with a `Drop` impl, not
by hand at each `return`. Both functions have many early exits — the
poison check, the cycle guard, the central Unit-skip rule, the
missing-node guard — and a leaked frame corrupts every ancestor's self
time silently.

That guard has to respect a constraint the `eval_in_progress` bracket
already documents. Its STACK-SIZE WARNING
(`network_evaluator.rs:2115`) says these functions "deliberately avoid
wrapper functions, labeled blocks, and large by-value temporaries"
because deep node chains run near the debug-build thread stack limit —
it is an argument *for* that bracket's manual cleanup, not for a guard,
and this design does not get to cite it as support. The guard is still
right for the profiler (an unbalanced pop is silent corruption rather
than a caught error), but it must be cheap enough not to violate the
warning:

- it is a plain struct of `Instant` + a `u32` record index — no
  borrows, no closure wrapper, no `Box`;
- it is **not constructed at all** when per-node profiling is off (one
  thread-local `bool` read at the top, then nothing), so the release
  build the maintainer runs pays no frame cost;
- the `tag_test` 33-node chain that guards the stack limit stays in the
  suite, and Phase 2 re-runs it in a debug build with profiling **on**.

Two attribution artifacts follow from evaluator semantics and must be
documented in the panel, not "fixed":

- **Lazy iterators shift time to the consumer.** A `map` body node runs
  when `collect` pulls it, so its time nests under `collect`; `map`'s own
  total looks near-zero.
- **A custom-node instance has ~zero self time.** It delegates to its
  network's return node (`:1985`), so its *total* covers the subnetwork
  while its *self* is bookkeeping only. That is the useful reading.

### D5. Node identity for aggregation is `(home frame, node_id)`

"Which node is this?" is a different question from "which evaluation
environment is this?" (D9). The answer is read off
`network_stack.last()`: the top frame always names the network the node
actually lives in, which is unambiguous under stack excursions and
per-network id collisions alike.

`NodeRef` is **not** used *as the key*. Mis-keying would only
mis-attribute time rather than corrupt results, but it would
mis-attribute it in exactly the confusing cases (a subnetwork's internals
vs. its caller's), which is where the profiler must be most trustworthy.

The top frame is identified by **D9's frame-identity rule**, not by its
raw address: a registry-owned network by its address (pinned for the
pass), a zone body by `(identity of the frame below it, owner node id)` —
the enclosing frame is needed because `node_id` counters are per-network,
so two HOFs in two different networks can own bodies with the same id.
Here the stakes are lower — a reused address merges or splits a table row
rather than corrupting a memo — but there is no reason to run two
identity rules, and the aggregation table is read as ground truth by the
Redundancy tab that *does* care.

One residual ambiguity is accepted rather than engineered away: a lazy
walker calls `run_closure_once` with an **empty** enclosing stack, so the
pair degenerates to `owner_node_id` alone — which `ZoneClosure` documents
as not unique across networks (`doc/design_closures.md`, §"`owner_node_id`:
the model's one conceptual debt"). Two lazily-driven bodies whose owners
share a node id in two different networks therefore merge into one table
row. This costs a cosmetically wrong row in a rare case; it does **not**
touch D9's key, where those frames are separated by their fresh epochs
regardless.

**But the key alone cannot be displayed or navigated to.** A network
address renders as nothing, and for a node inside a zone body the top
frame *is* the anonymous body — there is no network name to print in the
"By node" table and no scope path to hand to the click-to-jump
navigation, which is keyed by `NodeRef { scope_path, node_id }`. So each
record carries, alongside the key, a location captured **once on vacant
insert** (never on the hot update path, which would clone a `Vec` per
evaluation):

```rust
struct NodeLocation {
    root_network_name: String,   // network_stack.first()
    scope_path: Vec<u64>,        // context.eval_scope_path at first record
    node_id: u64,
    label: String,               // "net#id (custom name)" / "body of fold#12"
}
```

`scope_path` + `node_id` is exactly the `NodeRef` the Find Usages /
error-navigation code already consumes (`root_cause_navigation.dart`), so
D8b's click-to-jump needs no new navigation machinery. Key and location
answer different questions and both are needed; conflating them is what
this decision rejects.

Aggregation to node *type* is a roll-up of the same records — both
tables come from one map.

### D6. The API layer assembles the report; each layer times what it owns

`RefreshProfile` is built in `refresh_structure_designer`
(`api_common.rs:444`), the only place that sees all three Rust-side
phases:

```
RefreshProfile {
    mode: Full | Partial | Lightweight,
    // from StructureDesigner::refresh. `eval_ms` is None — not 0.0 — for a
    // Lightweight refresh, which runs no `with_eval_context` at all; the
    // panel and the strip must render that as "—", never as free evaluation.
    eval_ms: Option<f64>,
    scene_dependent_ms: f64,
    gadget_ms: f64,
    tessellate_ms: f64,
    gpu_upload_ms: f64,
    background_ms: Option<f64>,               // None on lightweight (skipped)
    total_ms: f64,
    node_stats: Option<EvalProfile>,          // Some only when profiling is on
}
```

`StructureDesigner::refresh` returns its own sub-phase timings rather
than reaching outward — the crate may not reference `api/`. The
`EvalProfile` is installed and taken back in `with_eval_context`, at the
same seam as the existing `print_buffer` drain and with the same
"regardless of how the closure returned" discipline.

It is taken out of the **thread-local** (D4), *not* out of
`NetworkEvaluationContext`. Draining it from the context would look
equally centralised and would still lose every eager-HOF body: those
bodies run against a `fresh_inner_for_eager_body` context whose
`drain_inner_context` merges only `print_buffer` (see "Current state").
Storing it off the context is what makes "no call site can forget it"
true rather than merely intended — a new eager body site cannot drop
what it never held. `with_eval_context` remains the single owner:

```rust
// with_eval_context, around the existing construct/run/drain
let profiling = self.profiling_enabled;
eval_profiler::install(profiling.then(EvalProfile::default));
let result = f(...);
let entries = std::mem::take(&mut context.print_buffer);
self.print_log.extend(entries);
let profile = eval_profiler::take();      // Option<EvalProfile>
```

The one piece of per-pass state that *does* stay on the context is the
`env_epoch` counter (D9), because the memo will read it on every
evaluation and a thread-local read per push is a worse trade than one
field. That makes it the one piece that must be threaded through the
eager-body split explicitly — see D9.

Reports live in a bounded ring on `StructureDesigner` (last ~20) and are
**read, not drained** — a profile is a snapshot the panel re-renders;
draining would empty the panel on every unrelated poll. Consecutive
`Lightweight` entries **coalesce into one row** (count + mean + max)
rather than each taking a slot: one gadget drag emits hundreds of ticks
and would otherwise flush the whole history of the interesting refreshes
before the user can open the panel.

### D7. Dart-side phases are timed in Dart

Rust cannot see the `refreshFromKernel()` block. A `Stopwatch` around it
captures marshalling plus view construction; because `#[frb(sync)]`
calls are synchronous, a stopwatch around the mutation call yields a
Dart-side total that brackets the Rust-side `total_ms`. **The gap
between the two is itself a measurement** — the FFI and serialization
overhead, which nothing else reports.

Widget rebuild and Flutter frame time are out of scope; Flutter devtools
cover them.

### D8. Presentation: strip, then panel, then canvas

Ordered by value per line of code.

**(a) Status strip — always on, Phase 1.** A ~20 px strip as the last
child of the shell `Column` in `structure_designer.dart`:

```
refresh 1.83 s — eval 1.61 · tess 0.15 · gpu 0.02 · view 0.05   (Partial)
refresh 0.04 s — eval —    · tess 0.02 · gpu 0.01 · view 0.01   (Lightweight)
```

Note the second line: a Lightweight refresh has no evaluation phase at
all (`eval_ms: None`, D6), and the strip renders that as `—`. Printing
`0.00` there would read as "evaluation is free", which is the single
most misleading thing this strip could say.

This alone answers "is it evaluation or something else?" without opening
anything. It is the highest-value element of this design.

**Its repaint must not ride on `notifyListeners()`.** `gadget_drag`
marks a lightweight refresh on **every pointer move**
(`structure_designer.rs:7240`), and this project already holds the rule
that drag ticks never notify the model (see the canvas drag-performance
work) — an always-on measurement widget that forces a model-wide rebuild
per drag tick would be a self-inflicted regression measured by itself.
So:

- the strip owns a dedicated `ValueNotifier<RefreshProfile>` on
  `StructureDesignerModel`; only the strip listens to it, and updating it
  never calls `notifyListeners()`;
- it is written unconditionally for Full/Partial refreshes, and for
  Lightweight ones **coalesced to at most ~5 Hz** (drop the tick if the
  last write was under 200 ms ago). Drag ticks are near-identical to each
  other; a strip that updates five times a second still reads as live.

The two coalescings are independent and must not be confused: the
Dart-side one above throttles *repaints* of the strip, while the ring
(D6) coalesces consecutive `Lightweight` **rows** so a drag cannot flush
the interesting history. A tick dropped by the strip is still counted in
the ring's coalesced row; the Phases tab therefore reports the full drag,
one row with a count, not the few ticks the strip happened to draw.

**(b) Profiler panel — Phases 2–3.** Bottom-docked, built on the
`console_panel.dart` template, with four tabs:

- **Phases** — last refresh plus the history ring, tagged with mode.
  Makes a 40 ms drag tick vs. a 1.8 s node activation visible at a
  glance.
- **By node type** — `Type | Evals | Self | Total | % self`, sorted by
  self time.
- **By node** — `Network#id (custom name) | Lookups | Evals | Self |
  Total | Wasted`, click-to-jump. Both the label and the jump target come
  from the record's `NodeLocation` (D5), not from its key; the jump
  itself reuses the scope-aware canvas navigation from Find Usages /
  error navigation (`root_cause_navigation.dart`).
- **Redundancy** — Phase 3 only. Totals (evaluations, distinct
  environments, factor) plus per-node offenders ranked by **Wasted**
  (D10). This tab is the memo's business case and, afterwards, its
  regression test.

**Column availability follows the phases.** `Lookups` and `Wasted` are
defined per *environment* (D10) and cannot be computed before the key
exists, so Phase 2 ships **By node** as `… | Evals | Self | Total` and
Phase 3 adds the two columns alongside its own tab. Phase 2 must not
show a `Lookups` column filled with `Evals`: they are equal only until
the memo lands, and a column that quietly changes meaning is how a
regression hides.

A **"Profile full refresh"** button belongs here: it forces
`mark_full_refresh` + a refresh with profiling on. Without it the panel
shows whatever partial refresh ran last, and successive measurements are
not comparable.

**(c) Canvas heat overlay — Phase 4, optional.** Tint each node by its
share of self time and append `11× · 240 ms` to the node subtitle. Nodes
already carry subtitles (`NodeData::get_subtitle`), so the plumbing is
short, and it turns a 61-node network's hot path into something visible
without reading a table.

### D9. Phase 3 — computing the environment key

**What a node's result depends on, and why the key below is sufficient,
is defined in `doc/design_eval_memoization.md` §"The evaluation
environment".** Not repeated here — a second copy would go stale. The
key it specifies is:

```
hash( for each frame: (frame identity, frame node_id, env_epoch),
      node_id, decorate )
```

This document owns the mechanism: what "frame identity" is, and where
`env_epoch` comes from.

**Frame identity is a network address only for registry-owned frames.**
`eval_frame_key` hashes `&NodeNetwork as *const _ as usize` for every
frame, and its doc comment justifies that with "A spurious collision
needs **two live frames** hashing identically". That argument holds for a
re-entrancy guard, whose keys exist only while the frames are on the
stack. It does **not** transfer to this key, which is *retained* — across
the pass in `distinct_envs` and D11's first-result map, and afterwards in
the memo table. A retained address can be reused by a later allocation
after the original is dropped, and both kinds of body network do get
dropped mid-pass: `zone_closure.rs:377` pushes a **locally constructed**
`body_network` that dies at the end of that call, and closure bodies are
`Arc`s that can drop when the last closure value goes away. Two
genuinely different environments hashing equal would understate
`distinct_envs` — inflating the reported redundancy, i.e. inflating this
document's own business case — and, once the memo keys on it, serve a
wrong result.

So identity is split by frame kind, and the body kind never hashes an
address:

| frame kind | pushed at | identity | `env_epoch` |
|---|---|---|---|
| root / custom-network instance | registry-owned network | network address | 0 |
| zone body — **invocation** | `zone_closure.rs:504` (`run_closure_once`) | owner node id | fresh |
| zone body — capture pre-eval | `zone_closure.rs:149`, `:377` | owner node id | 0 |
| zone body — scene descent | `network_evaluator.rs:707` | owner node id | 0 |

Root and instance frames borrow their network immutably from the
registry, which outlives the pass, so their addresses are pinned and
stable by construction — the one case where the address is both safe and
the only available identity. A body frame needs no address: it is fully
determined by the enclosing frames plus the id of the node that owns the
body, because networks are immutable during a pass and a synthesized body
is a pure function of its owner node. Dropping the address from body
frames removes the reuse hazard outright, rather than mitigating it.

**`env_epoch` is fresh only at the invocation push**, which is the whole
point of the field:

```rust
// NetworkStackElement
pub env_epoch: u64,        // 0 unless this frame is one closure invocation

// NetworkEvaluationContext
next_env_epoch: u64,       // starts at 1
pub fn alloc_env_epoch(&mut self) -> u64 {
    let e = self.next_env_epoch; self.next_env_epoch += 1; e
}
```

An invocation is exactly the unit at which the zone-input frame and the
captures `Arc` change — both are installed inside `run_closure_once`
(`zone_closure.rs:465`) and both are bracketed by the body push it
rebuilds on every call (`:501`). A `map` over three elements yields
epochs 7, 8, 9 on otherwise identical stacks, so its body reports
**redundancy 1.0, not 3×**.

The other three body pushes deliberately keep epoch 0, because they are
not invocations:

- **Capture pre-evaluation** (`:149`, `:377`) runs *before* any captures
  exist — the push is there so `source_scope_depth` walks resolve in the
  parent scope. Stamping a fresh epoch would mark every capture cone as a
  brand-new environment, pinning capture redundancy at 1.0 forever and
  making those cones permanently uncacheable by the memo.
- **The scene descent** (`:707`) happens once per displayed root, not per
  element. A fresh epoch there would make the same displayed body node
  look like a different environment under each root, hiding exactly the
  cross-root redundancy the memo exists to collect.

Where this key errs, it errs toward **under**-reporting redundancy: a
lazy walker calls `run_closure_once` with `network_stack == &[]`, so its
body frames carry no enclosing context and are separated by their fresh
epochs even when two invocations genuinely share an environment. That
direction is the safe one for both consumers — a conservative number here
never becomes an unsound cache hit there — and the Redundancy tab says so
in a footnote rather than pretending the ratio is exact.

**The counter must survive the eager-body context split.** It lives on
`NetworkEvaluationContext` (D6), and `fresh_inner_for_eager_body` /
`drain_inner_context` currently carry nothing but `print_buffer` — so
without an explicit change every `fold`/`foreach`/`apply` body would
restart at epoch 1 and hand out epochs already in use, producing
different environments with equal keys. Phase 3 therefore extends both
helpers: carry `next_env_epoch` **in**, and merge it **back** as
`self.next_env_epoch = self.next_env_epoch.max(inner.next_env_epoch)`,
with a debug assertion that the merged value never decreases. This is one
line each and belongs in the same change as the field.

**`eval_frame_key` itself is not touched.** The new function is a sibling
next to it, not an extension of it: they answer different questions under
different lifetime rules, and merging them would re-import the address
hazard the table above removes.

**Phase 3 computes this key and counts collisions. It does not act on
them.** Within this document a wrong key produces a wrong number, never a
wrong result. But the memo inherits this key verbatim, and there a wrong
key *is* a wrong result — which is why it is built and validated here,
under D11, where a mistake is only ever a bad row in a table.

Two rules make this survive the memo landing on top of it:

- **`env_epoch` is allocated unconditionally**, not behind the
  per-node-profiling toggle (D1/D2). It is one increment per closure
  invocation, against a push that already builds a stack and a captures
  map — and the memo needs it in every pass, including passes with
  profiling off. The same applies to the carry/merge through
  `fresh_inner_for_eager_body` and `drain_inner_context`: unconditional,
  since a counter that is only correct while profiling is on is a trap
  for the memo.
- **Key computation is gated in Phase 3** (it is O(stack depth) per
  evaluation) and becomes unconditional when the memo lands, because the
  memo consults it on every evaluation. It therefore lives in the
  evaluator as an ordinary function next to `eval_frame_key` — **not**
  inside the profiler module — so the memo can call it without depending
  on the profiler being compiled in or switched on.

### D10. Redundancy metrics and the "Wasted" column

The vocabulary is chosen so it stays meaningful after the memo lands and
starts satisfying requests without evaluating. Per node record:

- `lookups` — times a result for this (environment, node) was requested.
- `evaluations` — times `eval` actually ran. Before the memo,
  `evaluations == lookups`; afterwards the difference is memo hits.
- `distinct_envs` — number of distinct keys seen.
- `self_ns` / `total_ns` — per D4, over actual evaluations.
- **`wasted_ns = self_ns × (lookups − distinct_envs) / evaluations`** —
  the self time a perfect memo would avoid. This is the actionable
  column: the projected saving, in milliseconds, per node.

Do **not** define the redundancy factor over `lookups` alone. Post-memo,
lookups stay high while evaluations collapse; a factor computed from
lookups would read as unchanged and the memo's acceptance criterion
could never pass.

Report the factor **per node, never only globally.** A pass that is
globally 2.5× but 11× on `materialize` and 1.0× on body nodes is the
realistic shape, and only the breakdown says where a memo would pay.

Nodes the memo deliberately declines to cache — iterator producers
(`design_eval_memoization.md` D4) and results produced under the
re-entrancy backstop (D9 there) — are counted but flagged, so
`wasted_ns` is never read as an available saving.

### D11. The equal-key ⇒ equal-result self-check

An opt-in, `debug_assertions`-gated mode (a second toggle, off by
default, genuinely expensive) that retains each key's first result and
asserts later results under the same key are equal. This is how the key
model gets *empirically* validated rather than argued: it would have
caught the `decorate` omission, the `NodeRef` collision, and an
address-reuse collision of the kind D9's frame-identity table removes —
on a real design, with no risk of serving a wrong value.

`NetworkResult` equality is not universally cheap or even defined for
every variant; the check compares `to_display_string()` plus, where
available, atom/element counts. A weak check that runs beats a perfect
one that does not.

**The check only means anything with the memo disabled.** Once the memo
serves the second request from the first result, there is no second
computation to compare and the check passes vacuously. Enabling it must
therefore force the memo off for that pass, and the panel must say so —
otherwise a green result after the memo lands is evidence of nothing.

### D12. What is deliberately not attributed

Time inside the CSG conversion cache is charged to the node that
triggered it; `NetworkEvaluator::get_csg_cache_stats` already exists and
its hit/miss counts are shown next to the phase totals rather than
folded into node time. The invisible-node LRU
(`structure_designer_scene.rs:186`) spares *display* work, not
evaluation — a restore-from-cache refresh correctly shows a near-zero
eval phase.

## Phases

**Testing conventions, applying to every phase below.** Wall-clock
assertions are flaky and are **not** written — not "assert eval took
< 200 ms", not "assert self time is under half". What is asserted is
*structure*: counts, identities, orderings, and relations between two
recorded numbers. Rust tests live in
`rust/crates/atomcad-structure-designer/tests/structure_designer/`.
Flutter smoke tests are **not** run by agents (see `AGENTS.md`), so every
phase's UI verification is a manual walkthrough for the maintainer, and a
phase is not done until that list has been run.

### Phase 1 — Refresh phase timing + status strip (D6, D7, D8a)

Always-on timers at the four cost centres, `RefreshProfile`, the FFI
getter, the ring, the status strip. **No evaluator changes.** Answers "is
evaluation the bottleneck?" before any evaluator code is touched — and if
the answer is "no", Phases 2–3 may not be worth building at all.

#### Tests

- A refresh in each mode produces a `RefreshProfile` tagged with that
  mode. A `Lightweight` refresh records **no evaluation sub-phase at
  all** — it never enters `with_eval_context` (the other three callers,
  `structure_designer.rs:7944/8055/8118`, are CLI and Execute paths, not
  refreshes) — so `eval_ms` is `None` rather than a small number, and the
  panel can't misreport a drag tick as cheap evaluation.
- Additivity: the sub-phase values sum to no more than `total_ms`, and no
  sub-phase is negative. A relation between recorded numbers, not a
  threshold — stable on any machine.
- Ring behaviour (D6): 25 successive refreshes leave the last 20; a burst
  of `Lightweight` ticks collapses to a single coalesced row carrying the
  burst count, and does **not** evict the `Full`/`Partial` rows around
  it.
- The FFI getter **reads without draining**: two consecutive calls with
  no refresh in between return the same profile.

#### Manual walkthrough

- Open a heavy design (`SPM-tip-with-tool_…cnnd`), activate a node's
  display → strip reads `refresh … — eval … · tess … · gpu … · view …`
  with a mode tag matching the action.
- **Drag a gadget and watch for regression:** the strip updates a few
  times a second, reads `(Lightweight)` with `eval —`, and dragging feels
  exactly as it did before this phase. This is the one thing Phase 1 can
  break (D8a) and no automated test covers it.
- Switch to a trivial network → the strip's numbers drop; it never shows
  a stale reading from the previous design.
- Toggle a display on/off repeatedly → `Partial` refreshes are visibly
  cheaper than the `Full` one, which is the phase's whole claim.

### Phase 2 — Per-node profiler + panel (D3, D4, D5, D6, D8b, D12)

`EvalProfile` in the thread-local (it is both the live accumulator and
the finished report — one type, no separate `EvalProfiler`), the RAII
guard, node identity + `NodeLocation`, the install/take in
`with_eval_context`, the *View* menu toggle, the panel's first three
tabs, and the CSG cache hit/miss counts beside the phase totals (D12 —
`get_csg_cache_stats` already exists). With Phase 1 this is "basic
profiling".

#### Tests

- Evaluation counts: a diamond records the apex **twice**; a chain
  records each node **once**; two instances of one custom network
  aggregate under distinct frames but the same node identity (D5).
- Invariants: `self_ns <= total_ns` for every record; a node's
  children's summed `total_ns` never exceeds its own.
- Guard release: a network with a wire cycle (tripping the re-entrancy
  backstop) and one with a `Unit`-skipped effect node both leave the
  child-accumulator stack **empty** at end of pass. These are the two
  exit paths that bypass the tail of `evaluate`, and a leak there is
  silent (D4).
- **The eager-HOF context split** — the tests that would have caught the
  bug D4/D6 describe:
  - a `fold` over 3 elements produces records for its **body** nodes at
    all (they run against a `fresh_inner_for_eager_body` context, so
    context-owned state would lose them entirely);
  - body time was *subtracted* from the HOF, stated without a ratio:
    `fold.total_ns − fold.self_ns >= Σ (body records' total_ns)`;
  - the same for `apply` and `foreach`, which take the same path.
- The toggle is honoured: with profiling off, `node_stats` is `None` and
  no records accumulate.
- Profiling does not change results: one fixture evaluated with the
  toggle off and on produces identical node output strings. A profiler
  that perturbs the pass it measures is worse than none.
- `NodeLocation` round-trips: a record for a node **inside a zone body**
  yields a `scope_path` + `node_id` that the same lookup the navigation
  uses resolves back to that node (D5) — otherwise click-to-jump lands
  nowhere and only manual testing would notice.
- **Stack budget:** re-run the debug-build `tag_test` 33-node chain with
  profiling **on**. The guard adds a frame to the recursion the
  STACK-SIZE WARNING is about (D4), and this is the test that catches it.

#### Manual walkthrough

- *View* menu shows the profiler toggle next to the Console entry;
  enabling it and refreshing populates the Phases / By node type / By
  node tabs.
- Press **Profile full refresh** twice → the two readings are comparable
  (that button exists precisely so they are; without it the panel shows
  whatever partial refresh ran last).
- Click a row in **By node** → the canvas selects that node; repeat with
  a row that lives *inside a HOF body* → the canvas enters the body
  scope. Both use the existing Find Usages / error-navigation jump.
- Sort by self time and sanity-check the shape against intuition: a
  custom-node instance shows ~zero self and a large total; `map` shows
  near-zero total with the time under `collect` (D4's two documented
  artifacts — if the panel doesn't explain them, this is where it shows).
- Toggle profiling **off** → panel stops updating and the application
  feels exactly as it did in Phase 1.

### Phase 3 — Frame key + redundancy (D9, D10, D11)

`env_epoch`, the frame-identity split and the key, the
`fresh_inner_for_eager_body` / `drain_inner_context` carry-and-merge of
`next_env_epoch`, the redundancy metrics, the Redundancy tab, and the
self-check. Must be reviewed against `eval_frame_key`'s doc comment line
by line — in particular its "two live frames" argument, which is the one
claim this key may **not** borrow. Its output is the memo's
precondition: per-node numbers, and a self-check that ran clean on real
designs.

Budget for the field addition. `NetworkStackElement` is built by struct
literal at **~110 sites — 10 in `src/`, ~100 in tests**. Add
`::root`, `::instance`, `::body_invocation` (allocates an epoch) and
`::body_static` (takes none), and migrate every literal in the same
change — the edits are mechanical and compiler-driven, and the
constructors make every later field addition free instead of repeating
this. The split between the last two is the point: the four `src/` body
pushes are **not** interchangeable (D9's table), so a future push site
has to choose rather than defaulting into the wrong one.

#### Tests

- A diamond reports `lookups = 2, distinct_envs = 1`.
- A `map` body over 3 elements reports `lookups = 3, distinct_envs = 3` —
  the test that proves the epoch works.
- A node inside a HOF body that feeds **two** displayed nodes in that
  same body reports `lookups = 2, distinct_envs = 1`: both descents push
  the body identically, so the two evaluations share an environment. With
  a fresh epoch per descent it would read `distinct_envs = 2` — which is
  what proves the scene-descent push (`:707`) allocates none.
- A capture cone re-evaluated under the same enclosing environment
  reports `distinct_envs = 1` — proves the capture pre-evaluation pushes
  (`:149`, `:377`) keep epoch 0.
- Epoch allocation is monotonic across the **whole pass**, including
  epochs handed out inside a `fold`/`foreach`/`apply` body: none is ever
  re-issued after the inner context drains (the `max` merge, D9). Backed
  by the debug assertion in `drain_inner_context`.
- **The address is not an input to the key** (the D9 hazard, as a unit
  test on the key function): two body frames over *different* network
  allocations with the same owner id and epoch hash **equal**; two over
  the *same* allocation with different owner ids hash **differently**.
  This is the regression test for a bug that would otherwise appear only
  as a slightly-too-good redundancy number.
- `wasted_ns` arithmetic (D10) on a synthetic record set, including the
  post-memo case `evaluations < lookups`, so the column stays meaningful
  after the memo lands.
- The self-check (D11) has teeth: it runs clean on the fixtures above,
  **and** a test-only key variant with `decorate` omitted is *caught* by
  it. A check that can't fail proves nothing.

#### Manual walkthrough

- Open `SPM-tip-with-tool_…cnnd` (network
  `0_pecursor_edit_sequence_H&Cl`), press **Profile full refresh**,
  open **Redundancy**: totals plus per-node offenders ranked by
  `wasted_ns`. Expect a factor well above 1 on `materialize` and ~1.0 on
  body nodes (D10's "realistic shape") — if the numbers are globally flat
  the key is over-splitting and the memo case is not yet made.
- Display the additional `structure_move` from the Motivation section and
  re-profile: the doubling must show up as **more lookups**, not more
  `distinct_envs`. That difference is the entire business case for
  `design_eval_memoization.md`; if `distinct_envs` doubles too, the memo
  would not help and this document has just said so.
- Confirm the flagged rows (iterator producers, re-entrancy-backstop
  results) are visibly marked, so `wasted_ns` there isn't read as an
  available saving.
- In a debug build, enable the self-check and open 2–3 real designs: no
  assertion fires, and the panel states that the memo is forced off for
  those passes (D11 — otherwise a green result means nothing later).

### Phase 4 — Canvas heat overlay (optional)

D8c, only if the tables turn out to be used and the canvas view adds
something.

#### Tests

- Subtitle composition: a node with a record renders `11× · 240 ms`
  appended to its existing subtitle; a node with **no** record renders
  its subtitle unchanged (nodes skipped by the Unit rule, or never
  evaluated, must not read as "0 ms").

#### Manual walkthrough

- Enable the overlay on a profiled design → the hot path is visibly
  tinted and the subtitles carry counts; disable it → subtitles and
  colours return exactly to normal.
- With per-node profiling off, the overlay shows no tint at all rather
  than a stale one from the last profiled pass.

## Documentation to update alongside

Each item lands **in** the phase named, not in a documentation pass at
the end.

- `doc/reference_guide/ui.md` — user-visible surfaces, split across the
  phases that add them: the status strip (**Phase 1**), the *View* menu
  toggle and the Profiler panel's first three tabs (**Phase 2**), the
  Redundancy tab and what its columns mean (**Phase 3**), the heat
  overlay (**Phase 4**). Include D4's two attribution artifacts — lazy
  iterators charging their consumer, and custom instances having ~zero
  self time — where the panel shows them; a user who doesn't know that
  reads the table as broken.
- `evaluator/AGENTS.md` — three new invariants contributors can break.
  **Phase 2:** the profiler hook in `evaluate` / `evaluate_all_outputs`
  with its RAII-guard requirement, and the rule that per-pass state
  belongs either in the pass thread-local or in **both**
  `fresh_inner_for_eager_body` and `drain_inner_context`, never on the
  context alone (D4/D6). **Phase 3:** which body pushes allocate an
  `env_epoch` and which keep 0 (D9's table), since a new push site
  defaulting to the wrong one is a silent wrong number now and a silent
  wrong result after the memo.
- `flutter_rust_bridge.yaml` — **Phase 1**: a new `profiling_api` module
  must be added to `rust_input`, or codegen silently ignores it (the
  failure mode is a Dart-side type that degrades to an opaque handle, not
  a build error).
