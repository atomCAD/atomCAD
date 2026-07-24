# Design: Within-Pass Evaluation Memoization

The network evaluator re-evaluates shared upstream work redundantly:
a diamond dependency re-runs the shared apex once per consuming wire,
and every displayed node re-walks its entire upstream cone
independently of every other displayed node. For chained diamonds the
redundancy is exponential (a ladder of N diamonds evaluates the base
2^N times). This design adds a **per-pass result memo** that
eliminates the redundancy without touching the genuinely hard problem
— cross-refresh caching with dependency invalidation — which is an
explicit non-goal.

Companion document: `doc/design_error_management.md`. The designs are
independent; the single interaction point is D8 here / D7 there.

## Motivation

Established by code research (see Current state for citations):

- No result memoization exists anywhere in the evaluator. Every
  fan-out edge re-walks its full upstream cone.
- If displayed node A is upstream of displayed node B, A is fully
  evaluated twice per refresh — once as its own scene entry point and
  once inside B's recursion. With k displayed descendants, k+1 times.
- Heavy nodes (CSG carving, materialization, relax) make this real
  wall-clock cost, only partially blunted by the CSG→mesh tessellation
  cache (which caches the mesh conversion, not the `eval()` producing
  the `GeoNode`).

This is a *proven structural redundancy*, not speculative caching (per
the project rule: cache only proven hot paths): the redundancy factor
is a static property of the wire graph, and instrumentation (Phase 1)
quantifies it per design before the memo lands.

## Current state (analysis)

- `NetworkEvaluationContext` (`network_evaluator.rs:153-226`) holds no
  `NodeRef → NetworkResult` map. `resolve_incoming_wire` calls
  `evaluate` unconditionally (`network_evaluator.rs:1558`); neither
  `evaluate` (`:1811`) nor `evaluate_all_outputs` (`:1651`) consults or
  populates a cache.
- One context is shared across a refresh (`with_eval_context`,
  `structure_designer.rs:269`), but `generate_scene_scoped` clears the
  scratch state per displayed root (`network_evaluator.rs:579-586`);
  only `print_buffer` / `execute` / `use_vdw_cutoff` /
  `top_level_parameters` survive the loop. No results are shared.
- Existing caches, none of which is a result memo:
  - `csg_conversion_cache` (`network_evaluator.rs:473`) — CSG→poly-mesh
    tessellation only.
  - Zone capture cache `captured_source_values`
    (`network_evaluator.rs:225`; `zone_closure.rs:536`) — captures
    pre-evaluated once per HOF invocation, reused across iterations.
    Precedent for "check cache before evaluate" at a seam.
  - The invisible-node LRU (`structure_designer_scene.rs:178`) — a
    *display* cache of finished `NodeSceneData` for hidden nodes; never
    spares the evaluator upstream work.
- The evaluator AGENTS.md documents the absence of memoization for
  iterators as a **correctness feature**: each `Iter[T]` consumer needs
  an independent walker, so the producer runs once per consumer.
- Instance keying is sound for a memo: custom-network entry pushes the
  instance id onto `eval_scope_path` (`network_evaluator.rs:1726`,
  `1947`), so `NodeRef` distinguishes instances. Zone bodies also push
  (`:621`), but body-local evaluation is iteration-dependent (D3).
- `NetworkResult` derives `Clone, Default` only; heavyweight payloads
  (atom structures) are owned values, so a memo hit costs one clone —
  strictly cheaper than re-evaluating the cone that produced it.

## Non-goals

- **Cross-refresh caching.** Keeping results alive between refreshes
  requires dependency-based invalidation, dirty tracking, and memory
  budgets. The per-pass lifetime avoids all of it by construction. Any
  future incremental-evaluation effort is a separate design.
- **Memoizing iterators** or changing walker semantics (D4).
- **Background evaluation** of non-displayed nodes
  (`doc/design_background_evaluation.md`).
- Changing zone-body per-iteration evaluation or the capture cache.

## Design decisions

### D1. Per-pass lifetime — no invalidation problem exists

The memo is created at the start of a refresh pass (`refresh_full` /
`refresh_partial`), shared across the entire displayed-roots loop, and
dropped at the end. Network data cannot change mid-pass, so entries
never go stale. It must NOT be cleared by `generate_scene_scoped`'s
per-root scratch reset — sharing across roots is where the largest win
(displayed-upstream-of-displayed) comes from.

### D2. Key: `(NodeRef, output_pin_index)`

`NodeRef` = `(eval_scope_path, node_id)` — already the keying scheme of
`node_errors` / hover strings, already instance-disambiguating. Value:
the pin's `NetworkResult` (memo hit returns a clone).

### D3. No memoization inside zone-iteration frames

A body-local node evaluates once per element with different zone-input
values — same `NodeRef`, different results. Rule: **skip both lookup
and insertion while any `is_zone_body` frame is on the network stack**
(the flag exists on `NetworkStackElement`,
`network_evaluator.rs:42-56`). Captures — the iteration-invariant part
— are already cached by `captured_source_values`; bodies lose nothing.
0-ary closure bodies rendered by the scene (`is_zone_body` pushes
during `generate_scene_scoped` descent, `:621`) follow the same
conservative rule in v1; they are iteration-free and could be admitted
later if profiling justifies it.

### D4. Exclude `NetworkResult::Iterator` (and nothing else)

Walkers are stateful streams; sharing one between consumers would
interleave/exhaust it. The documented "producer runs once per walker
consumer" behavior is preserved by never storing `Iterator` results.
Everything else is cacheable — including `Function` values
(`ZoneClosure` is Arc-backed and immutable).

### D5. Effects and `print` fire once per pass (accepted change)

Today a `print` node with fan-out 2 prints twice per display pass;
memoized it prints once. This is the more correct semantics ("one
evaluation per pass") and applies equally under Execute — an effect
node's side effect fires once per pass regardless of fan-out. The
central Unit-skip rule (`network_evaluator.rs:1671-1694`) is
unaffected: skipped synthesized Unit outputs need no memo entry.

### D6. Memory strategy: measure first, then refcount eviction

Holding all intermediate results for a pass can spike peak memory on
million-atom flows. V1 caches everything and reports the memo's peak
entry count/size via the Phase 1 instrumentation. The refinement (only
if measurements demand it): precompute per-`(node, pin)` consumer
counts from the wire graph, store only fan-out > 1 results, and drop
each entry after its last read.

### D7. Single seam

Lookup/insert happens at one choke point — `resolve_incoming_wire`'s
`NodeOutput` arm (`network_evaluator.rs:1558`) plus the
`generate_scene_scoped` entry evaluation — mirroring how the capture
cache is consulted. No per-node-type code changes.

Note on error/hover recording: on a memo hit the producing node's
`eval` is skipped, so no new `node_errors` / `node_output_strings`
insert happens for it in the *current root's* snapshot. This is
already the correct outcome: the node's entries were recorded under
its own `NodeRef` when it was first evaluated, and
`get_node_error` / `get_node_output_strings` scan all snapshots
(`structure_designer_scene.rs:259`, `:247`).

### D8. Interaction with error management (origin links)

`doc/design_error_management.md` D7 records
`consumer NodeRef → source NodeRef` origin links whenever a resolved
wire value is an `Error`. That recording happens at the same seam as
the memo lookup and **must fire on cache hits too** — a cached
upstream `Error` still links its second consumer to the root cause.

## Phases

### Phase 1 — Instrumentation
Cheap counters in the evaluator (debug/feature-gated): evaluations per
`(NodeRef, pin)` per pass; report redundancy factor and would-be memo
peak size. Validates the win on real designs before any behavior
change.

### Phase 2 — The memo
Implement D1–D5, D7 behind the rules. Tests: diamond evaluates apex
once, results identical; displayed-upstream-of-displayed evaluates once
per pass; zone bodies still evaluate per element (map over 3 elements →
body node evaluated 3 times); iterator fan-out still yields independent
walkers (existing iterator tests must stay green unchanged); `print`
fan-out fires once (update affected test expectations deliberately);
two instances of one custom network don't share results.

### Phase 3 — Memory refinement (conditional)
Fan-out-counted storage with last-read eviction, only if Phase 1/2
measurements show problematic peaks.

## Deferred / follow-ups

- Admitting 0-ary-closure body scenes to the memo (D3 conservatism).
- Cross-refresh incremental evaluation — separate future design; this
  memo neither helps nor hinders it.
- A shared-walker design for iterators (would require restartable
  walkers) — no current need.
