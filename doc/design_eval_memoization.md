# Design: Within-Pass Evaluation Memoization

The network evaluator re-evaluates shared upstream work redundantly: a
diamond dependency re-runs the shared apex once per consuming wire, and
every displayed node re-walks its entire upstream cone independently of
every other displayed node. For chained diamonds the redundancy is
exponential (a ladder of N diamonds evaluates the base 2^N times). This
design adds a **per-pass result memo** that eliminates the redundancy
without touching the genuinely hard problem — cross-refresh caching with
dependency invalidation — which is an explicit non-goal.

Division of labour with `doc/design_eval_profiling.md`: this document
owns the *evaluation environment model* below — what a node's result
depends on, and why the key is sufficient. That document owns the
mechanism that computes it, and measures the redundancy. **This design
does not start until its Phase 3 has produced per-node numbers and its
self-check has run clean on real designs.** The single interaction point
with `doc/design_error_management.md` is D8 here / D7 there.

> **Revision note.** The original draft keyed the memo on
> `(NodeRef, output_pin_index)`. That key is unsound; §"The evaluation
> environment" replaces it, and D2–D7 plus the phase plan were rewritten
> to follow from it.

## Motivation

- No result memoization exists anywhere in the evaluator. Every fan-out
  edge re-walks its full upstream cone.
- If displayed node A is upstream of displayed node B, A is fully
  evaluated twice per refresh — once as its own scene entry point and
  once inside B's recursion. With k displayed descendants, k+1 times.
- **`evaluate` already computes every output pin and discards all but
  one.** Its built-in arm calls `node.data.eval(...)` and returns
  `eval_output.get(output_pin_index)` (`network_evaluator.rs:2234-2269`),
  so a two-output node consumed on both pins runs `eval` twice today and
  throws away half of each result.
- Heavy nodes (CSG carving, materialization, relax) turn this into real
  wall-clock cost, only partially blunted by the CSG→mesh tessellation
  cache — which caches the mesh conversion, not the `eval()` producing
  the `GeoNode`.

This is a *proven structural redundancy*, not speculative caching (per
the project rule: cache only proven hot paths). Its measured magnitude
on a real design is in `doc/design_eval_profiling.md` §Motivation, and
its Phase 3 produces the trustworthy per-node numbers this design is
gated on.

## Current state (analysis)

- `NetworkEvaluationContext` (`network_evaluator.rs:153-226`) holds no
  result map. `resolve_incoming_wire` calls `evaluate` unconditionally
  (`:1740`); neither `evaluate` (`:2127`) nor `evaluate_all_outputs`
  (`:1892`) consults or populates a cache.
- One context is shared across a refresh (`with_eval_context`,
  `structure_designer.rs:307`), but `generate_scene_scoped` clears the
  scratch state per displayed root (`network_evaluator.rs:669-676`);
  only `print_buffer` / `execute` / `use_vdw_cutoff` /
  `top_level_parameters` survive the loop. No results are shared.
- Existing caches, none of which is a result memo:
  - `csg_conversion_cache` (`:561`) — CSG→poly-mesh tessellation only.
  - Zone capture cache `captured_source_values` (`:225`;
    `zone_closure.rs:536`) — captures pre-evaluated once per HOF
    invocation, reused across iterations. Precedent for "check cache
    before evaluate" at a seam.
  - The invisible-node LRU (`structure_designer_scene.rs:186`) — a
    *display* cache of finished `NodeSceneData`; never spares the
    evaluator upstream work.
- `NetworkResult` derives `Clone, Default` only; heavyweight payloads
  are owned values, so a memo hit costs one clone — strictly cheaper
  than re-evaluating the cone that produced it.

## The evaluation environment

This section is load-bearing; everything else follows from it.

### The input surface is closed

`NodeData::eval` (`node_data.rs:156`) can read exactly six things:

```rust
fn eval(&self, network_evaluator, network_stack, node_id,
        registry, decorate, context) -> EvalOutput;
```

Anything a node pulls recursively arrives back through the same six, so
enumerating them enumerates every input:

| Argument | Varies with the environment? |
|---|---|
| `&self` — the node's own data | No. Immutable during a pass. Six interior-mutability fields exist — `cached_input`, `cached_unit_cell`, `last_stats` (atom_edit), `available_parameters` (materialize, motif_sub), `last_report` (patch_latticefill), `available_tags` (tag) — and **all six are write-only during `eval`**; no `borrow()` of them feeds a result. |
| `network_evaluator` | No. Holds only `csg_conversion_cache`, which is result-neutral. |
| `network_stack` | **Yes** — the frame path (below). |
| `node_id` | **Yes.** |
| `decorate` | **Yes.** Genuinely changes results — `atom_edit_data.rs:2213`, `:2250` and `edit_atom.rs:90` decorate atoms when the node is the selected root. |
| `registry` | No. Immutable for the pass. |
| `context` | Mixed — split below. |

There is no ambient state beyond these: no randomness anywhere in the
evaluator or in `atomcad-crystolecule`, and the only clock reads are
`print`'s timestamp and an interactive-only minimization timer
(`nodes/atom_edit/minimization.rs:155`) that never runs under `eval`.

`context` splits three ways:

- **Pass constants** — `execute`, `use_vdw_cutoff`,
  `top_level_parameters`. Identical for every evaluation in a pass, so
  they need no place in the key.
- **Outputs** — `node_errors`, `node_output_strings`,
  `node_error_origins`, `print_buffer`, `selected_node_eval_cache`,
  `eval_scope_path`. Written, never read to form a result.
- **Two live reads not determined by the network stack** —
  `current_zone_input_values` (read through `try_current_zone_input`,
  `:374`) and `captured_source_values` (an `Arc`, swapped per
  invocation). These are the entire gap, and `env_epoch` closes it.

### The frame is a path, not a struct

`NetworkStackElement` (`network_evaluator.rs:36-50`) is
`{ node_network: &NodeNetwork, node_id: u64, is_zone_body: bool }`, and
what matters is the whole slice plus the `node_id` being evaluated. A
frame's `node_id` is the node that *caused* the push: `0` for the root,
the instance node's id for a custom-network entry, the zone-owning
node's id for a body.

`eval_frame_key` (`:246-272`) already fingerprints this path for the
re-entrancy guard, and its doc comment explains why **`NodeRef` is not a
frame identity**: a custom-network `parameter` resolves its argument by
a *stack excursion* that pops the network frame while the instance's
eval scope stays pushed (`nodes/parameter.rs:110-125`), and per-network
`next_node_id` counters let a body node and a top-level node share a
numeric id. For the re-entrancy guard a `NodeRef` collision produced a
spurious error; for a memo it would return **a wrong value**. Read that
doc comment before touching this design.

### `env_epoch` closes the zone gap

One `u64` per body invocation, allocated from a per-pass counter and
stamped onto the body frame at push time (`0` on non-body frames). It
works because `run_closure_once` rebuilds `body_stack` on every
invocation, and an invocation is exactly the unit at which both live
reads change: `push_zone_input_frame` installs the iteration values and
`captured_source_values` is swapped, both inside that function, both
bracketed by that push.

A `map` over three elements therefore yields three distinct epochs on
otherwise byte-identical stacks. Two consumers pulling *within* one
iteration share a key; two iterations never do. The epoch propagates
downward for free because the key walks the whole path — a custom
network entered from inside iteration 7 has stack
`[{root, ep:0}, {body, 87, ep:7}, {instance, 6, ep:0}]`, and frame 1
still carries epoch 7.

A monotonic counter, **not** the captures `Arc` pointer: an address can
be freed and reallocated within a pass (ABA), silently aliasing two
different environments.

The field, the counter, and the four push sites are specified in
`doc/design_eval_profiling.md` D9, which implements them a phase ahead.

### The key

```
MemoKey = hash( for each frame: (network address, frame node_id, env_epoch),
                node_id,
                decorate )
```

Note what is *absent*: the output pin index (D2) and any hash of input
values. Values never need hashing because the environment determines
them.

Because a collision here returns a wrong value rather than a spurious
error, store the full key material alongside a 64-bit digest and verify
on hit, or use a 128-bit digest. This is the one place this design is
less collision-tolerant than the re-entrancy guard.

### Exclusions and the direction of safety

Given this key, "same environment ⇒ same result" holds except for three
cases, each decided below: **re-entrancy** (D9 — the one genuine
remaining hole), **effects** (D5 — same result, different side effects),
and **iterators** (D4 — a memory question, not a soundness one).

Over-keying costs a missed cache hit; under-keying returns a wrong
value. When a future change makes it unclear whether something belongs
in the key, **put it in the key.** `doc/design_eval_profiling.md` D11 is
the empirical backstop: it asserts that evaluations sharing a key
produced equal results, and would have caught both the `decorate`
omission and the `NodeRef` collision on a real design.

### The user-facing cost model

The key projects into a mental model simple enough to teach, and this
design is only finished when the model is true:

> **A node's result is computed once per call stack, where the call
> stack is extended with the iteration index of each enclosing HOF.**

In a network with no HOFs and no subnetworks that collapses to "every
node is computed once per refresh" — which is what users already assume,
and which is false today. The model is compositional: a body can be
reasoned about without knowing what encloses it.

It yields one actionable optimization, and it is narrower than "hoist
work out of loops", because the runtime already hoists part of it:

| Where an expensive node sits | Runs |
|---|---|
| Outside a body, feeding it through a capture wire | once per invocation — already true today (`build_captures`) |
| **Inside a body, but loop-invariant** (all inputs are captures) | **once per element — move it out of the body and it runs once** |
| Inside a body, reading the iteration value | once per element; nothing to do |

The middle row is the whole advice. The third row is not a limitation
users can work around, and the system enforces it: a node reading the
iteration value cannot be hoisted, because a zone-input wire cannot
reach outside its body.

Two caveats belong with the model, or it over-promises:

- **Two instances of the same subnetwork compute twice** — different
  call stacks, correctly so, since their arguments differ. Without this,
  the model invites "wrap it in a subnetwork and it will be shared",
  which is backwards.
- **Iterators are the exception** (D4), and a lazy body's cost lands on
  whoever pulls it — `map` appears to cost nothing while `collect`
  appears to cost everything.

**Deliverable:** a "Cost model" subsection in
`doc/reference_guide/node_networks.md`, written in these terms and
shipping **with Phase 2**, not before — today only the capture row is
true, and documenting the rest early would describe behavior the
application does not have.

## Non-goals

- **Cross-refresh caching.** Keeping results alive between refreshes
  requires dependency-based invalidation, dirty tracking, and memory
  budgets. The per-pass lifetime avoids all of it by construction; any
  incremental-evaluation effort is a separate design.
- **Changing walker semantics** or the zone capture cache.
- **Background evaluation** of non-displayed nodes
  (`doc/design_background_evaluation.md`).
- **Measuring the win** — `doc/design_eval_profiling.md`.

## Design decisions

### D1. Per-pass lifetime — no invalidation problem exists

The memo is created at the start of a refresh pass, shared across the
entire displayed-roots loop, and dropped at the end. Network data cannot
change mid-pass, so entries never go stale. It must NOT be cleared by
`generate_scene_scoped`'s per-root scratch reset — sharing across roots
is where the largest win (displayed-upstream-of-displayed) comes from.

### D2. Key: the environment; value: the whole `EvalOutput`

The key is the `MemoKey` above. The **value is the complete
`EvalOutput`**, not one pin's `NetworkResult`.

Storing the whole output is what the original per-pin key got wrong:
`evaluate` already runs `eval` once and computes every pin, so one entry
per (environment, node) also removes the multi-pin redundancy, and keeps
`display_results`, `pin_subtitles`, and `unit_cell_override` intact for
whichever consumer needs them. `evaluate` serves a hit through
`eval_output.get(pin)`; `evaluate_all_outputs` serves it whole. Either
function's insertion is usable by the other.

`decorate` is in the key rather than a reason to skip caching: the
selected node is normally evaluated once with `decorate=true` (as its own
scene root) and repeatedly with `decorate=false` (as an input). Both are
worth memoizing — just not against each other.

### D3. Zone bodies are memoized, keyed by epoch

The original draft skipped memoization entirely while any `is_zone_body`
frame was on the stack. With `env_epoch` that is unnecessary: a
body-local entry is keyed to its iteration and cannot leak into the next.

Be honest about the size of the win — entries created inside iteration N
are only reusable *within* iteration N, so this buys fan-out inside a
body, not reuse across elements. Bodies are small on today's designs
(2–4 nodes in the reference file). The point is that the rule is now
principled rather than a blanket exclusion.

Per-iteration entries are dead the moment their body frame pops. Evict
them there — keep a per-epoch list of inserted keys and drop it on pop —
so a 10⁵-element `map` does not accumulate 10⁵ generations before the
LRU notices.

### D4. Iterators are excluded for memory, not for correctness

The original rationale — "sharing one walker between consumers would
interleave/exhaust it" — is **wrong for this codebase**. `Walker`
guarantees Invariant 2 (`evaluator/iterator_walker.rs:9-14`): cloning
yields an independent walker, every variant owns its position state, and
a memo hands out clones rather than sharing one. Two consumers would each
get an independent stream from position 0 — exactly today's semantics.

The conclusion survives for two weaker reasons. Memoizing buys almost
nothing: a `map` node's `eval` only *builds* a walker; the expensive work
happens in `next()`, driven by the consumer, and the memo does not touch
it. And a stored walker pins its `ZoneClosure` — possibly an
`Arc<Vec<NetworkResult>>` over a large source array — for the whole pass.

So: **do not store `NetworkResult::Iterator` results**, and flag them in
the profiler's redundancy view so the numbers stay honest.

### D5. Effects fire once per pass (accepted change)

Today a `print` node with fan-out 2 prints twice per display pass;
memoized it prints once. This is the more correct semantics ("one
evaluation per pass") and applies equally under Execute. The central
Unit-skip rule (`:1911-1948`) is unaffected — skipped synthesized Unit
outputs need no memo entry.

The same reasoning extends to every node that mutates NodeData during
`eval` (the six fields listed in the environment table). All are
write-only, so results are unaffected, but the memo makes the *first*
write win where the *last* one used to. **Implementation task:** confirm
per field that first-write and last-write agree — they should, since the
writes are environment-determined too — and add a regression test for
`atom_edit`'s `get_cached_input()` and `get_subtitle()`.

### D6. Memory: a bounded LRU, not a fan-out precompute

Reuse `atomcad_util::memory_bounded_lru_cache::MemoryBoundedLruCache`,
which already backs the CSG cache (`csg_cache.rs:83`) and the
invisible-node cache, with `estimate_memory_bytes` for sizing. An
evicted entry is simply recomputed, so memory is bounded **by
construction** and needs no correctness argument.

This replaces the original plan (precompute per-`(node, pin)` consumer
counts, store only fan-out > 1, drop after last read): more code, its own
correctness argument, and no bound on a pass that is large for reasons
other than fan-out. D3's epoch-scoped eviction removes the largest source
of garbage before the LRU has to act.

### D7. Seam: `evaluate` and `evaluate_all_outputs`

Lookup and insert go in the two functions that dispatch to
`NodeData::eval` — the same hooks the profiler uses
(`doc/design_eval_profiling.md` D3), so the two agree by construction on
what counts as an evaluation, and the network stack the key needs is
already in hand.

On a memo hit the producing node's `eval` is skipped, so no new
`node_errors` / `node_output_strings` insert happens for it in the
*current root's* snapshot. That is already correct: the entries were
recorded under the node's own `NodeRef` when it was first evaluated, and
`get_node_error` / `get_node_output_strings` scan all snapshots
(`structure_designer_scene.rs:259`, `:247`).

### D8. Interaction with error management (origin links)

`doc/design_error_management.md` D7 records
`consumer NodeRef → source NodeRef` origin links whenever a resolved wire
value is an `Error`. That recording happens in `evaluate_arg`, *outside*
the memo seam, so it should fire on cache hits automatically — a cached
upstream `Error` still links its second consumer to the root cause.
Verify with a test rather than assuming.

### D9. Never memoize a result produced under re-entrancy

When `context.eval_in_progress.insert(frame_key)` returns `false`, the
synthesized cycle `Error` must be returned **without** being inserted,
and the enclosing evaluation of the same environment must still insert
its own real result. Without this the inner `Error` is stored first and
served to every later consumer of that node.

This is the one case where the key is genuinely insufficient: with a
cycle `A → B → A`, the inner and outer evaluations of A share a
byte-identical stack, involve no body frames, and return different
results. Cycles are supposed to be caught by validation, so this is a
backstop on a backstop — but it fails silently rather than loudly.

## Phases

**Phase 1 — Instrumentation.** Delivered by
`doc/design_eval_profiling.md` Phase 3: `env_epoch`, the key, the
per-node redundancy factor, the would-be memo peak size, and the D11
self-check. Phase 2 below does not start until those numbers exist.

**Phase 2 — The memo.** Implement D1–D9. Tests: a diamond evaluates the
apex once with identical results; displayed-upstream-of-displayed
evaluates once per pass; a two-output node consumed on both pins
evaluates once (new — the old key could not do this); a `map` over 3
elements still evaluates its body 3 times (D3); iterator fan-out still
yields independent walkers, with existing iterator tests green unchanged;
`print` fan-out fires once (update expectations deliberately); two
instances of one custom network do not share results; a
`parameter`-excursion graph does not cross-contaminate parent and child
nodes of the same id (the regression the old key would have caused); a
cyclic graph's non-cyclic consumers still get real values (D9).

**Acceptance criterion**, in the profiler's own terms
(`doc/design_eval_profiling.md` D10): on the designs measured before the
change, every node not flagged as deliberately-uncached reports
`evaluations == distinct_envs`, and the measured drop in eval-phase time
matches the `wasted_ns` predicted beforehand. Note this is stated over
*evaluations*, not lookups — lookups stay high by design once the memo
starts serving them.

Phase 2 also ships the reference-guide "Cost model" subsection — see
§"The user-facing cost model". The feature is not done until the guide
states the contract the memo now honours.

**Phase 3 — Memory tuning (conditional).** Only if Phase 2 shows
problematic peaks despite the LRU and epoch-scoped eviction: tune the
byte budget, or reintroduce fan-out counting for the shapes that need it.

## Deferred / follow-ups

- Cross-refresh incremental evaluation — a separate future design; this
  memo neither helps nor hinders it.
- Memoizing iterator *elements* rather than the walker — would need
  restartable walkers and a per-element key; no current need.
- Sharing memo entries across the eager HOFs' `fresh_inner_for_eager_body`
  contexts, which today get their own context and therefore their own
  memo.
