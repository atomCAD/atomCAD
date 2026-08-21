# Design: Within-Pass Evaluation Memoization

The network evaluator re-evaluates shared upstream work redundantly: a
diamond dependency re-runs the shared apex once per consuming wire, and
every displayed node re-walks its entire upstream cone independently of
every other displayed node. For chained diamonds the redundancy is
exponential (a ladder of N diamonds evaluates the base 2^N times). This
design adds a **per-pass result memo** that removes it, without touching
the genuinely hard problem — cross-refresh caching with dependency
invalidation — which is an explicit non-goal.

Division of labour with `doc/design_eval_profiling.md`: this document owns
the *evaluation environment model* — what a node's result depends on, and
why the key is sufficient. That document owns the mechanism that computes
it, and measured the redundancy. Its Phase 3 is implemented and its
numbers are in §"The measurement", so this design is unblocked — and the
project rule "cache only proven hot paths" is satisfied by measurement
rather than by argument. The single interaction point with
`doc/design_error_management.md` is D8 here / D7 there.

## The measurement

Taken 2026-08-21 with the `design_eval_profiling.md` Phase 3 profiler on
`SPM-tip-with-tool_2026-08-20_12-52-CET.cnnd`, network
`0_pecursor_edit_sequence_H&Cl`, via *Profile full refresh*. This is the
**before-picture** the acceptance criterion is measured against; it must
not be re-taken after the memo lands.

**Refresh 6.63 s, of which eval 6.63 s** (tess / gpu / view all 0.00 —
this design is entirely evaluation-bound). **1169 lookups over 145
distinct environments, 8.06× overall**; a perfect memo would hold 145
entries and save **~6033 ms**.

| node | lookups | envs | factor | self (ms) | wasted (ms) |
|---|---|---|---|---|---|
| `geo.1-precursor_proxy/materialize#8` | 12 | **1** | 12.0× | 6292 | 5767 |
| `…H&Cl/atom_edit#113` | 11 | 1 | 11.0× | 140 | 127 |
| `…H&Cl/freeze#1` | 12 | 1 | 12.0× | 138 | 126 |
| `…H&Cl/geo.1-precursor_proxy#6` (instance) | 12 | 1 | 12.0× | 5.8 | 5.3 |
| `structure.14Si/motif#3` | 57 | 8 | 7.1× | 1.5 | 1.3 |
| `structure.14Si/structure#2` | 57 | 8 | 7.1× | 0.56 | 0.48 |

**One node is the whole story.** A single `materialize` inside the
`geo.1-precursor_proxy` subnetwork is 6292 ms of a 6630 ms evaluation and
is computed **twelve times in one environment** — not twelve loop
elements, not twelve call sites. Expected outcome: 6.6 s → ~0.6 s.

Three readings worth keeping:

- **The self-check ran clean** on this pass, so the 8.06× is not an
  artifact of an under-split key.
- **`Envs = 1` on the dominant rows**: pure fan-out and cross-root
  repetition, which is exactly what D1's shared-across-roots lifetime
  targets. No epoch subtlety is involved in the win.
- **`motif#3`, 57 lookups over 8 environments**, is the instance
  machinery working: the same node in eight subnetwork instances is
  eight environments, correctly not counted as redundancy. It costs
  1.5 ms — which is why the tab ranks by `wasted`, not by factor.

**Two things could erode the 6033 ms**, both worth measuring rather than
assuming:

- **A hit costs a clone.** `NetworkResult` payloads are owned, so serving
  `materialize` eleven times means eleven deep `AtomicStructure` clones.
  At this design's size that is milliseconds against 524 ms/evaluation;
  on a million-atom structure it may not be. D6 decides **clone**, for
  the simpler system; the escape hatch, if the measurement says
  otherwise, is to store `Arc<EvalOutput>` — a change to D2's value type,
  not to the key.
- **The custom-network arm does not insert** (D2). Here that costs almost
  nothing: the expensive node is *inside* the subnetwork and the instance
  carries only 5.8 ms of its own, so each of the 12 instance pulls
  re-enters and hits the memo at the child's return node.

## Current state (analysis)

- No result memo exists. `resolve_incoming_wire` calls `evaluate`
  unconditionally (`network_evaluator.rs:2014`, always with
  `decorate = false`); neither `evaluate` (`:2417`) nor
  `evaluate_all_outputs` (`:2166`) consults or populates a cache.
- **`evaluate` already computes every output pin and discards all but
  one** (`:2576`), so a two-output node consumed on both pins runs `eval`
  twice and throws away half of each result. D2 fixes this for free.
- One context is shared across a refresh (`with_eval_context`,
  `structure_designer.rs:367`), but `generate_scene_scoped` clears the
  scratch state per displayed root (`:948-958`); only `print_buffer` /
  `execute` / `use_vdw_cutoff` / `top_level_parameters` /
  `next_env_epoch` survive the loop.
- Existing caches, none of them a result memo: `csg_conversion_cache`
  (`:836`, mesh conversion only); the zone capture cache
  `captured_source_values` (`:314`, `zone_closure.rs:539` — precedent for
  "check cache before evaluate" at a seam); the invisible-node LRU
  (`structure_designer_scene.rs:187`, a *display* cache).
- `NetworkResult` derives `Clone, Default` only; payloads are owned, so a
  hit costs one clone.

## The evaluation environment

This section is load-bearing; everything else follows from it.

### Demand-driven, but without sharing

> Evaluation is **demand-driven**: nothing is computed until a displayed
> node asks for it, and each node pulls its inputs recursively from
> inside its own `eval` — some conditionally, so an untaken `if` branch
> is never computed (`nodes/if_else.rs`). What the evaluator lacks is
> **sharing**: a node reached twice is computed twice.
>
> That sharing cannot be keyed on input *values*, because when a node
> begins evaluating, its inputs do not exist yet — obtaining them **is**
> the work the memo would be avoiding. The key has to be something known
> beforehand: the node's **position in the evaluation** — which
> subnetwork instance, which loop iteration, and whether it is being
> decorated. Within one pass that position determines the result,
> because nothing in the network changes mid-pass.

In the standard vocabulary the evaluator is **call-by-name**, not
call-by-need: the demand-driven half of lazy evaluation without the
memoizing half, which is why the diamond apex runs twice. This design
supplies the missing half. (Not the *other* laziness in the system:
`Iter[T]` / `Walker` streams are lazy in the data sense — different axis,
and D4 is where the two meet.)

Three qualifications:

- **Not uniformly demand-driven.** Most nodes pull every wired pin
  unconditionally (`expr` does); conditional pulling lives in `if` and
  `switch`. Two mechanisms are eager: the Unit-skip rule declines to
  evaluate a node at all, and **capture pre-evaluation**
  (`zone_closure::build_captures`) evaluates every outside-the-body
  source once at body entry, demanded or not. So: demand-driven, except
  at body boundaries, where it is eager and shared.
- **Sound only *within* a pass.** Across refreshes the same position
  routinely yields a different result. This is a per-pass identity, not
  a cache key for incremental evaluation — reading it as the latter is
  the unsound version of this design.
- **Weaker than value keying, not better.** A conservative approximation
  of "same inputs": two different nodes computing the same value never
  share, and no common-subexpression elimination happens. It is chosen
  because it is the key that is *available and free*, and it errs toward
  a missed hit, never a wrong value.

The rest of this section proves the claim the box makes.

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
| `&self` — the node's own data | No. Immutable during a pass. Six interior-mutability fields exist — `cached_input`, `cached_unit_cell`, `last_stats` (atom_edit), `available_parameters` (materialize, motif_sub), `last_report` (patch_latticefill), `available_tags` (tag) — and **all six are write-only during `eval`**. |
| `network_evaluator` | No. Holds only `csg_conversion_cache`, which is result-neutral. |
| `network_stack` | **Yes** — the frame path (below). |
| `node_id` | **Yes.** |
| `decorate` | **Yes.** Genuinely changes results — `atom_edit_data.rs:2213`, `:2250` and `edit_atom.rs:90` decorate atoms when the node is the selected root. |
| `registry` | No. Immutable for the pass. |
| `context` | Mixed — split below. |

No ambient state beyond these: no randomness in the evaluator or in
`atomcad-crystolecule`, and the only clock reads are `print`'s timestamp
and an interactive-only minimization timer
(`nodes/atom_edit/minimization.rs:155`) that never runs under `eval`.

`context` splits three ways:

- **Pass constants** — `execute`, `use_vdw_cutoff`,
  `top_level_parameters`. Identical for every evaluation in a pass, so
  they need no place in the key.
- **Outputs** — `node_errors`, `node_output_strings`,
  `node_error_origins`, `print_buffer`, `selected_node_eval_cache`,
  `eval_scope_path`. Written, never read to form a result. (Not
  therefore harmless: see D5.)
- **Two live reads not determined by the network stack** —
  `current_zone_input_values` (via `try_current_zone_input`, `:631`) and
  `captured_source_values` (an `Arc`, swapped per invocation). These are
  the entire gap, and `env_epoch` closes it.

### The frame is a path, not a struct

`NetworkStackElement` (`network_evaluator.rs:39`) is
`{ node_network, node_id, is_zone_body, env_epoch }`, and what matters is
the whole slice plus the `node_id` being evaluated. A frame's `node_id`
is the node that *caused* the push: `0` for the root, the instance node's
id for a custom-network entry, the zone-owning node's id for a body.
Frames are built through `::root` / `::instance` / `::body_invocation` /
`::body_static`, which is where the epoch rule is enforced.

`eval_frame_key` (`:363`) already fingerprints this path for the
re-entrancy guard, and its doc comment explains why **`NodeRef` is not a
frame identity**: a custom-network `parameter` resolves its argument by a
*stack excursion* that pops the network frame while the instance's eval
scope stays pushed (`nodes/parameter.rs:108-125`), and per-network
`next_node_id` counters let a body node and a top-level node share an id.
For the guard a `NodeRef` collision produced a spurious error; for a memo
it would return **a wrong value**. Read that doc comment before touching
this design.

### `env_epoch` closes the zone gap

One `u64` per body invocation, allocated from a per-pass counter and
stamped onto the body frame at push time (`0` on non-body frames). It
works because `run_closure_once` rebuilds `body_stack` on every
invocation, and an invocation is exactly the unit at which both live
reads change: `push_zone_input_frame` installs the iteration values and
`captured_source_values` is swapped, both inside that function and both
bracketed by that push.

A `map` over three elements therefore yields three distinct epochs on
otherwise byte-identical stacks. Two consumers pulling *within* one
iteration share a key; two iterations never do. The epoch propagates
downward for free because the key walks the whole path — a custom network
entered from inside iteration 7 has stack
`[{root, ep:0}, {body, 87, ep:7}, {instance, 6, ep:0}]`, and frame 1
still carries epoch 7.

A monotonic counter, **not** the captures `Arc` pointer: an address can
be freed and reallocated within a pass (ABA), silently aliasing two
different environments.

Specified in `doc/design_eval_profiling.md` D9 and **already
implemented**. Two of its rules are load-bearing here and easy to undo by
accident: only `run_closure_once` allocates an epoch (capture
pre-evaluation and the displayed-body scene descent keep `0`, or capture
cones become permanently uncacheable and cross-root sharing vanishes),
and the counter is carried through `fresh_inner_for_eager_body` /
`drain_inner_context` so an eager HOF body cannot re-issue an epoch the
outer context already spent.

### The key

**The key already exists** — call it, do not re-derive it.
`network_evaluator::eval_env_key(network_stack, node_id, decorate)` lives
next to `eval_frame_key` rather than inside the profiler precisely so
this design can use it without depending on the profiler being on.

```
MemoKey = eval_env_key(stack, node_id, decorate)
        = hash( for each frame: (is_zone_body, network address?, frame node_id, env_epoch),
                node_id,
                decorate )
```

**The network address is hashed only for registry-owned frames** (root
and custom-network instance entries), never for zone-body frames. A
retained key may not hash an address that can be freed and reused, and
both kinds of body network *are* dropped mid-pass (`zone_closure` pushes
a locally constructed body; closure bodies are `Arc`s). A registry-owned
network is borrowed from the registry, which outlives the pass. A body
frame needs no address: it is determined by the enclosing frames plus its
owner node id. Full frame-identity table: `design_eval_profiling.md` D9.

Note what is *absent*: the output pin index (D2) and any hash of input
values. Values never need hashing because the environment determines
them.

**Width: 128 bits** (`EvalEnvKey = u128`, two domain-separated
`DefaultHasher` digests concatenated), because a collision here returns a
*wrong value* where the re-entrancy guard's would only produce a spurious
error. The memo therefore needs no verify-on-hit layer and no retained
key material.

### Instrumentation, as built

Everything this design keys on and measures against already exists:

| what | where |
|---|---|
| `env_epoch`, allocated only at `run_closure_once` | `NetworkStackElement::body_invocation` |
| the per-pass counter, carried through the eager-body split | `NetworkEvaluationContext::alloc_env_epoch`, `fresh_inner_for_eager_body`, `drain_inner_context` |
| the key, 128-bit | `network_evaluator::eval_env_key` |
| per-node `lookups` / `evaluations` / `distinct_envs` / `wasted_ns` | `evaluator/eval_profiler.rs`, Redundancy tab |
| would-be memo peak entry count | `EvalProfile::total_distinct_envs` |
| rows the memo must not cache, already flagged | `RecordFlags::{produced_iterator, under_reentrancy_backstop}` |
| equal-key ⇒ equal-result validation | `EvalProfile::self_check_violations`, panel toggle |

The flags being pre-computed is what lets D4 and D9 add no new detection
logic, and the acceptance criterion is readable off the Redundancy tab
before and after the change.

### Exclusions and the direction of safety

Given this key, "same environment ⇒ same result" holds except for three
cases, each decided below: **re-entrancy** (D9 — the one genuine
remaining hole), **effects** (D5 — same result, different side effects),
and **iterators** (D4 — a memory question, not a soundness one).

Over-keying costs a missed hit; under-keying returns a wrong value. When
a future change makes it unclear whether something belongs in the key,
**put it in the key.** `doc/design_eval_profiling.md` D11 is the
empirical backstop, and it would have caught both the `decorate` omission
and the `NodeRef` collision.

### The user-facing cost model

The key projects into a mental model simple enough to teach, and this
design is only finished when the model is true:

> **A node's result is computed once per call stack, where the call
> stack is extended with the iteration index of each enclosing HOF.**

With no HOFs and no subnetworks that collapses to "every node is computed
once per refresh" — what users already assume, and false today. The model
is compositional: a body can be reasoned about without knowing what
encloses it.

It yields one actionable optimization, narrower than "hoist work out of
loops" because the runtime already hoists part of it:

| Where an expensive node sits | Runs |
|---|---|
| Outside a body, feeding it through a capture wire | once per invocation — already true today (`build_captures`) |
| **Inside a body, but loop-invariant** (all inputs are captures) | **once per element — move it out of the body and it runs once** |
| Inside a body, reading the iteration value | once per element; nothing to do |

The middle row is the whole advice. The third is not a limitation users
can work around: a node reading the iteration value cannot be hoisted,
because a zone-input wire cannot reach outside its body.

Two caveats, or the model over-promises: **two instances of the same
subnetwork compute twice** (different call stacks, correctly so — without
this the model invites "wrap it in a subnetwork and it will be shared",
which is backwards), and **iterators are the exception** (D4), where a
lazy body's cost lands on whoever pulls it.

**Deliverable:** a "Cost model" subsection in
`doc/reference_guide/node_networks.md`, shipping **with Phase 3** — today
only the capture row is true.

## Non-goals

- **Cross-refresh caching.** Requires dependency-based invalidation and
  dirty tracking, neither of which the per-pass lifetime needs: entries
  cannot go stale inside a pass. (A memory budget it *does* need — D6 and
  D11 — but a transient per-pass one, not a resident set to manage.)
  Incremental evaluation is a separate design.
- **Changing walker semantics** or the zone capture cache.
- **Background evaluation** (`doc/design_background_evaluation.md`).
- **Measuring the win** — `doc/design_eval_profiling.md`.

## Design decisions

### D1. Per-pass lifetime — no invalidation problem exists

Created at the start of a refresh pass, shared across the entire
displayed-roots loop, dropped at the end. Network data cannot change
mid-pass, so entries never go stale. It must NOT be cleared by
`generate_scene_scoped`'s per-root scratch reset — sharing across roots is
where the largest win comes from.

### D2. Key: the environment; value: the whole `EvalOutput`

The **value is the complete `EvalOutput`**, not one pin's
`NetworkResult`. `evaluate` already runs `eval` once and computes every
pin, so one entry per (environment, node) also removes the multi-pin
redundancy and keeps `display_results`, `pin_subtitles` and
`unit_cell_override` intact. `evaluate` serves a hit through
`eval_output.get(pin)`; `evaluate_all_outputs` serves it whole.

**Insert from two of the three arms only.** Because the key omits the pin
index, an entry must be the *complete* output — and `evaluate`'s
**custom-network arm** never has one: it forwards a single
`output_pin_index` to the child's return node and gets one
`NetworkResult` back. Wrapping that in a one-pin `EvalOutput` means the
next `evaluate_all_outputs` on that instance, or the next `evaluate` for
a different pin, is served a truncated output under a key that claims to
be complete.

So insert from `evaluate_all_outputs` and from `evaluate`'s **built-in**
arm (where `eval_output` is in hand before the `.get(pin)` projection);
**not** from the custom-network arm, where the complete output exists one
level down in the child's return node. Reads may happen in all three.
This exact trap caught the profiler's self-check in
`doc/design_eval_profiling.md` Phase 3. Regression
test: *a two-output custom-network instance consumed on both pins returns
both correctly*.

`decorate` is in the key rather than a reason to skip caching: the
selected node is evaluated once with `decorate=true` (as its own scene
root) and repeatedly with `false` (as an input). Both are worth
memoizing — just not against each other.

### D3. Zone bodies are memoized, keyed by epoch

A body-local entry is keyed to its iteration and cannot leak into the
next, so no blanket exclusion is needed. Be honest about the size of the
win: entries created inside iteration N are reusable only *within*
iteration N, so this buys fan-out inside a body, not reuse across
elements, and bodies are small on today's designs.

Per-iteration entries are dead the moment their body frame pops. Evict
them there — keep a per-epoch list of inserted keys and drop it on pop —
so a 10⁵-element `map` does not accumulate 10⁵ generations before the LRU
notices.

### D4. Iterators are excluded for memory, not for correctness

Not for the intuitive reason: `Walker` guarantees Invariant 2
(`evaluator/iterator_walker.rs:9-14`) that clones are independent, so two
consumers would each get a stream from position 0 — today's semantics.

The exclusion survives for two weaker reasons. Memoizing buys almost
nothing (a `map`'s `eval` only *builds* a walker; the work happens in
`next()`, which the memo does not touch), and a stored walker pins its
`ZoneClosure` — possibly a large `Arc<Vec<NetworkResult>>` — for the whole
pass.

So: **do not store `NetworkResult::Iterator` results**. The profiler
already flags the top-level case (`RecordFlags::produced_iterator`,
rendered as `iterator` with `—` under *Wasted*), but that flag is not
sufficient as the memo's exclusion test — an iterator nested inside an
`Array` or `Record` escapes it. See D6 R4.

### D5. Effects fire once per pass (accepted change)

Today a `print` node with fan-out 2 prints twice per display pass;
memoized it prints once. This is the more correct semantics ("one
evaluation per pass") and applies equally under Execute. The Unit-skip
rule (`:2200`) is unaffected — skipped Unit outputs need no entry.

The same applies to the six interior-mutability `NodeData` fields: all
write-only, so results are unaffected, but the memo makes the *first*
write win where the *last* one used to. **Task:** confirm per field that
first-write and last-write agree, and add a regression test for
`atom_edit`'s `get_cached_input()` and `get_subtitle()`.

**`selected_node_eval_cache` is the same hazard and easier to miss**,
because the environment table files it under "written, never read to form
a result" — true of *results*, false of *behaviour*. Sixteen node types
write it from inside `eval` (`atom_edit`, `structure_move`, `relax`,
`facet_shell`, …); `generate_scene_scoped` takes it into the root's
`NodeSceneData`; the gadget layer reads it back via
`get_selected_node_eval_cache()`. A memo hit skips `eval` and so skips
the write — an empty gadget cache, invisible to any result-comparing
test.

It is safe today, but only via an invariant stated nowhere:

> `decorate` is `true` **only** for a displayed root's own
> `evaluate_all_outputs` — every wire pull and both custom-network arms
> pass `false`. The gadget wants the *active* node's cache, and every
> selection path in `node_network.rs` keeps `active_node_id` inside
> `selected_node_ids`. So the active node's own root evaluation is the
> unique `decorate = true` evaluation of that node in the pass and can
> never be served from the memo.

Write it down as a test (*a gadget-bearing node upstream of another
displayed node still has its eval cache*), because the invariant lives in
selection code that has no reason to know the evaluator depends on it.
Add "activate without selecting" later and gadgets break — only *after*
this design ships, which is the worst possible attribution.

### D6. Memory: a bounded LRU, not a fan-out precompute

Reuse `atomcad_util::memory_bounded_lru_cache::MemoryBoundedLruCache`,
which already backs the CSG cache (`csg_cache.rs:83`) and the
invisible-node cache. An evicted entry is simply recomputed, so memory is
bounded **by construction**.

**Budget: 1 GB, and a user preference rather than a constant** — see
D11, which also brings the three cache budgets that are hardcoded today
into the same preferences section. Phase 5 tunes the *policy* if a real
pass still evicts at that size.

#### The size estimator

That cache takes `size_estimator: fn(&V) -> usize`, which under D2's value
type is `fn(&EvalOutput) -> usize`, and **no such estimator exists today**.
It is not a formality — a single `NetworkResult` can hold a million-atom
`AtomicStructure` — and it is on the memo's critical path: the cache
cannot be constructed without it. Phase 2 is exactly this work, and half
of it is already built.

**What exists.** `atomcad_util::memory_size_estimator::MemorySizeEstimator`
is the project's estimator trait, and the two heavyweight payloads already
implement it — `AtomicStructure` and `GeoNode` — which between them cover
`Molecule`, `Crystal`, `Blueprint` and `Geometry2D`. Three worked
precedents to follow: `estimate_csg_mesh_size` / `estimate_csg_sketch_size`
(`csg_cache.rs:211`, `:262`) and `NodeSceneData::estimate_memory_bytes`
(`structure_designer_scene.rs:408`).

**The work** is two impls — `MemorySizeEstimator` for `NetworkResult` and
for `EvalOutput`. Of `NetworkResult`'s ~30 variants roughly twenty are a
bare `size_of::<Self>()` (scalars, vectors, matrices, `Unit`, `None`,
`DrawingPlane`, `LatticeVecs`). `EvalOutput` is its four fields, with
`display_results` counted deeply — a decorated structure is a second full
payload, not a view of the first. Two new leaf impls are needed in
`atomcad-crystolecule`: `Structure` and `Motif`, both plain `Vec` walks.
Both crates already depend on `atomcad-util`, so no dependency edge
changes.

Four rules, each a decision rather than a detail:

**R1 — `ScalarField` gains an estimator method on the trait.**
`ScalarField(Arc<dyn ScalarField>)` cannot be measured from outside: the
trait (`crystolecule/src/field/mod.rs:184`) exposes sampling only, and the
payload is a `.cube` grid — megabytes, the largest single thing the memo
can hold. Add `fn estimate_memory_bytes(&self) -> usize` to `ScalarField`
and implement it per field kind. This is the task's only API change, and
it is not optional: without it the byte budget is blind to precisely the
payload it exists to bound.

**R2 — Recurse fully into `Array` and `Record`.** These nest arbitrarily
deep and no cheaper approximation is honest. The cost is affordable
because the estimator is called **only on insert and on eviction**
(`MemoryBoundedLruCache::insert` — on the new value, on each evicted
value, and on a replaced old value) and **never on a lookup**, so the walk
is off the path the memo exists to make fast.

**R3 — Two tiers for `Arc`-backed payloads.** Deep-count an `Arc` whose
purpose is to make cloning a *large result* cheap; pointer-count an `Arc`
whose purpose is to *share structure* with the network or with sibling
values. Concretely:

- **Deep:** `ScalarField`'s payload, via R1. One field can be megabytes,
  and several distinct fields can be live in one pass. When *k* entries
  share one field this over-counts *k*-fold; the price is evicting some
  small entries early, which is cheap.
- **Pointer (`size_of::<Arc<_>>()`):** `ZoneClosure`'s `body`, `captures`,
  `zone_output_wires` and `pre_supplied_args`, reached through
  `Function`. `body` is shared with the network itself and with every
  other closure over the same body, and `captures` is an
  `Arc<HashMap<_, NetworkResult>>` that recurses back into arbitrary
  results — deep-counting it would charge the same map once per closure
  value in the pass. `Function` therefore costs `size_of::<ZoneClosure>()`
  plus its owned `param_types` / `return_type` at `size_of` granularity.
- **Not needed at all:** `Walker`. `Iterator` results are never stored
  (D4), and R4 extends that to nested occurrences, so no walker ever
  reaches the estimator.

**R4 — the iterator exclusion must recurse, and today's detection does
not.** D4 says never store a `NetworkResult::Iterator` and points at the
profiler's `RecordFlags::produced_iterator` as pre-built detection. That
flag is **top-level only**: `eval_profiler.rs:567` is a flat
`results.iter().any(|r| matches!(r, NetworkResult::Iterator(_)))`. Nested
occurrences are representable — `contains_iterator` (`data_type.rs:987`)
recurses through `Array`, `Optional`, `Function` and
`Record::Anonymous` precisely because those shapes exist, and no guard in
`array`, `collect` or `record_construct` rejects an iterator element or
field type. No user path was found that constructs one today, but the memo
must not rest on that: a stored walker pins its `ZoneClosure` for the whole
pass, which is the hazard D4 names. So the memo's skip-insert test is a
**recursive** value-level predicate — the value-side twin of
`contains_iterator` — not the profiler flag.

Note the two containers it recurses into are `Array` and `Record`, and
**only** those. `NetworkResult` has no `Optional` variant: an
`Optional[T]` value is the `T` value itself or `None`, so it needs no arm
of its own, and `Optional[Iter[_]]` is rejected at construction anyway
(`validate_optional_inner`). `Function` needs no arm either — an iterator
cannot be captured into a closure.

**Direction of error: undercount.** Restating D6's rule with the reason it
is specific to *this* cache: the memo is per-pass, and most of what it
holds is alive elsewhere anyway (the scene, downstream results), so its
true marginal footprint is smaller than a naive sum. Over-estimating makes
the LRU evict entries that cost nothing to keep and throws away the 8.06×
this design exists to capture; under-estimating risks overshooting a budget
that is a safety net, not a contract. R3's deep-counted `ScalarField` is
the one deliberate exception, for the reason given there.

**Value type: `EvalOutput` by value, not `Arc<EvalOutput>`.** Decided in
favour of the simpler system, accepting that a hit costs a clone —
§"The measurement" flags that as one of the two things that could erode
the win, and Phase 3's measurement is what tests it. The choice is cheap
to reverse: switching to `Arc<EvalOutput>` makes the estimator a
five-line forwarder (`fn(&Arc<EvalOutput>)` calling the same function,
exactly as `estimate_arc_csg_mesh_size` does) and changes neither the key
nor any call site.

This replaces precomputing per-`(node, pin)` consumer counts: more code,
its own correctness argument, and no bound on a pass that is large for
reasons other than fan-out.

### D7. Seam: `evaluate` and `evaluate_all_outputs`

Lookup and insert go in the two functions that dispatch to
`NodeData::eval` — the same hooks the profiler uses, so the two agree by
construction on what counts as an evaluation, and the network stack the
key needs is already in hand.

**Put the table in a pass thread-local, not on `NetworkEvaluationContext`.**
This is the rule in `evaluator/AGENTS.md`: per-pass state belongs either
in the pass thread-local or in **both** `fresh_inner_for_eager_body` and
`drain_inner_context`, never on the context alone. A context-owned memo
would hand every eager-HOF body (`apply`, `fold`, `foreach`) a fresh empty
table that `drain_inner_context` then discards — bodies would memoize
nothing, and it would look like a tuning problem rather than a wiring
bug. Installed and taken by `with_eval_context` next to `EvalProfile`, the
sharing is automatic; it is sound across the split precisely because
`env_epoch` is in the key.

On a hit the producing node's `eval` is skipped, so no new `node_errors` /
`node_output_strings` insert happens for it in the *current root's*
snapshot. That is already correct: the entries were recorded under the
node's own `NodeRef` when it was first evaluated, and `get_node_error` /
`get_node_output_strings` scan all snapshots
(`structure_designer_scene.rs:280`, `:268`).

### D8. Interaction with error management (origin links)

`doc/design_error_management.md` D7 records `consumer → source` origin
links whenever a resolved wire value is an `Error`. That happens in
`evaluate_arg`, *outside* the memo seam, so it should fire on hits
automatically — a cached upstream `Error` still links its second consumer
to the root cause. Verify with a test rather than assuming.

### D9. Never memoize a result produced under re-entrancy

When `context.eval_in_progress.insert(frame_key)` returns `false`, the
synthesized cycle `Error` must be returned **without** being inserted, and
the enclosing evaluation of the same environment must still insert its own
real result. Otherwise the inner `Error` is stored first and served to
every later consumer.

This is the one case where the key is genuinely insufficient: with a cycle
`A → B → A` the inner and outer evaluations of A share a byte-identical
stack, involve no body frames, and return different results. A backstop on
a backstop — but one that fails silently.

Detection already exists: the profiler calls
`eval_profiler::note_reentrancy_backstop` from both cycle arms. The memo's
rule is narrower — **skip the insert on the arm that synthesizes the cycle
error**, the same arm that raises the flag.

### D10. The memo has an off switch, and the profiler is where it lives

The memo is the one change in this design that can turn a correct network
into a wrong one *silently*: a stale or over-shared entry produces a
plausible value, not a crash. The only way to answer "is this a memo bug
or is my network wrong?" is to recompute the same design without the memo
and compare — which has to be one click, in the same session, on the state
that provoked it. A rebuild behind a cargo feature is a multi-minute round
trip on this machine and loses that state (`design_eval_profiling.md` D2).

**The switch.** Session state on `StructureDesigner`, mirrored on
`StructureDesignerModel`, toggled from the *View* menu and from the
profiler panel header next to *Per-node* — the same lifetime and plumbing
as `evalProfilingEnabled`. Three deliberate differences from that flag:

- **Default on.** It is the product's behaviour; the profiler's is not.
- **Not persisted — for the opposite reason.** Profiling must not
  silently stay *on* across sessions and skew later measurements; the memo
  must not silently stay *off*, because a session quietly running 8×
  slower forever is the worse failure. Session-scoped, resets to on.
- **Toggling forces a full refresh**, on the *Profile full refresh* path.
  A per-pass memo only shows its effect on the next pass, and comparing a
  memo-off partial against a memo-on full measures nothing.

**Memo-off must be visible outside the profiler panel.** The refresh strip
(`refresh_profile_strip.dart`) is always on screen and the profiler panel
usually is not; an off memo belongs there as a marker, or the maintainer
will eventually spend an afternoon on a performance regression they
switched on themselves.

#### Self-check: gated on the memo being off, not auto-forcing it

`design_eval_profiling.md` D11 already owns the fact — the check retains
each key's first result and compares later results under the same key, so
once the memo serves the second request *from* the first result there is
no second computation and the check passes vacuously. D11 proposed that
arming the check **force** the memo off for that pass.

**Use a hard gate instead:** the self-check can be armed only while the
memo is off. With the memo on, the control is inert and its status line
says why and points at the memo switch.

The reason is that auto-forcing makes one switch have two effects, and the
second one is invisible: the pass's *Self*, *Total* and *Phases* numbers
would silently become memo-off numbers — a profile 8× slower than the
product for a reason recorded nowhere in the row, sitting in the same
history ring as comparable ones. Arming a correctness check must not
quietly invalidate the measurement next to it. The gate makes the coupling
explicit and costs the user one extra click on a workflow that is already
deliberate.

The gate needs a rule for the other direction too, or it has a hole:
**switching the memo on while the self-check is armed disarms the
check**, with a line saying so. The asymmetry is deliberate — the memo is
the product's behaviour and a diagnostic must not block it, while a
self-check left silently armed under a memo is exactly the vacuous green
the gate exists to prevent. Refusing to enable the memo instead would be
the wrong way round.

The panel's existing green-state copy already asserts this — "(no memo is
running, so this is a real test)" (`profiler_panel.dart:699`). Today it is
true by absence; after Phase 3 it is true only because of the gate, so it
should name the gate rather than read as an assumption.

#### The Redundancy tab keeps its numbers and changes tense

Both formulas survive the memo unchanged, but two of them change meaning,
and the tab has to say which:

- **`redundancy_factor = lookups / distinct_envs`**
  (`eval_profiler.rs:183`) is **unchanged by the memo on purpose** — it
  measures demand, not waste. `materialize#8` still reads 12.0× with the
  memo working perfectly. This is D10 of the profiling doc holding: a
  factor that collapsed would be a factor defined over the wrong thing.
- **`wasted_ns = self_ns × (lookups − distinct_envs) / evaluations`**
  (`:169`) is **numerically unchanged too**, which is easy to misread as a
  bug. `self_ns` now accumulates over one evaluation instead of twelve,
  and the division by `evaluations` restores the same per-computation
  mean, so `materialize#8` reports ~5767 ms before *and* after. What flips
  is the tense: with the memo off it is the saving **available**, with the
  memo on it is the saving **realized**. Label the column **Wasted** when
  the memo is off and **Saved** when it is on — same number, honest word.
  (`wasted_ns`'s doc comment currently claims it reaches zero once the
  memo works. It does not — `lookups` do not fall when evaluations do —
  and that sentence must be corrected in the same change.)
- **`evaluations` against `distinct_envs`** is the actual regression test,
  and the reader should not have to diff two columns to run it. Put the
  count of unflagged rows where `evaluations > distinct_envs` in the
  footnote. With the memo on it reads zero, and that single number *is*
  this design's acceptance criterion.

#### Two new reasons a row is legitimately not memoized

The Note column carries two values today — `iterator` (D4) and `cycle`
(D9) — and both are pre-computed (`RecordFlags`). The memo adds two more,
and neither is optional, because an unflagged row that re-evaluates reads
as a memo failure:

- **`subnetwork`** — a custom-network instance node. D2 forbids inserting
  from that arm, so such a row shows `evaluations == lookups` permanently.
  Unflagged, **every subnetwork instance in every design reads as a memo
  bug**, including `geo.1-precursor_proxy#6` in this design's own
  measurement (12 lookups, 1 env, 5.8 ms). That its cost is small — the
  expensive work re-enters and hits at the child's return node — is
  exactly why it must be flagged rather than fixed.
- **`evicted`** — the LRU dropped the entry and it was recomputed (D6).
  Needs a per-node counter of misses on a key that was previously
  inserted. Without it, memory pressure is indistinguishable from a
  correctness bug, and Phase 5's trigger ("problematic peaks despite the
  LRU") has no signal to fire on.

#### Memo statistics, next to the CSG cache statistics

`design_eval_profiling.md` D12 puts `get_csg_cache_stats` hit/miss counts
beside the phase totals rather than folding them into node time, because
what those counters add is *why two otherwise identical refreshes differ*.
The memo earns the same slot, and for the same reason.

**They are per-refresh counters, and always on.** This follows D1 of the
profiling doc — two clocks with different lifetimes. Phase timing is
always on because a handful of `Instant::now()` calls are unmeasurable;
memo counters are the same shape (a few increments and one `max` per
insert), so they belong with the always-on strip and **not** behind the
per-node toggle. That matters practically: the per-node profiler inflates
the timings it reports, and someone chasing a memory number should not
have to distort the time numbers to see it.

**Unlike the CSG cache, the memo does not exist when the panel renders.**
The CSG cache persists across refreshes and can be queried at any moment;
the memo is created and dropped inside one pass (D1). There is therefore
no `get_memo_stats()` — the counters must be **harvested into
`RefreshProfile` before the table is dropped**, at the same point
`with_eval_context` takes the `EvalProfile`. A stats API that reads a live
memo would return zeroes every time it was called.

`APIRefreshProfile` gains a `memo: APIMemoCounts` field beside
`csg_cache`, so every row of the history ring carries its own — which is
what makes the D10 A/B comparison readable: two rows, one memo-off, one
memo-on, each with its own numbers. The **domain** side (collecting the
counters and harvesting them into `RefreshProfile`) is Phase 3, because
the D3 eviction test asserts against them; the **API twin and the panel
block** are Phase 4.

| Counter | The question it answers |
|---|---|
| **peak entries** | How many distinct environments were live at once. Compare against `EvalProfile::total_distinct_envs`, which *predicts* it. |
| **peak bytes / budget** | Is the pass anywhere near the ceiling? The number Phase 5 tunes against, and the one this question is really about. |
| **end bytes** | How much of the peak was transient. A peak far above the ending size means D3's epoch eviction is doing its job. |
| **hits / misses** | Is the memo doing anything at all on this design. |
| **LRU evictions** | The budget was too small and work was recomputed. |
| **epoch drops** (D3) | Entries retired because their body iteration ended. |
| **declined inserts** | The deliberate exclusions (D2 subnetwork arm, D4 iterators, D9 re-entrancy) firing, as a total. |

**Peak, not final, and the two kinds of removal must be separated.**
Neither is available today: `MemoryBoundedLruCache` tracks
`current_memory_bytes` and `len()` but has no high-water mark and no
eviction counter. Add both to the cache itself rather than to the memo —
one comparison and one increment per insert, and the CSG cache and the
invisible-node cache get the same visibility for free.

Separating **LRU evictions** from **epoch drops** is the part that is easy
to skip and expensive to skip. Both remove entries and both show up as
later misses, but they mean opposite things: an epoch drop is D3 working
as designed, an LRU eviction is the budget being too small. Collapsed into
one "removals" number, the single signal Phase 5 is supposed to fire on
becomes unreadable — and a design with a large `map` will always show
plenty of removals.

**Predicted against actual peak entries is a free check on the key.** The
profiler already computes `total_distinct_envs` from the same key the memo
uses, so the two should agree closely (the memo's peak is lower by
whatever D3 retired and D6 evicted, never higher). A large unexplained gap
means the memo and the profiler are keying on different things — the
failure mode D9's frame-identity table exists to prevent, caught here
without needing the self-check.

One optional counter, worth adding only if it stays quiet: **time spent in
the size estimator**. D6 R2 recurses fully on every insert and eviction,
and if that ever became significant it would erode the win invisibly —
charged to the eval phase, attributable to nothing. A single accumulated
duration rules it out at a glance. Drop it if it reads as noise.

#### The history ring must tag memo state

The Phases tab tags each entry `Full` / `Partial` / `Light`. The whole A/B
workflow this decision exists to support — profile with the memo off,
toggle, profile again, compare the two rows — is unreadable if the two
rows are indistinguishable. `APIRefreshProfile` gains a memo-on flag and
the ring renders it next to the mode.

A one-click "profile both ways and diff" button is **not** part of this:
two runs and a visible tag are enough, and a comparison UI would have to
decide what counts as a meaningful difference, which is the maintainer's
judgement rather than the panel's.

### D11. Cache budgets are user preferences, in one place

**The memo's budget is 1 GB.** 256 MB — the number the invisible-node
cache uses — is a small allowance on a modern workstation, and small
against what this cache actually holds: a single million-atom
`AtomicStructure` runs to tens or hundreds of megabytes, so a handful of
large results can reach 256 MB while the entry count still looks trivial
(145 in the measurement). The memo is also per-pass, so the allocation is
transient by construction — this is not a resident-set commitment.

That number should not be a constant in the source, and neither should
the ones already there. Today three cache budgets are hardcoded in three
files, none of them reachable by the person whose machine they are sized
for:

| Cache | Today | Where |
|---|---|---|
| CSG mesh conversion | 200 MB | `csg_cache.rs:111` (`with_defaults`) |
| CSG sketch conversion | 56 MB | `csg_cache.rs:112` |
| Invisible-node scene cache | 256 MB | `structure_designer_scene.rs:210` |
| **Evaluation memo** | **1 GB** | this design |

**Deliverable: a "Memory" section in the preferences window** holding all
four (arriving over two phases — see *Phasing* below), expressed in
**megabytes**: bytes are the wrong unit for a person, and a `u32` of MB
cannot overflow a `usize` of bytes on any target we build for. It follows
the existing preference machinery exactly — a `MemoryPreferences` struct
in `rust/crates/atomcad-structure-designer/src/preferences.rs` with
`#[serde(default)]` per field (the tolerant-reader contract), a same-named
twin in `rust/src/api/structure_designer/structure_designer_preferences.rs`
with `From` impls both ways and `#[frb(non_final)]` on each field, and a
grey section in `lib/structure_designer/preferences_window.dart` with
`IntInput` rows and `PreferencesKeys` entries, like *Simulation*.

Three things worth settling here rather than at the keyboard:

- **Applying a change must not need a restart.**
  `MemoryBoundedLruCache::resize()` already exists and evicts down to a
  new smaller limit, so the cache layer is ready. What is missing is
  pass-through: `CsgConversionCache` and `StructureDesignerScene` need
  setters, and `StructureDesigner::set_preferences`
  (`structure_designer.rs:7315`) is the hook that calls them — the same
  place node-display and geometry-visualization changes are already
  applied. The budgets must also be applied in `StructureDesigner::new()`,
  which loads the persisted preferences; otherwise a saved budget only
  takes effect once the user opens the dialog.
- **A small budget must degrade, not break.** `insert` already inserts an
  over-budget value anyway once the cache is empty, so a tiny budget
  turns a cache into a pass-through rather than a failure. Clamp to a
  sane floor in the UI regardless, and say in the tooltip that lowering a
  budget costs recomputation rather than correctness — which is true of
  every cache here, and is the reassurance that makes the setting safe to
  offer at all.
- **Preferences are not undoable**, unlike document state. These are app
  settings and follow the existing preference path, which is persisted on
  change and has no undo command. No `UndoCommand` is needed or wanted.

**Phasing.** Phase 2 ships the section with the **three existing**
budgets; Phase 3 adds the memo row when there is a memo to budget. That
ordering exists to avoid shipping a control that does nothing — the wart
would be small but it is entirely avoidable, and the three real budgets
become adjustable a phase earlier as a side effect.

One consequence to keep in view: at 1 GB the Phase 5 trigger (evictions
above zero) becomes correspondingly less likely to fire, which is the
intent — but it also means that if it *does* fire, the answer is no longer
"raise the budget", because the user can now do that themselves. It is a
signal about the policy, not the number.

## Phases

The memo itself is one indivisible change — the key, the seam and the
lifetime only work together — so the split is not *of* the memo but
*around* it: everything that can be built and tested before the evaluator
changes goes first (Phase 2), everything that is only presentation goes
last (Phase 4). Phase 3 is the risky one, and it is deliberately left with
nothing to invent.

### Phase 1 — Instrumentation. Done

Delivered by `doc/design_eval_profiling.md` Phase 3, and the gating
measurement is taken (§"The measurement": 8.06× overall, 12.0× on the node
holding 95% of the evaluation time, ~6033 ms projected saving, clean
self-check). **Phase 2 may start.**

Optional, non-blocking: run the self-check clean on one or two further
designs to broaden the evidence that the key is not under-split.

### Phase 2 — Sizing, exclusion, and budgets (D6, D11)

Everything the memo needs that is a **pure function**: the
`MemorySizeEstimator` impls for `NetworkResult` and `EvalOutput`, the
`Structure` / `Motif` leaves, the `ScalarField` trait method (R1), the
recursive iterator predicate (R4), and the high-water mark plus eviction
counter added to `MemoryBoundedLruCache` itself.

Plus the **Memory preferences section** (D11) carrying the three cache
budgets that are hardcoded today, and the pass-through setters that let a
change apply without a restart. The memo's own row waits for Phase 3, so
this phase ships no control that does nothing. Being user-visible, it
also ships a *Memory* subsection in `doc/reference_guide/ui.md` under
"Preferences Dialog", beside *Simulation*.

Nothing here can change a single *evaluation result*: the estimator and
the predicate have no callers outside tests, and the preferences section
touches caches this design does not own. It is separated for exactly that
reason — about a third of the work and none of the risk — and doing it
first means Phase 3 is a change to the evaluator and nothing else.

#### Tests

- **Relations, not thresholds** (the numbers are machine-independent only
  as relations): a structure-bearing `EvalOutput` sizes above a scalar
  one; a 1000-atom structure sizes above a 2-atom one; an `Array` of *n*
  identical elements sizes at roughly *n* times one of them.
- **R3, the two tiers.** A `Function` value sizes at pointer cost
  regardless of how large its captures are — build two closures over the
  same body with very different capture maps and assert their sizes agree
  within the `param_types` difference. A `ScalarField` result, by
  contrast, sizes *with* its grid.
- **R2 recursion terminates and counts**: a nested `Record` inside an
  `Array` inside a `Record` sizes above the sum of its scalar leaves.
- **R4 is recursive where the profiler flag is not**: the predicate says
  "do not store" for a bare `Iterator`, for `Array([Iterator, …])`, and
  for a `Record` with an iterator-valued field. This is the test that
  distinguishes the new predicate from `RecordFlags::produced_iterator`;
  without it the two look interchangeable and the recursion gets
  optimized away by a later reader.
- **Cache instrumentation**: the high-water mark never decreases within a
  cache's life and is always `>= current_memory_bytes`; the eviction
  counter increments on an LRU eviction caused by the budget and **not**
  on an explicit `pop`, `clear`, or a same-key replacement. The existing
  CSG-cache tests stay green — this phase touches a type two other caches
  already use.
- **Budgets are live and tolerant** (D11): lowering a budget through
  `set_preferences` evicts down to it immediately rather than waiting for
  the next insert; a budget below one entry's size degrades to a
  pass-through instead of failing; a `preferences.json` written before
  this phase loads with the documented defaults (the tolerant-reader
  contract, which is the one way a preferences change can break existing
  users).

#### Manual walkthrough

- *Edit > Preferences* shows a **Memory** section with the three budgets
  in MB. Lower the CSG mesh budget to something small, work with a
  geometry-heavy design, and confirm it stays correct and merely slower —
  the claim the tooltip makes.
- Restart and confirm the values persisted, then delete
  `preferences.json` and confirm the defaults come back.

### Phase 3 — The memo (D1–D5, D7–D10, plus D11's memo row)

The behaviour change: the pass thread-local table, lookup and insert at
the two seams, the D2 insert rule, epoch-scoped eviction, the D9
skip-on-re-entrancy rule — plus, in the same phase and not after it, the
memo switch, the self-check gate, the `subnetwork` and `evicted`
`RecordFlags`, and the memo counters harvested into `RefreshProfile`.

Those four are not reporting polish and cannot wait for Phase 4. The
switch is how a suspected memo bug gets confirmed at all; the gate is what
stops a self-check run under the memo from reporting a vacuous green from
the first day the memo exists; the `subnetwork` flag is what makes the
acceptance criterion readable; and the counters are what the D3 eviction
test asserts against.

Phase 3 adds the memo's own row to the Memory preferences section (D11),
now that there is a memo to budget, defaulting to 1 GB.

Phase 3 also ships the reference-guide "Cost model" subsection
(§"The user-facing cost model"). The feature is not done until the guide
states the contract the memo now honours — before this phase only the
capture row of that table is true.

#### Tests

Sharing, and its limits:

- A diamond evaluates the apex **once**, and the two consumers receive
  equal results.
- A displayed node upstream of another displayed node evaluates once per
  pass — the cross-root sharing D1 exists for, and the largest single win
  in the measurement.
- A two-output node consumed on both pins evaluates once (D2 — this is
  redundancy `evaluate` has today for free).
- A two-output **custom-network instance** consumed on both pins returns
  both pins correctly (D2's insert rule; the trap that caught the
  profiler's self-check in `design_eval_profiling.md` Phase 3).
- Two instances of one custom network do **not** share results.
- A `parameter`-excursion graph does not cross-contaminate a parent and a
  child node that share an id — the `NodeRef`-is-not-a-frame-identity
  hazard, which for a memo is a wrong value rather than a spurious error.
- **The eager-HOF split (D7).** A diamond *inside* a `fold` body
  evaluates its apex once per iteration. This is the test that proves the
  table is in the pass thread-local rather than on
  `NetworkEvaluationContext`: context-owned, the body would get a fresh
  empty table that `drain_inner_context` discards, and the symptom is a
  memo that silently does nothing inside every eager HOF. Repeat for
  `apply` and `foreach`, which take the same path.

Bodies and iterators:

- A `map` over 3 elements still evaluates its body 3 times (D3) — the
  epoch in the key, doing its job.
- **Epoch-scoped eviction (D3):** a `map` over many elements holds a peak
  entry count that does not grow with the element count. Asserted against
  the memo counters this phase adds, as a relation (peak entries under a
  1000-element
  map is within a small factor of peak entries under a 10-element one),
  never a byte threshold.
- Iterator fan-out still yields independent walkers, and the existing
  iterator suite is green **unchanged** (D4 — Walker Invariant 2 is not
  this design's to renegotiate).
- An `Array` whose elements are iterators is not stored (D6 R4, now
  wired).

Effects and side channels:

- `print` with fan-out 2 fires **once** per pass (D5 — a deliberate
  semantic change; update the expectations rather than working around
  them).
- A gadget-bearing node upstream of another displayed node still has its
  `selected_node_eval_cache` populated (D5). The invariant this rests on
  lives in selection code that has no reason to know the evaluator
  depends on it, which is why it needs a test and not a comment.
- `atom_edit`'s `get_cached_input()` and `get_subtitle()` return the same
  values with the memo on as with it off (D5's first-write-vs-last-write
  question, for the two interior-mutability fields that are read back by
  the UI). Confirm the remaining four fields by inspection in the same
  change.
- **Origin links survive a hit (D8).** A cached upstream `Error` consumed
  by a second consumer still records that consumer's `consumer → source`
  link. `evaluate_arg` is outside the memo seam so this *should* be
  automatic — which is the reason to assert it rather than reason about
  it.

Cycles, the switch, and the criterion:

- A cyclic graph's non-cyclic consumers still get real values (D9): the
  synthesized cycle `Error` is never inserted, and the enclosing
  evaluation of the same environment inserts its own real result.
- **The A/B, run once as a test:** a fixture design evaluated with the
  memo off and with it on produces identical node output strings. This is
  the switch's whole purpose, executed automatically so it cannot rot.
- Arming the self-check is refused while the memo is on, and switching
  the memo on disarms an armed self-check (D10 — the two halves of the
  gate; without the second, a pass can run memoized with the check
  silently armed and report a vacuous green).
- A custom-network instance row carries the `subnetwork` flag and is
  excluded from the `evaluations > distinct_envs` count; an
  eviction-forced recomputation carries `evicted`.
- Memo counters survive into `RefreshProfile` after the table is dropped,
  and a `map` body's retired entries are reported as **epoch drops**, not
  LRU evictions.

#### Manual walkthrough

- Open `SPM-tip-with-tool_2026-08-20_12-52-CET.cnnd`, network
  `0_pecursor_edit_sequence_H&Cl`, press **Profile full refresh**: the
  eval phase falls from ~6.6 s to roughly 0.6 s, and `materialize#8`
  reads `lookups = 12, evaluations = 1`.
- **The A/B by hand, which is the point of the switch:** toggle the memo
  off, refresh again — the time returns to ~6.6 s and *the structure in
  the viewport is unchanged*. Toggle back on. Anything visible moving
  between those two refreshes is a memo bug, and this is the loop for
  finding it.
- With the memo on, try to arm the self-check → refused, with the reason
  and a pointer to the memo switch. Switch the memo off → it arms; run a
  profiled pass → clean.
- Work normally for a few minutes with the memo **off** and confirm the
  memo-off marker in the refresh strip is noticeable enough to be
  believed later, when it is the explanation for "everything got slow".
- Walk one design with a `map` over a large collection and one with
  nested subnetworks — the two shapes whose cost model the reference
  guide now promises — and check the guide's claims against what the
  Redundancy tab reports.

The Flutter smoke test (`flutter test integration_test/`) is a **pending
manual step for the maintainer**; it is not run as part of this phase's
automated verification.

### Phase 4 — Reading the result (D10, presentation only)

What is left of D10 once the counters and flags exist: the
`Wasted`/`Saved` relabel keyed on the memo switch, the memo statistics
block beside the CSG cache counts, the memo tag on history-ring rows, and
the footnote count of unflagged rows where `evaluations > distinct_envs`.

No evaluator code changes here. Split off because none of it can be
designed properly until Phase 3 has produced one real profile to look at,
and because a panel change that lands with a semantics change is a panel
change nobody reviews.

#### Tests

- The relabel follows the switch, not the data: the same profile renders
  the column as *Wasted* with the memo off and *Saved* with it on.
- The footnote's offender count excludes `iterator`, `cycle`,
  `subnetwork` and `evicted` rows, and reads **zero** on a memoized
  fixture. This number is the acceptance criterion; a test that it is
  computed over the right population is a test of the criterion itself.
- A history ring holding one memo-off and one memo-on row renders them
  distinguishably.
- Predicted (`total_distinct_envs`) against actual peak entries is
  rendered, and actual never exceeds predicted on a fixture.

#### Manual walkthrough

- Profile the heavy design twice, once each way, and read the two rows
  side by side in the ring — the comparison this phase exists to make
  legible.
- Check the memory block against the budget on the heaviest available
  design; if peak bytes is nowhere near the ceiling and evictions are
  zero, Phase 5 is not needed and should not be done.

### Phase 5 — Memory tuning (conditional)

Only if Phase 4's numbers show problematic peaks despite the 1 GB budget
(D11) and epoch-scoped eviction. **Any eviction at all on an ordinary
design is the trigger**; anything else is speculation. Note that this is
not far-fetched on the heaviest designs: the working set is small in
*entries* (145 in the measurement) but a single million-atom
`AtomicStructure` is tens to hundreds of megabytes, so a handful of large
results can reach the budget while the entry count still looks trivial.

Three moves, cheapest first. Take the first one that clears the trigger:

1. **Raise the byte budget.** The memo is per-pass and most of what it
   holds is alive elsewhere anyway (D6, "direction of error"), so the
   marginal footprint is smaller than the number suggests. The user can
   already do this themselves (D11), so reaching for it here is an
   admission that the default is wrong, not a fix.
2. **Evict largest first, instead of least-recently-used.** Free — the
   estimator already provides size — and it removes the specific
   pathology, which is discarding a small, hugely expensive entry to make
   room for a large cheap one.
3. **Cost-per-byte eviction** (GreedyDual-Size in the caching
   literature): time the computation on the miss path and evict the
   lowest recomputation-cost-per-byte entry.

**Prefer these over reintroducing fan-out counting**, which was rejected
in D6 for reasons that still hold. And prefer them over
LRU-with-a-bigger-budget for a reason worth stating: **recency is a weak
signal in a memo pass.** LRU assumes temporal locality; an evaluation pass
is a topological sweep, where an entry touched early is often wanted again
at the very end — that is precisely D1's cross-root sharing, the largest
win in the measurement. When the trigger fires, the policy is the thing
that is wrong, not the number.

Two things to know before building option 3:

- **The measurement is cheap; the *right* measurement is not.** Two clock
  reads on the miss path only (hits need no timing) is microseconds per
  pass — nothing like the per-node profiler, which is expensive because
  of *attribution*, not timing. But on a DAG the recomputation cost of an
  entry is not a property of the entry: storing **total** time overstates
  it whenever a dependency is still cached (and double-counts along a
  chain), while storing **self** time understates it whenever a
  dependency was evicted too — and evictions are correlated. Self time is
  the coherent conservative choice, and it needs a child-accumulator
  stack at the same seam the memo already brackets: one `u64` per frame,
  far less than the profiler's record, but new always-on work in the
  hottest path in the application.
- **Do not derive the policy from the profiler.** Reusing the profiler's
  costs when it happens to be armed would make a profiled run evict
  differently from a real one, which breaks D10's A/B: the two runs would
  no longer be the same system.

### Acceptance criterion

Stated once, in the profiler's own terms, and measured at the end of
Phase 3:

Every node not flagged as deliberately-uncached reports
`evaluations == distinct_envs`, and the measured drop in eval-phase time
matches the predicted `wasted_ns`. Concretely: `materialize#8` reports
`lookups = 12, evaluations = 1`, and the eval phase falls from **6.63 s to
roughly 0.6 s**. Materially worse means the clone cost is real (D6's value
type is where to look); materially better means something else changed and
the comparison is invalid.

Stated over *evaluations*, not lookups — lookups stay high by design once
the memo serves them, and `wasted_ns` keeps its value while changing tense
(D10).

## Deferred / follow-ups

- Cross-refresh incremental evaluation — a separate future design; this
  memo neither helps nor hinders it.
- Memoizing iterator *elements* rather than the walker — would need
  restartable walkers and a per-element key; no current need.
- **Cost-aware eviction** — see Phase 5. Recorded here as well because it
  becomes clearly correct if cross-refresh caching ever happens: across
  refreshes the working set is unbounded, eviction is the norm rather
  than the exception, and recency is a weaker signal still. Premature by
  one or two designs, not wrong.
