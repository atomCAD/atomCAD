# Design: Error Management — unified surfacing, cone-scoped blocking, navigable chains

Errors in atomCAD come from two pipelines — **validation** (structural
checks over the whole design, analogous to compilation) and
**evaluation** (runtime failures while computing results, analogous to
running the program). Users experience both the same way: "this part is
broken, show me where." Today the two pipelines have different
surfacing, different navigation support, and wildly different blast
radius. This design unifies them behind one user-facing model and
shrinks the blast radius of validation errors from "the whole network
goes blank" to "the offending node and its downstream cone go dark."

Companion document: `doc/design_eval_memoization.md` (within-pass
evaluation memoization). The two designs are independent; the single
interaction point is called out in D7 here and D8 there.

## Motivation

Three user-reported pains drove this design:

1. **Errored nodes missing from the error list.** The user-types panel's
   error badges and the F8 next-error cycle (see
   `doc/reference_guide/ui.md`, "Where is the error?") list *validation*
   errors only. A node whose failure is a *runtime* error — missing
   input at eval time, an atom-op failure, a failed relax — shows a red
   badge on the canvas but never appears in the panel list and is
   skipped by F8.
2. **A lone unconnected node blanks the whole viewport.** Dropping a
   bare `relax` / `structure_move` / `passivate` / any of ~17
   polymorphic-output node types on the canvas — the completely normal
   "place node, then wire it" workflow — makes the entire network
   invalid and refuses evaluation of *everything*, including finished,
   unrelated parts of the graph.
3. **Errors caused by other errors are hard to trace.** A failure fans
   out downstream as chained error text; the root cause is embedded in a
   string with an ambiguous node reference, and there is no way to
   navigate to it.

## Current state (analysis)

### The two error channels

**Validation errors** live on `NodeNetwork::validation_errors`
(`node_network.rs:919`), each a `ValidationError { error_text, node_id:
Option<u64>, blocking: bool }` (`node_network.rs:50`). They are
(re)computed by `validate_network` (`network_validator.rs:697`) for the
whole design and surfaced three ways: canvas node badges
(`build_node_view`, `structure_designer_api.rs:670`), the user-types
panel error badges + F8 cycle (via
`scoped_validation_errors::collect_scoped_validation_errors` →
`get_node_networks_with_validation`), and the direct-editing banner
(`hasValidationErrors`).

**Evaluation errors** are `NetworkResult::Error(String)` values produced
during scene generation. They are recorded into
`context.node_errors: HashMap<NodeRef, String>`
(`network_evaluator.rs:160`; insert sites `1784`, `1832`, `1969`,
`1178`), snapshotted per displayed root into
`NodeSceneData.node_errors` (`network_evaluator.rs:796`), and read back
by `StructureDesignerScene::get_node_error`
(`structure_designer_scene.rs:259`). They surface **only** as canvas
node badges, and only via the fallback branch of `build_node_view`.

### Facts established by research (load-bearing for the design)

- **One error per node, recorded regardless of how the node is
  reached.** Every evaluated node — as its own displayed entry point or
  as a dependency inside another root's recursion — gets its error
  inserted under **its own** `NodeRef` (`network_evaluator.rs:1969`
  keys by the node currently being evaluated). HashMap insert =
  last-write-wins → exactly one entry per node; no duplicate display.
- **Instance keying is already sound.** Custom-network entry pushes the
  instance node id onto `eval_scope_path`
  (`network_evaluator.rs:1726`, `1947`), so two instances of the same
  network produce distinct `NodeRef` keys — no collision. Inner nodes
  of a custom network *are* recorded (keyed under the instance's scope
  path) but are never read back by the view
  (`network_evaluator.rs:1722` comment) and cannot be attributed to the
  child network's own panel row without translation.
- **Coverage boundary.** Evaluation errors exist only for nodes actually
  evaluated: displayed nodes plus their upstream cones, in the *active*
  network. A node that is neither displayed nor upstream of anything
  displayed is never evaluated and has no eval error anywhere. (The
  eventual fix for the coverage gap is
  `doc/design_background_evaluation.md`; this design just states the
  boundary honestly.)
- **The whole-network suppression gate (bug).** `build_node_view` falls
  back to the scene's eval error only when
  `node_network.validation_errors.is_empty()`
  (`structure_designer_api.rs:676`) — the **whole network's** list. One
  validation error anywhere (even a non-blocking warning) suppresses
  every eval-error badge on every canvas node in the network.
- **Blocking validation blanks everything.** `valid == false` makes
  `generate_scene_scoped` return an empty scene per displayed node
  (`network_evaluator.rs:568`), blanking the viewport and recording no
  eval errors. Custom-network eval refuses with a localized
  `Error("{name} is invalid")` (`network_evaluator.rs:1704`, `1930`) —
  note this refusal is *already* clean and localized.
- **The lone-node rule.** There is no "required pin not connected"
  validation rule; disconnected inputs are normally a clean localized
  runtime error (`input_missing_error`, `network_result.rs:1215`). The
  blanking culprit is the polymorphic-output-resolution rule
  (`network_validator.rs:655`): a `SameAsInput` output fails to resolve
  when its input is unwired → blocking → whole network invalid. Of the
  ~17 node types with polymorphic outputs, only `atom_edit` declares a
  `fallback_if_disconnected` (`node_type.rs:36`;
  `atom_edit_data.rs:2607`).
- **Short-circuit validation.** `validate_wires` and
  `validate_parameters` return on the **first** error
  (`network_validator.rs` — every push in those passes is followed by
  `return false`). Only `validate_zones_recursive` accumulates. Users
  therefore fix blocking errors one at a time, and the panel badge
  count is dishonest for blocking classes.
- **Cross-network cascade.** "References invalid node network"
  (`network_validator.rs:489`, `574`) is blocking, so network A using
  invalid network B is itself fully blanked, and
  `validate_active_network_with_initial_errors` cascades the validity
  flip upward through parents (`structure_designer.rs:7690`).
- **No intra-network wire-cycle protection (latent bug).**
  `can_connect_nodes` (`node_network.rs:1409`) does only type checking —
  no reachability test. The validator has no cycle rule (the
  `ValidationContext` memo only guards *type resolution* recursion,
  with a comment "real cycles should be rejected elsewhere",
  `network_validator.rs:47` — nothing elsewhere rejects them). The
  evaluator has no visited-set. A wire cycle, if authored, hangs or
  overflows evaluation. Custom-network *reference* cycles are rejected
  at creation (defensively re-handled in `migrate_v3_to_v4.rs:157`).
- **Chain text is lossy and ambiguous.** The chaining hub is
  `evaluate_arg` (`network_evaluator.rs:1342/1358/1381/1395` →
  `error_in_input_chained`, `network_result.rs:1228`): format
  `error in {pin} input (from {type} #{id}): {inner}`. The source
  identity (`describe_wire_source`, `network_evaluator.rs:1483`) is a
  **bare type name + numeric id** — no scope path (per-body id counters
  make bare ids ambiguous), no network name; `ZoneInput` sources carry
  no identity at all (`:1498`). The custom-network wrap
  `Error in {network}: …` (`network_evaluator.rs:1742`, `1958`) keeps
  only the type name — two instances indistinguishable.
- **Pass-through preserves error payloads.** `convert_to` is a no-op on
  `Error` (`network_result.rs:634`); walkers, eager HOF drains,
  `collect`, `apply`, array ops, and the near-universal per-node
  `if let Error = input { return }` guard all forward upstream errors
  verbatim. Known violations that *lose* the inner cause:
  `lattice_symop.rs:191`, `array.rs:298-349`,
  `atom_composediff.rs:111-180`, `apply_diff.rs:119-132`.
- **`NetworkResult` is runtime-only.** Derives `Clone, Default` only —
  no `Serialize`, no `PartialEq`; errors are never persisted to
  `.cnnd`, undo, or caches. Fewer than 10 test files match error text,
  all substring-style.
- **Third ad-hoc channel.** `motif` / `materialize` / `motif_sub`
  construct `ValidationError`s whose return value is discarded at the
  call site (`structure_designer_api.rs:7456/7546/7656`) — they surface
  only as a node-local badge string and never reach `validation_errors`
  or the panel. Only `expr` threads its parse errors into the network
  gate (as *blocking*, via `initial_errors`,
  `structure_designer.rs:4354`).

### The blocking/non-blocking model today

`blocking: true` (default, `ValidationError::new`) flips
`NodeNetwork::valid` → whole network refuses to evaluate.
`blocking: false` (`ValidationError::warning`) surfaces a badge but the
network keeps evaluating. The litmus test for choosing between them is
documented in `rust/src/structure_designer/AGENTS.md` ("Validation
errors: blocking vs non-blocking"): blocking is only justified when
evaluating would be unsafe (panic/hang) or silently wrong. The blast
radius of "blocking" is the *entire network* — this design shrinks it
to the offending node's downstream cone, which makes most of the
per-rule litmus agonizing unnecessary.

## Non-goals

- **Cross-refresh result caching / memoization** — separate document,
  `doc/design_eval_memoization.md`.
- **A debugging environment / call-stack UI** — the structured error
  payload (D10) is designed so it becomes possible later, but no
  debugger UI is in scope.
- **Background evaluation of non-displayed nodes** — separate existing
  design (`doc/design_background_evaluation.md`); this design accepts
  the evaluation coverage boundary.
- **Evaluation warnings** (e.g. relax non-convergence as amber) — the
  severity model reserves the slot (D2) but no producer is added.
- **Fixing the evaluator's redundant re-evaluation** — companion doc.

## Design decisions

### D1. One unified error list per network; source shown by icon, not color

The panel badge, the badge picker, the tooltip, and the F8 cycle all
consume **one merged list** per network: validation errors (whole
design, always fresh) + evaluation errors (see D6 for lifetime). Users
do not care which pipeline produced an error; the model they act on is
"this part is broken."

The **color channel keeps encoding severity** (red = something does not
evaluate; amber = advisory) exactly as today. The **icon encodes
source**: validation errors keep the filled circle / warning triangle;
evaluation errors use a bolt glyph (`Icons.offline_bolt` family — the
established "runtime" icon). Source icons appear in picker rows and
tooltips; the aggregated badge stays count + severity color only (too
small for a second dimension).

### D2. Severity model: two user-facing axes, one internal scope

- **Severity** (user-facing, color): **Error** (red — the node's cone
  does not evaluate) vs **Warning** (amber — advisory; everything still
  evaluates). Today's `blocking: true` maps to Error, `blocking: false`
  to Warning. The stored bool survives unchanged; its *meaning* shrinks
  from "blanks the network" to "poisons this node's cone" (D3).
- **Source** (user-facing, icon): Validation vs Evaluation (D1).
- **Effect scope** (internal, not a user concept): *advisory* (warnings
  — no evaluation effect), *cone* (node-attributed blocking validation
  errors — D3), *interface* (the residue that makes a network unusable
  as a custom node type — D5). Runtime errors are always effectively
  cone-scoped via normal error propagation.

Evaluation warnings do not exist today; the model reserves amber + bolt
for them so adding a producer later is purely additive.

### D3. Cone-scoped validation blocking (skip-and-synthesize)

Replace the whole-network evaluation refusal with per-node poisoning:

- Before dispatching a node's `eval`, the evaluator checks whether the
  current network has a **blocking** validation error attributed to
  this node (by id, in the node's own scope). If so it does **not**
  call `eval`; it synthesizes `NetworkResult::Error("<validation error
  text>")` as the node's output and records it under the node's
  `NodeRef` like any runtime error.
- Downstream consumers receive the synthesized error through the
  existing chaining machinery (`evaluate_arg`); independent nodes
  evaluate untouched. The viewport shows everything evaluable.
- `generate_scene_scoped`'s `!valid` blank (`network_evaluator.rs:568`)
  is retained **only** for the interface-level residue (D5). For
  networks whose only blocking errors are node-attributed, the scene
  generates normally with poisoned cones.
- **Why this is safe without per-rule audits:** the historical reason
  these rules block is "evaluating the broken node could panic or
  produce garbage" (e.g. type mismatch → `extract_*().unwrap()`).
  Skip-and-synthesize never enters the unsafe code path — safety comes
  from *not evaluating*, not from proving each runtime path handles the
  condition.
- **Warnings still evaluate** (unchanged): several warning rules mark
  nodes that remain partially useful (e.g. `Supplied`-but-unwired still
  displays pin 0). Skipping their eval would regress display.
- `execute_node` / CLI `evaluate_node` gates
  (`structure_designer.rs:7854`, `7756`) relax to the same residue;
  executing a poisoned cone naturally yields the synthesized `Error`.

This also fixes the cross-network cascade for free: an instance of an
invalid network is a node with a validation error → it poisons its own
cone in the parent instead of blanking the parent. The existing
localized refusal (`"{name} is invalid"`,
`network_evaluator.rs:1704/1930`) remains as the defense-in-depth
fallback.

### D4. Validation must accumulate (prerequisite for D3)

Under cone-poisoning, an error the validator **did not record** is a
node that is **not poisoned** — its eval would run against the very
condition validation was supposed to catch. `validate_wires` therefore
must stop short-circuiting the whole pass: process each node's checks
fully, record, continue to the next node. (Within one node the checks
keep their early-outs — later checks assume earlier invariants.)
`validate_parameters` accumulates where safe. Side benefit: the badge
count becomes honest and F8 has a real list for blocking classes.

**Ordering rule: D4 must land before or with D3 — never D3 alone.**

### D5. The interface-level residue (still network-blocking) + new cycle rule

Two classes cannot be localized to a cone and keep the whole-network
refusal (network unusable, instances elsewhere poisoned per D3):

1. **Malformed `parameter` nodes** (duplicate name / invalid or
   abstract type, `network_validator.rs:239/252/266`): instances map
   arguments by parameter index; a desynced interface is the known
   OOB-panic class (see `project_evaluate_arg_oob_panic`).
2. **Errors with no node attribution** (`node_id: None`) — nothing to
   poison.

**New rule — intra-network wire cycles.** Add cycle detection to
`validate_network` (DFS over wires within each scope). Attribute the
error to **every cycle member**, blocking. Under D3 this cone-poisons:
evaluation never enters a fully poisoned cycle, so the current
hang/overflow risk disappears without a special evaluator guard. (A
cheap connect-time reachability refusal in `can_connect_nodes` is a
desirable complement but validation is the safety net — hand-authored
`.cnnd` files bypass connect-time checks.)

The lone-node rule (`network_validator.rs:655`) stays **blocking** but
becomes cone-scoped by D3 — dropping a bare `relax` darkens only that
`relax` node. No `fallback_if_disconnected` sprinkling needed.

### D6. Evaluation-error lifetime in the panel: keep, dim, replace wholesale

Per network, keep a **last-known evaluation-error snapshot**:

- After each refresh of the active network, harvest the live scene
  (`get_all_node_errors`, `structure_designer_scene.rs:217` — the scene
  already maintains merged current state across partial refreshes, so
  harvesting gives replace-not-accumulate semantics for free), filter
  to root causes (D7), and **replace** the active network's snapshot.
- On leaving a network, its snapshot **persists** (dropping it would
  make badge counts change merely from switching networks — errors
  appearing to fix themselves reads as flakiness). Inactive networks'
  eval entries render **dimmed** (faded/hollow bolt) — "from last
  evaluation".
- A jump to a snapshot entry validates the node still exists; vanished
  targets are dropped from the snapshot at that point.
- Snapshots are runtime-only state (never serialized).

Coverage note (stated in the reference guide): eval entries cover only
what was evaluated — displayed nodes and their upstream cones.

### D7. Root-cause origin links + "Go to root cause" (chains, stage 1)

Record the chain **structurally in the context**, without touching
`NetworkResult`:

- At the wire-resolution choke point (`evaluate_arg` /
  `resolve_incoming_wire`), whenever a resolved wire value is an
  `Error`, record `consumer NodeRef → source NodeRef` in a new
  `context.node_error_origins: HashMap<NodeRef, NodeRef>`. Recording at
  resolution time covers both wrapped and verbatim pass-through cases
  uniformly — it fires before the consumer decides what to do with the
  error.
- **A root cause is an errored node with no origin link.** The panel
  lists root causes; derived errors are collapsed behind them (shown
  indented / on demand in the picker, not as top-level entries), so one
  failure does not flood the list with its downstream cone.
- **"Go to root cause"**: from any errored node (context menu) or any
  derived entry (picker row action), follow the links to the end and
  jump via the existing `jumpToNode` spine.
- Cross-network chains stop at the instance node in this stage (the
  custom-network hop's inner refs are not viewable in the active
  scene); the user descends with Go to Definition. Stated limitation.
- **Interaction with memoization** (the one cross-doc touch point):
  origin links must be recorded at wire resolution *even on a memo
  cache hit* — a cached upstream `Error` still links its second
  consumer to the root cause. See `doc/design_eval_memoization.md` D8.

### D8. Fix the eval-error suppression gate (standalone bug fix)

`build_node_view` appends the scene's eval error unless **this node**
already has a validation error — replacing the whole-network
`validation_errors.is_empty()` check (`structure_designer_api.rs:676`).
Ship first; independent of everything else.

### D9. Fold the third channel into the two real ones

`motif` / `materialize` / `motif_sub` stop discarding their
`ValidationError`s: their parse errors surface through the node badge
as today *and* join the unified list (as non-blocking validation
errors — the litmus test: their eval paths already no-op/localize on
unparsed data). While here, reconsider `expr`'s parse errors: they are
blocking today; under D3 blocking is cone-scoped, so the current
severity becomes acceptable without change.

### D10. Structured error payload (stage 2, designed now, built later)

Widen `NetworkResult::Error(String)` →
`Error(Arc<EvalError>)` where `EvalError = { message: String, frames:
Vec<ErrorFrame> }`, `ErrorFrame = { pin: String, source: NodeRef-like
(scope-qualified) + type name, network hop marker }`. Feasibility
established: no `Serialize`/`PartialEq` on `NetworkResult`; errors
never persisted; pass-through sites don't mutate payloads; `Arc` keeps
clones O(1); a `Display` impl renders today's text so
`node_errors: HashMap<NodeRef, String>`, `NodeView.error`, and
`execute` `error_message` keep working; <10 substring-style tests to
migrate. The wrap sites are ~4 (`evaluate_arg`) + 2 (custom-network) +
~6 (zone/closure). This obsoletes none of D7's UI — it upgrades its
data source and is the substrate for a future debug call stack.

## Phases

### Phase 1 — Gate fix (D8)
Per-node suppression check in `build_node_view`. Test: node A with a
validation warning + node B with a runtime error → B's badge shows.

### Phase 2 — Validation accumulation (D4)
`validate_wires` accumulates per node; `validate_parameters` where
safe. Tests: a network with two independent type mismatches reports
both; `valid` flips identically to today.

### Phase 3 — Cone-scoped blocking (D3, D5)
Evaluator skip-and-synthesize; `generate_scene` blank restricted to the
interface residue; cycle-detection rule; `execute_node`/CLI gate
relaxation. Tests: lone `relax` + independent finished subgraph → the
subgraph renders, the `relax` shows its validation error as its output
error; type-mismatch node's eval is never entered (no panic); A-uses-
invalid-B → only the instance cone dark in A; authored wire cycle →
all members flagged, no hang; malformed parameter → network still
refuses as before.

### Phase 4 — Unified list + UI (D1, D2, D6)
API: per-network error list gains eval entries (active network live,
inactive snapshots); severity + source fields. Flutter: bolt icons,
dimmed stale entries, badge/picker/tooltip/F8 consume the merged list;
`hasValidationErrors`-style aggregates keep a blocking-only variant for
the direct-editing banner. Reference guide updated.

### Phase 5 — Root-cause navigation (D7)
`node_error_origins` recording; root-cause filtering in the harvested
snapshot; derived-entry collapsing in the picker; "Go to root cause" in
the node context menu (Navigate section) and picker rows. Tests: chain
of three nodes → one root entry; origins recorded on pass-through nodes
(walkers, if/switch) as well as wrapped ones.

### Phase 6 — Chain hygiene (D9 + violations)
Fix `lattice_symop` / `array` / `atom_composediff` / `apply_diff`
inner-cause loss; route `motif`/`materialize`/`motif_sub` errors into
the unified list; document the no-re-wrap convention in
`rust/src/structure_designer/nodes/AGENTS.md`.

### Future (explicitly deferred)
- D10 structured `EvalError` payload; debug call-stack UI.
- Evaluation warnings (first candidate: relax non-convergence).
- Background evaluation for full error coverage
  (`doc/design_background_evaluation.md`).
- Connect-time cycle refusal in `can_connect_nodes`.

## Deferred / follow-ups

- The `Error in {network}` wrap keeps only the type name; D10's frames
  carry instance identity. Until then, cross-network root-cause
  navigation stops at the instance node (D7).
- `describe_wire_source` returns `None` for `ZoneInput` sources; D7's
  origin links cover these structurally (link recorded at resolution),
  so the text gap stops mattering for navigation.
- Panel eval coverage for *inactive* networks' newly-introduced errors
  requires background evaluation; out of scope.
