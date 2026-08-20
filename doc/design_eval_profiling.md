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
  the only functions that dispatch to `NodeData::eval`; between them
  they have 7 call sites across the crate. Both are already recursive
  and already maintain a per-frame bracket (the re-entrancy guard), so
  they are the natural instrumentation point.
- **Frame fingerprinting already exists.** `eval_frame_key`
  (`network_evaluator.rs:246-272`) hashes the network stack for the
  re-entrancy guard, and its doc comment explains why `NodeRef` is not a
  frame identity. Phase 3 extends that function rather than reinventing
  it; the reasoning is in `design_eval_memoization.md` §"The evaluation
  environment".
- **Existing per-pass plumbing to mirror.** `with_eval_context`
  (`structure_designer.rs:307`) constructs `NetworkEvaluationContext`,
  runs the pass, and **drains** `print_buffer` into
  `StructureDesigner.print_log` regardless of how the closure returned.
  The profiler drains the same way at the same seam.
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

**Phase timing is always on.** Six `Instant::now()` calls per refresh is
not measurable against a refresh measured in milliseconds, and always-on
means a regression surfaces during ordinary work rather than only during
a deliberate profiling session.

**Per-node profiling is opt-in.** It adds two clock reads and a hash-map
update per node evaluation — microseconds at ~10³ evaluations per pass,
but not inside a `map` body over 10⁵ elements, and a profiler that
inflates the numbers it reports is worse than useless.

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

The bookkeeping **must** be released by a guard with a `Drop` impl, not
by hand at each `return`. Both functions have many early exits — the
poison check, the cycle guard, the central Unit-skip rule, the
missing-node guard — and the existing `eval_in_progress` bracket already
carries a STACK-SIZE WARNING about exactly this class of leak. A leaked
frame corrupts every ancestor's self time silently.

Two attribution artifacts follow from evaluator semantics and must be
documented in the panel, not "fixed":

- **Lazy iterators shift time to the consumer.** A `map` body node runs
  when `collect` pulls it, so its time nests under `collect`; `map`'s own
  total looks near-zero.
- **A custom-node instance has ~zero self time.** It delegates to its
  network's return node (`:1985`), so its *total* covers the subnetwork
  while its *self* is bookkeeping only. That is the useful reading.

### D5. Node identity for aggregation is `(home network, node_id)`

"Which node is this?" is a different question from "which evaluation
environment is this?" (D9). The answer is read off
`network_stack.last()`: the top frame always names the network the node
actually lives in, which is unambiguous under stack excursions and
per-network id collisions alike.

`NodeRef` is **not** used. Mis-keying would only mis-attribute time
rather than corrupt results, but it would mis-attribute it in exactly
the confusing cases (a subnetwork's internals vs. its caller's), which is
where the profiler must be most trustworthy.

Aggregation to node *type* is a roll-up of the same records — both
tables come from one map.

### D6. The API layer assembles the report; each layer times what it owns

`RefreshProfile` is built in `refresh_structure_designer`
(`api_common.rs:444`), the only place that sees all three Rust-side
phases:

```
RefreshProfile {
    mode: Full | Partial | Lightweight,
    eval_ms, scene_dependent_ms, gadget_ms,   // from StructureDesigner::refresh
    tessellate_ms, gpu_upload_ms, background_ms,
    total_ms,
    node_stats: Option<EvalProfile>,          // Some only when profiling is on
}
```

`StructureDesigner::refresh` returns its own sub-phase timings rather
than reaching outward — the crate may not reference `api/`. The
`EvalProfile` is drained out of `NetworkEvaluationContext` in
`with_eval_context` alongside the existing `print_buffer` drain, so no
call site can forget it.

Reports live in a bounded ring on `StructureDesigner` (last ~20) and are
**read, not drained** — a profile is a snapshot the panel re-renders;
draining would empty the panel on every unrelated poll.

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
```

This alone answers "is it evaluation or something else?" without opening
anything. It is the highest-value element of this design.

**(b) Profiler panel — Phases 2–3.** Bottom-docked, built on the
`console_panel.dart` template, with four tabs:

- **Phases** — last refresh plus the history ring, tagged with mode.
  Makes a 40 ms drag tick vs. a 1.8 s node activation visible at a
  glance.
- **By node type** — `Type | Evals | Self | Total | % self`, sorted by
  self time.
- **By node** — `Network#id (custom name) | Lookups | Evals | Self |
  Total | Wasted`, click-to-jump. Reuse the scope-aware canvas navigation from
  Find Usages / error navigation (`root_cause_navigation.dart`).
- **Redundancy** — Phase 3 only. Totals (evaluations, distinct
  environments, factor) plus per-node offenders ranked by **Wasted**
  (D10). This tab is the memo's business case and, afterwards, its
  regression test.

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
hash( for each frame: (network address, frame node_id, env_epoch),
      node_id, decorate )
```

This document owns the mechanism — one field and one counter:

```rust
// NetworkStackElement
pub env_epoch: u64,        // 0 for non-body frames

// NetworkEvaluationContext
next_env_epoch: u64,       // starts at 1
pub fn alloc_env_epoch(&mut self) -> u64 {
    let e = self.next_env_epoch; self.next_env_epoch += 1; e
}
```

stamped at each of the four body-push sites (`zone_closure.rs:149`,
`:377`, `:504`; `network_evaluator.rs:707`). It works because
`run_closure_once` rebuilds `body_stack` on every invocation
(`zone_closure.rs:512`), and an invocation is exactly the unit at which
the zone-input frame and the captures `Arc` change — both installed
inside that function, both bracketed by that push. A `map` over three
elements yields epochs 7, 8, 9 on otherwise byte-identical stacks, so
its body reports **redundancy 1.0, not 3×**.

**Phase 3 computes this key and counts collisions. It does not act on
them** — a wrong key produces a wrong number, never a wrong result.
That is why the memo's key is built and validated here rather than
there.

Two rules make this survive the memo landing on top of it:

- **`env_epoch` is allocated unconditionally**, not behind the
  per-node-profiling toggle (D1/D2). It is one increment per body push,
  against a push that already builds a stack and a captures map — and
  the memo needs it in every pass, including passes with profiling off.
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
caught both the `decorate` omission and the `NodeRef` collision on a
real design, with no risk of serving a wrong value.

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

**Phase 1 — Refresh phase timing + status strip.** Always-on timers at
the four boundaries (D6, D7), `RefreshProfile`, the FFI getter, the
status strip (D8a). No evaluator changes. Answers "is evaluation the
bottleneck?" before any evaluator code is touched.

**Phase 2 — Per-node profiler + panel.** `EvalProfiler` on
`NetworkEvaluationContext`, the RAII guard (D3, D4), node identity (D5),
the drain in `with_eval_context`, the *View* menu toggle, and the panel's
first three tabs (D8b). With Phase 1 this is "basic profiling".

**Phase 3 — Frame key + redundancy.** `env_epoch`, the key (D9), the
redundancy metrics (D10), the Redundancy tab, and the self-check (D11).
Must be reviewed against `eval_frame_key`'s doc comment line by line.
Its output is the memo's precondition: per-node numbers, and a
self-check that ran clean on real designs.

Budget for the field addition. `NetworkStackElement` is built by struct
literal at **~100 sites, ~90 of them in tests**. Add
`NetworkStackElement::root / ::instance / ::body(…, epoch)` constructors
and migrate the literals in the same change — the edits are mechanical
and compiler-driven, and the constructors make every later field
addition free instead of repeating this.

**Phase 4 — Canvas heat overlay (optional).** D8c, only if the tables
turn out to be used and the canvas view adds something.

## Testing

Wall-clock assertions are flaky and are not written. What is tested is
**structure**, in
`rust/crates/atomcad-structure-designer/tests/structure_designer/`:

- evaluation counts: a diamond records the apex twice; a chain records
  each node once; two instances of one custom network aggregate under
  distinct frames but the same node identity (D5).
- invariants: `self_ns <= total_ns` for every record; a node's children's
  summed `total_ns` never exceeds its own.
- guard release: a network with a wire cycle (tripping the re-entrancy
  backstop) and one with a `Unit`-skipped effect node both leave the
  profiler stack empty at end of pass.
- Phase 3: a diamond reports `lookups = 2, distinct_envs = 1`; a `map`
  body over 3 elements reports `lookups = 3, distinct_envs = 3` — the
  test that proves the epoch works.

Flutter smoke tests are **not** run by agents (see `AGENTS.md`); the
panel and status strip go on the manual walkthrough list.

## Documentation to update alongside

- `doc/reference_guide/ui.md` — the status strip, the *View* menu
  entries, and the Profiler panel are user-visible.
- `evaluator/AGENTS.md` — the profiler hook in `evaluate` /
  `evaluate_all_outputs` and the RAII-guard requirement (D4) are a new
  invariant contributors can break.
- `flutter_rust_bridge.yaml` — a new `profiling_api` module must be added
  to `rust_input`, or codegen silently ignores it.
