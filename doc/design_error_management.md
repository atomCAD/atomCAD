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

- **Severity** (user-facing, color): **Error** (red — the node's
  output is unavailable and its downstream cone is dark, whether
  because evaluation was skipped (D3) or because it ran and failed) vs
  **Warning** (amber — advisory; everything still evaluates). Today's
  `blocking: true` maps to Error, `blocking: false` to Warning. The
  stored bool survives unchanged; its *meaning* shrinks from "blanks
  the network" to "poisons this node's cone" (D3). The derived
  `NodeNetwork::valid` flag shrinks in lockstep, to "free of the
  interface residue" (D5).
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
  call `eval`; it synthesizes a `NetworkResult::Error` from the node's
  blocking validation text — when several blocking errors are
  attributed to the node (D4 accumulates), from **all** their texts
  joined with newlines, the same join convention the canvas tooltip
  already uses — as the node's output and records it under the node's
  `NodeRef` like any runtime error. (The synthesized entry is a
  propagation vehicle, not a second error: badge display and the panel
  harvest dedupe it against the validation entries by predicate, never
  by text — see D8's coexistence rule.)
- Downstream consumers receive the synthesized error through the
  existing chaining machinery (`evaluate_arg`); independent nodes
  evaluate untouched. The viewport shows everything evaluable.
- `generate_scene_scoped`'s `!valid` blank (`network_evaluator.rs:568`)
  is retained **only** for the interface-level residue (D5) — not by
  editing the gate, but because D5 redefines `valid` itself to mean
  "free of the residue". For networks whose only blocking errors are
  node-attributed, the scene generates normally with poisoned cones.
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

**Cross-network blast radius — this does *not* come for free.** Both
cross-network gates key on the *child's* `valid` flag: the "References
invalid node network" rule (`network_validator.rs:489`, `574`) and the
custom-network eval refusal (`"{name} is invalid"`,
`network_evaluator.rs:1704`, `1930`). If `valid` kept its current
meaning ("any blocking error"), a bare `relax` dropped inside custom
network B — the headline scenario, just performed while editing a
custom network — would still flip B invalid, stamp a blocking error on
every instance of B, and make the retained refusal reject evaluating B
under those instances: every parent goes dark even though B's dangling
node feeds nothing. Cone-scoping would hold only at top level.

The fix is D5's redefinition of `valid` (residue-only), which both
gates inherit untouched. Resulting cross-network semantics:

- **B has only node-attributed blocking errors** → B stays `valid`;
  instances *evaluate* B normally. If a poisoned node lies inside the
  return cone, the child-side skip-and-synthesize fires during the
  instance's evaluation (child frames carry B's own
  `validation_errors`, so the poison lookup works under instances) and
  the error propagates out of the instance as an ordinary chained
  runtime error. If it lies outside the return cone, the instance is
  **completely unaffected**. Localization is exact, per call site,
  with no parent-side validation rule involved.
- **B has interface residue (D5)** → the now-narrowed rule 3 stamps a
  blocking error on the instance → the instance is a poisoned node in
  the parent (this design's D3 rule), and the localized refusal
  (`"{name} is invalid"`) remains as the defense-in-depth fallback.
  The parent renders everything else.

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

**Mechanism (how the code tells the classes apart).** Class 2 is
detectable from `node_id`, but class 1 is *node-attributed* — D3's
"blocking + attributed → poison" rule would wrongly cone-scope it. So
`ValidationError` gains an `interface: bool` field (serde default
`false`, so existing serialized errors and all current constructors are
unaffected), set `true` only by the `validate_parameters` rules. The
two predicates the evaluator uses:

- *Network refuses evaluation* iff any error has `interface == true`
  **or** (`blocking && node_id == None`).
- *Node is poisoned* (skip-and-synthesize) iff it has an error with
  `blocking && node_id == Some(id) && !interface`.

**`NodeNetwork::valid` is redefined to be the first predicate** (in
Phase 3): `valid == true` iff the network has no interface-residue
error. Node-attributed blocking errors stop flipping it — they poison
cones instead (D3). This redefinition happens at the *producer*
(`validate_network`'s flag computation), deliberately not at the
consumers: every `.valid` reader — the scene gate
(`network_evaluator.rs:568`), the custom-network eval refusal
(`:1704`/`:1930`), the "References invalid node network" rule
(`network_validator.rs:489`/`574`), the `execute_node`/CLI gates
(`structure_designer.rs:7854`/`7756`), and the upward validity cascade
(`validate_active_network_with_initial_errors`,
`structure_designer.rs:7690`) — asks exactly "is this network usable
at all?", and the residue is the new answer to that question. Editing
gates one by one would leave the cross-network sites behind (see D3,
"Cross-network blast radius"). Phase 3 includes a grep audit of all
`.valid` readers: any reader that actually means "does this network
have any blocking error?" must switch to an explicit
`validation_errors` query (none are known — the AGENTS.md
re-validation-heuristics rule already routes those to
`validation_errors`, not `valid`). Two knock-on effects, both wanted:
the upward validity cascade narrows automatically (a cone-scoped error
in B no longer changes B's `valid`, so nothing cascades; parents'
re-display is driven by the existing dirty propagation, not by
validity), and the AGENTS.md section "Validation errors: blocking vs
non-blocking" — including its `valid`-vs-`validation_errors` gate
guidance — is rewritten in the same phase to document the shrunk
meaning.

New rules default to `interface: false`; adding an interface-level rule
is a deliberate act of setting the flag, mirroring how
`blocking`/`warning` is chosen today.

**New rule — wire cycles (cross-scope-complete).** Add cycle detection
to `validate_network`. A naive per-scope DFS over regular wires is
**not** sufficient, because wires are not scope-local: capture wires
(`source_scope_depth ≥ 1`) and `zone_output_arguments` thread through
zone bodies, so a cycle can run "node X → captured into H's body →
body node → zone-output wire → H's output → ordinary wires → X" —
invisible to a DFS that treats H as opaque. The saving structural fact:
a body node's output is consumable only intra-body or by the owning
node's `zone_output_arguments` — there is no wire from outside into a
body — so **every cross-scope cycle passes through the zone-owning
node itself**. That makes the complete dependency graph cheap to build
per scope S:

- one vertex per node of S; one edge per regular wire in S;
- for each zone-owning node H in S (HOFs *and* `closure` nodes —
  closure captures are frozen at the `closure` node's own eval, so the
  dependency sits exactly there), walk H's whole body subtree across
  all nesting depths and, for every capture wire whose depth resolves
  to a node of S, add the edge "H depends on that node". Captures
  resolving to a scope *above* S are projected onto the zone-owning
  ancestor when that ancestor's scope is validated. `ZoneInput`
  references need no edge — they reach the iteration value through H's
  own input wires, which are already edges. Custom-network *reference*
  cycles are already rejected at creation.

The DFS runs for every scope (top-level networks and every zone body —
the same recursion tree `validate_zones_recursive` walks, and the
body-subtree capture walk is the same traversal its capture-target
checks already perform). Every authorable cycle is caught in the
highest scope it touches. Attribute the error to **every cycle member
in S**, blocking — a zone-owning node standing in for its body's
capture is the right target: it is the node whose eval would recurse.
Under D3 this cone-poisons: evaluation never enters a fully poisoned
cycle.

**Defense in depth — evaluator re-entrancy guard.** The validation rule
is the primary protection, but it is also the *only* thing standing
between a hand-authored `.cnnd` and a hang, so Phase 3 additionally
adds a cheap backstop at the central eval dispatch: a per-pass
`context.eval_in_progress: HashSet<NodeRef>` (same keying as
`node_errors`), inserted before dispatching a node's eval and removed
after it returns. Re-entering a `NodeRef` already in the set
synthesizes a localized `NetworkResult::Error("evaluation cycle
detected at …")` instead of recursing. Legitimate *sequential*
re-evaluation — per-item body runs under walkers, the same network
under two instances (distinct scope paths, hence distinct `NodeRef`s)
— never trips it; only true same-key re-entrancy, which today means a
hang, does. Cost: one hash insert/remove per node eval. (A cheap
connect-time reachability refusal in `can_connect_nodes` is a
desirable complement but validation is the safety net — hand-authored
`.cnnd` files bypass connect-time checks.)

The lone-node rule (`network_validator.rs:655`) stays **blocking** but
becomes cone-scoped by D3 — dropping a bare `relax` darkens only that
`relax` node. No `fallback_if_disconnected` sprinkling needed.

### D6. Evaluation-error lifetime in the panel: keep, dim, replace wholesale

Per network, keep a **last-known evaluation-error snapshot** (stored on
`StructureDesigner`, keyed by network name; runtime-only, never
serialized):

- After each refresh of the active network, harvest the live scene
  (`get_all_node_errors`, `structure_designer_scene.rs:217` — the scene
  already maintains merged current state across partial refreshes, so
  harvesting gives replace-not-accumulate semantics for free) and
  **replace** the active network's snapshot.
- **Harvest scope, staged by phase.** Harvested keys are eval-scoped
  `NodeRef`s whose scope paths may contain custom-network-instance
  hops (recorded for child-network internals but not addressable in
  the active network's coordinate system). In **Phase 4** the harvest
  keeps only entries whose scope path resolves through the active
  network's own zone-body tree (`resolve_scope_network`-style walk:
  every hop must be a zone-owning node) — exactly the set that is
  viewable today — and every kept eval error is a top-level list
  entry. In **Phase 5**, origin links (D7) add the rest: root causes
  behind custom-network hops enter the list via their jump-ready link
  addresses, and derived entries collapse behind their roots. The
  synthesized-duplicate dedupe (D8) applies in both phases: an eval
  entry is dropped iff its node has a blocking validation error — a
  predicate check, no text comparison.
- On leaving a network, its snapshot **persists** (dropping it would
  make badge counts change merely from switching networks — errors
  appearing to fix themselves reads as flakiness). Inactive networks'
  eval entries render **dimmed** (faded/hollow bolt) — "from last
  evaluation".
- A jump to a snapshot entry validates the target still exists — the
  node, and (for Phase-5 rows carrying a cross-network jump address)
  the named host network; vanished targets are dropped from the
  snapshot at that point.
- **Key lifecycle — the issue-#377 name-vs-stable-id lesson applies
  here, in the opposite direction from D7.** D7's origin links may be
  name-keyed *because* they are regenerated on every refresh; the
  snapshot is deliberately long-lived (it survives network switches),
  so its name keys must be actively maintained by the two operations
  that mutate the name space. `rename_node_network` re-keys the
  snapshot map entry **and** rewrites the renamed name wherever it
  appears inside stored entries (the Phase-5 jump addresses embed
  `host_network` names) — a linear scan of a small runtime-only map,
  same spirit as its existing rename cascades. Network **deletion**
  drops the entry, so a network later created under the same name
  starts with no inherited errors. Duplicate-network needs no hook:
  the copy has never been evaluated and correctly starts snapshotless.
  Node-level concerns need nothing: entries key nodes by `NodeRef`
  (ids, not names), and node deletion is covered by the jump-time
  check above.
- Snapshots are runtime-only state (never serialized).

Coverage note (stated in the reference guide): eval entries cover only
what was evaluated — displayed nodes and their upstream cones.

### D7. Root-cause origin links + "Go to root cause" (chains, stage 1)

Record the chain **structurally in the context**, without touching
`NetworkResult`:

- At the wire-resolution choke point (`evaluate_arg` /
  `resolve_incoming_wire`), whenever a resolved wire value is an
  `Error`, record an origin link for the consumer in a new
  `context.node_error_origins` map. Recording at resolution time covers
  both wrapped and verbatim pass-through cases uniformly — it fires
  before the consumer decides what to do with the error.
- **Origins are recorded as jump-ready addresses, not raw eval refs.**
  An eval-scoped `NodeRef` interleaves zone-body hops and
  custom-network-instance hops and means nothing to the navigation
  layer. Reconstructing the distinction after the fact from bare ids is
  fragile — but at record time the evaluator holds the full network
  stack, where every frame knows whether it is a zone hop
  (`is_zone_body`) and, for a network hop, *which* network it entered
  (`NetworkStackElement.node_network`). So the link value is an
  `ErrorOrigin { host_network: String, scope_path: Vec<u64> (zone hops
  since the last network hop), node_id: u64 }`, computed from the live
  stack — exactly the triple `jumpToNode` consumes. The map key is the
  consumer's eval-scoped `NodeRef` (same keying as `node_errors`); the
  value is a small `Vec<ErrorOrigin>` — **one entry per distinct
  failing input**, in input-pin order, deduped by address. A
  single-slot value would silently lose chains: a consumer with two
  independently errored inputs must keep a link to each
  (last-write-wins is the naive shape to avoid). Conceptually
  `ErrorOrigin` ≡ `(host_network, NodeRef)` — the **global address of a
  node in the .cnnd document**, the same triple `APINetworkUsage`
  already uses for Find Usages. Each link stores the address of the
  consumer's *immediate* origin; root causes are derived by following
  links to the ends of the link graph (a DAG once multi-input fan-in
  exists), so the full chain remains available (future debug-stack
  substrate). Note the network
  half of the address is the network's *name* — safe here only because
  origin links are runtime-only and regenerated on every refresh (a
  rename triggers a refresh, so a stale name is never dereferenced).
  The one place these addresses *do* outlive a refresh — copied into
  D6's long-lived snapshots at harvest time — is covered by D6's
  key-lifecycle rule (rename rewrites stored `host_network` names;
  jump-time validation catches deletions). Persisting these addresses
  to disk would require a stable-network-id scheme first (the
  issue-#377 name-vs-stable-id lesson).
- **Chains cross network boundaries.** The custom-network wrap sites
  (`Error in {network}: …`, `network_evaluator.rs:1742`, `1958`) do not
  pass through `evaluate_arg`, so they must record the boundary link
  explicitly: instance node → the child network's return-node cone
  (whose internal links already exist, since `evaluate_arg` runs inside
  the child too). With the boundary link in place, a chain
  `M-consumer → … → instance I → C.return → … → X` is complete and
  every hop is navigable.
- **A root cause is an errored node with no origin link.** The panel
  lists root causes; derived errors are collapsed behind them (shown
  indented / on demand in the picker, not as top-level entries), so one
  failure does not flood the list with its downstream cone. A root
  cause may live in **another network** than the one whose evaluation
  surfaced it; its row carries a provenance qualifier (e.g.
  `in C (via instance1)`). *Jump addresses for root-cause rows:* a
  root cause reached through links is addressed by the terminal link's
  `ErrorOrigin` value; an errored node with no links at all was
  necessarily evaluated in the active network's own scope tree, so its
  eval-scoped ref converts directly (active network name + zone-hop
  scope path).
- **"Go to root cause"**: from any errored node (context menu) or any
  derived entry (picker row action), follow the links to the end and
  jump via the existing `jumpToNode` spine — including across network
  boundaries. When the walk fans out to more than one distinct root
  (multi-input fan-in), jump to the first in input-pin order — a
  deterministic choice, and the other roots are not lost: each is
  independently visible as its own top-level root-cause row.
- **Landing in another network:** activating the root cause's host
  network re-evaluates it *standalone* (its own parameter defaults and
  display set), so the target node may legitimately show no badge — the
  error existed only under the originating instance's arguments. The
  jump still selects and scrolls to the node, and the UI shows the
  original error text + provenance in a transient surface (snackbar /
  status line): the user lands with the context in hand even though the
  live badge cannot reproduce it. (Viewing a definition in the *actual
  call context* of an instance is the "super advanced" follow-up
  already noted in `doc/design_find_usages.md`; explicitly out of
  scope.)
- **Interaction with memoization** (the one cross-doc touch point):
  origin links must be recorded at wire resolution *even on a memo
  cache hit* — a cached upstream `Error` still links its second
  consumer to the root cause. See `doc/design_eval_memoization.md` D8.

### D8. Fix the eval-error suppression gate (standalone bug fix)

`build_node_view` appends the scene's eval error unless **this node**
has a **blocking** validation error — replacing the whole-network
`validation_errors.is_empty()` check (`structure_designer_api.rs:676`).
Ship first; independent of everything else.

**Multiplicity and coexistence rule.** A node can carry *several*
validation entries (`validation_errors` is a `Vec` with no per-node
dedupe; the zones pass accumulates, `expr` can push multiple) but at
most *one* eval error (`node_errors` is keyed by `NodeRef`, last write
wins).

**Neither surface is limited to one entry per node.** The canvas node
badge/tooltip already joins all of the node's messages with newlines
(`build_node_view`), and the panel lists one row per *error* (a node
may contribute several rows, each with its own severity color and
source icon; the badge counts errors, not nodes). There is no display
slot forcing a choice between validation and evaluation.

**There is exactly one suppression, and it is about redundancy, not
capacity — applied identically on every surface** (canvas badge,
tooltip, panel list, F8 targets):

- *Blocking validation error + eval error:* the eval entry is dropped.
  Under D3 this collision is degenerate by construction — the poisoned
  node was never evaluated; its "eval error" is the synthesized join
  of its own blocking validation text(s), manufactured only so
  downstream nodes receive a propagating `Error`. Showing it would
  print the same sentence(s) twice for one underlying fact. The
  suppression is a **predicate check** ("does this node have a
  blocking validation error?"), never a text comparison — with several
  accumulated blocking errors (D4) the synthesized join matches no
  single validation entry byte-for-byte, so text matching would be
  fragile as well as wrong.
- *Everything else is always shown everywhere:* multiple validation
  entries all appear; a validation warning **never** suppresses
  anything — warnings evaluate (D3), and the eval can fail for a
  different, more specific reason, so warning(s) + eval error appear
  together (suppressing the eval error would mask the actual runtime
  failure behind an advisory).

Distinct from suppression: **derived-error collapsing (D7) is
panel-only presentation.** Derived eval errors (those with an origin
link — the downstream cone of a root cause) are collapsed behind their
root-cause row in the panel list, reachable on demand in the picker
rather than shown as top-level rows. On the **canvas** they remain
fully visible per node — when looking at a node the user wants to know
why *it* is dark; when scanning the panel the user wants one row per
underlying problem, not its downstream echoes.

**F8 cycles distinct errored nodes, not entries** — multiple entries
on one node would otherwise make a keypress a visible no-op; the
landing shows all of the node's messages anyway.

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

## Regression strategy

This design changes validation outcomes, evaluation gating, and error
text — the three things most likely to silently regress the ~3400-test
baseline and existing user `.cnnd` designs. Cross-cutting measures,
built once and reused by every phase:

**R1. The `.cnnd` validation-corpus snapshot harness (built in Phase 2,
the backbone of the story).** A new test loads **every** fixture under
`rust/tests/fixtures/**/*.cnnd` (49 files today; new fixtures join
automatically), runs the full validate pass, and insta-snapshots, per
network: `(name, valid, [(node_id, scope, blocking, error_text)])`,
sorted deterministically. Fixtures that fail to *load* (the corpus
includes deliberately corrupt/legacy migration files, e.g.
`corrupt_v2.cnnd`) record their load outcome as the snapshot entry
instead of panicking the harness. The snapshot is recorded **before**
the first behavioral phase lands, so every later phase's effect on real
designs shows up as a reviewable `cargo insta review` diff — intended
changes are accepted deliberately; anything unexpected is a caught
regression.
Phases 2, 3, and 6 each predict their own diff shape in their Tests
subsection (e.g. "additional errors may appear; no `valid` flag may
change").

**R2. Eval-equivalence guard for networks without blocking errors.**
Cone-gating must be a provable no-op wherever no blocking validation
error exists. Structurally: the skip-and-synthesize branch is only
reachable when a blocking error is attributed to the node, so such a
network cannot take it. Behaviorally: a corpus-driven test evaluates
the displayed nodes of every fixture network that has no blocking
validation errors and insta-snapshots, per displayed node, the
*outcome* — `Ok` or the error text. **Note: clean validation does NOT
imply no runtime errors** (a missing required input is deliberately a
runtime error), so the assertion is that the outcome snapshot is
**byte-identical** before and after Phase 3 — not that it is empty.
(Same harness as R1, second snapshot section; baseline recorded with
R1's in Phase 2.)

**R3. Full-suite gates per phase.** Every phase lands only with
`cargo test -j 4` green, `flutter analyze` clean, and the Flutter smoke
test (`flutter test integration_test/`) passing. Expectation updates to
existing tests are enumerated in the phase's Tests subsection (found
via targeted greps, e.g. `validation_errors.len()`, error-text
substrings) and changed *deliberately*, never as drive-by fixes.

**R4. Manual walkthrough after every phase with a user-perceivable
change.** Per the project's testing convention, UI behavior is
verified by a scripted manual walkthrough rather than mandated widget
tests; the Rust surface below it gets the automated coverage. **Every
phase in this design changes something a user can see, so every phase
carries a `#### Manual walkthrough` checklist** — the implementing
agent completes the automated tests, then hands the checklist to the
human for sign-off before the next phase starts. Checklist steps are
written as *action → expected observation* so a failed expectation is
unambiguous.

## Phases

### Phase 1 — Gate fix (D8)

Per-node, blocking-only suppression check in `build_node_view`.

#### Tests
- New: node A with a validation warning + node B with a runtime error
  → B's badge shows (the old network-wide gate hid it).
- New: node C with a warning *and* its own runtime error → C's badge
  shows both messages.
- New: node D with a blocking validation error → only the validation
  text (no duplicate once Phase 3's synthesized entry exists; until
  then, D simply has no eval entry).
- Regression: full suite (R3). Display-layer only — no corpus diff
  expected (R1 not yet built; this phase is safe to land first
  precisely because it cannot change validation or evaluation state).

#### Manual walkthrough
- Build a network containing a node with a validation *warning* (e.g.
  a `closure` with an unwired zone-output pin) **and**, elsewhere, a
  node that fails at runtime (e.g. a node with a required input
  unwired) → the runtime node shows its red error badge (previously
  the warning anywhere in the network suppressed it).
- Give one node *both* a warning and its own runtime failure → its
  tooltip shows both messages on separate lines.
- A node with a blocking validation error → shows only the validation
  text, no duplicated sentence.

### Phase 2 — Validation accumulation (D4)

`validate_wires` accumulates per node (per-node early-outs retained —
later checks within one node assume earlier invariants);
`validate_parameters` accumulates where safe. **Build the R1 corpus
harness in this phase, recording the pre-change baseline first.**

#### Tests
- New: a network with two independent type mismatches reports both
  errors, attributed to the right nodes.
- New: `valid` flips under exactly the same conditions as before —
  a fixture that was invalid stays invalid, valid stays valid
  (asserted network-wide by the R1 snapshot: the `valid` column must
  show **zero** diffs in this phase). Phase 3 then *deliberately*
  changes these conditions (the D5 `valid` redefinition); asserting
  zero diffs here isolates accumulation from that semantic change.
- Corpus (R1): expected diff shape — *additional* error rows may
  appear on already-invalid networks; no row may disappear; no `valid`
  flag may change.
- Expectation review (R3): grep and enumerate tests asserting
  `validation_errors.len() == 1` / first-error-only behavior; update
  each deliberately.
- Ordering note: this phase must land **before or with** Phase 3
  (D4 rule) — an unrecorded error is an unpoisoned node.

#### Manual walkthrough
- Create two *independent* type mismatches in one network (wire
  incompatible pins in two unrelated places, e.g. via a hand-edited
  file or by breaking two record schemas) → **both** destination nodes
  show red badges at once (previously only the first was reported).
- The user-types panel badge for that network shows count 2; clicking
  it lists both entries; F8 cycles through both nodes.
- Fix one → count drops to 1 without re-validating manually (the
  mutation's own validate pass updates it).

### Phase 3 — Cone-scoped blocking (D3, D5)

Evaluator skip-and-synthesize; `valid` redefined to the interface
residue (D5) — the scene blank, custom-network refusal, "references
invalid network" rule, and `execute_node`/CLI gates all inherit the
relaxation through the flag, backed by a grep audit of remaining
`.valid` readers; new cycle-detection rule (with cross-scope capture
projection) plus the evaluator re-entrancy backstop; AGENTS.md
blocking-vs-non-blocking section rewritten for the shrunk meanings.

#### Tests
- New: lone `relax` + independent finished subgraph → the subgraph
  renders; the `relax` node's output is the synthesized validation
  error; its `eval` was never entered.
- New: type-mismatch destination node's `eval` is never entered (the
  panic class this gate historically protected against) — assert via
  a test node/counter or the absence of the panic under a crafted
  mismatch fixture.
- New: B has a cone-scoped blocking error **outside** its return cone;
  A instantiates B → B stays `valid`, no "references invalid network"
  error appears in A, and A's instance evaluates and renders normally
  (completely unaffected).
- New: B has a cone-scoped blocking error **inside** its return cone;
  A instantiates B → the instance's output is a chained error (its
  cone dark in A); A's other nodes evaluate; B's own scene renders
  with just the poisoned cone.
- New: authored wire cycle (hand-built fixture — the UI cannot create
  one yet) → all cycle members flagged, evaluation terminates, no
  hang/overflow; cycle members' cones dark, independents render.
- New: capture-threaded cycle (X → capture into an HOF body →
  zone-output wire → HOF output → wires → X), plus a nested-body
  variant and a `closure`-node variant → the projected capture edge
  makes the DFS flag the cycle members (zone owner included);
  evaluation terminates.
- New: evaluator re-entrancy guard — evaluate a cyclic fixture
  *without* running validation first (direct evaluator-level test,
  simulating an escaped cycle) → terminates with the localized "cycle
  detected" error, no hang; and a fixture with heavy legitimate
  re-evaluation (HOF over many elements; one custom network used by
  two instances) never trips the guard.
- New: malformed `parameter` network → still refuses whole-network
  evaluation (the `interface` flag keeps `valid == false` under the
  D5 redefinition); the "references invalid network" rule still fires
  in the parent, so instances of it are poisoned nodes; parent renders
  otherwise.
- New: `.cnnd` round-trip of a `ValidationError` with and without the
  `interface` field (serde default `false` keeps legacy files
  loading).
- New: `execute_node` on a poisoned cone yields the synthesized
  `Error` result (no special-case refusal needed); on a clean cone in
  a partially-broken network it executes normally.
- Corpus (R1): expected diff — the `valid` column flips to `true` for
  every fixture network whose blocking errors are all node-attributed
  and non-interface (the headline semantic change; review these
  fixture by fixture); "References invalid node network" rows
  disappear wherever the referenced network is no longer
  residue-invalid; new cycle-rule rows possible on hand-authored
  fixtures; no other rows change. Fixtures with interface-residue
  errors keep `valid == false`.
- Eval-equivalence (R2): activated this phase — clean fixture networks
  evaluate identically before/after.
- Undo interplay: existing `undo_test.rs` suite green — validation
  runs on the same triggers as before; only the evaluation *gate*
  moved.

#### Manual walkthrough
This is the headline UX change of the whole design — walk it
thoroughly, ideally also on a real (non-fixture) design:
- Open a design with working displayed geometry, then drop a bare
  `relax` (or `structure_move`, `passivate`, …) onto the canvas → the
  existing geometry **stays visible** (previously the viewport
  blanked); the new node shows a red badge with the
  unresolved-output message.
- Wire the node up → badge clears, its output appears; unwire again →
  only that node and its downstream cone go dark.
- Chain something downstream of the poisoned node → the downstream
  node darkens with a chained `error in … input` message; independent
  branches keep rendering.
- Drop a bare `relax` inside custom network B on a branch that does
  **not** feed B's return node; open network A that instantiates B →
  A renders **fully unchanged** (previously every instance of B went
  dark). This is the headline scenario performed inside a custom
  network — it must be just as painless there.
- Now break a node that *does* feed B's return (e.g. a wire type
  mismatch in the return cone) → in A, only the B-instance cones
  darken, with a chained error naming the failure inside B; A's other
  nodes render. Fix B → A recovers on its next refresh.
- Break a `parameter` node in a network (hand-edit or duplicate name)
  → that network still refuses to evaluate (viewport blank there —
  residue preserved); its instances elsewhere show as poisoned nodes
  only.
- Right-click → Execute on a node in a *clean* cone of a
  partially-broken network → executes normally; Execute on a poisoned
  cone → error result surfaced, no crash.
- Undo/redo across the wire/unwire steps above → badges and rendering
  track state correctly at every step.
- Sanity: load several of your existing real `.cnnd` designs → they
  render exactly as before (no new dark nodes, no new badges).

### Phase 4 — Unified list + UI (D1, D2, D6)

API: per-network error list gains eval entries (active network live,
inactive snapshots; synthesized-duplicate dedupe per D8; **harvest
limited to the active network's own scope tree per D6's staging — all
kept eval entries are top-level rows in this phase**); severity +
source fields. Flutter: bolt icons, dimmed stale entries,
badge/picker/tooltip/F8 consume the merged list;
`hasValidationErrors`-style aggregates keep a blocking-only variant
for the direct-editing banner. Reference guide updated.

#### Tests
- New (Rust API): merged-list getter — validation + eval entries with
  correct severity/source tagging; a poisoned node contributes exactly
  one entry (dedupe); warning + eval error contributes two.
- New (Rust API): snapshot lifecycle — refresh replaces the active
  network's eval entries wholesale (no accumulation across refreshes);
  switching networks preserves the inactive snapshot; re-activating
  and refreshing replaces it.
- New (Rust API): jump-target validation — a snapshot entry whose node
  was deleted is dropped, not returned.
- New (Rust API): snapshot key lifecycle — renaming a network re-keys
  its snapshot (its entries survive under the new name); deleting a
  network drops its snapshot; a new network created under the deleted
  network's name reports no eval entries.
- Expectation review (R3): `hasValidationErrors` consumers (the
  direct-editing banner gate) keep blocking-only semantics — test that
  an eval-error-only design does not trip the banner.
#### Manual walkthrough
- Badges: a network with only validation errors → red/amber badge as
  before; a network with only eval errors → badge appears (new);
  mixed → one badge, count = validation entries + kept eval entries.
- Picker rows: each row shows its own severity color **and** source
  icon (structural glyph vs bolt); a node with a warning + eval error
  contributes two rows; a poisoned node contributes exactly one
  (dedupe).
- Jump: click an eval-error row → lands on the node, selected and
  scrolled, same feel as validation-row jumps.
- Snapshot lifecycle: cause an eval error, note the panel entry; fix
  it → entry disappears on the next refresh (replace, not
  accumulate). Switch to another network → the first network's eval
  entries remain, rendered **dimmed** (stale bolt); switch back and
  refresh → entries replaced with live state.
- Deleted target: delete a node that has a stale eval entry, then
  open the picker from another network → the entry is gone (not a
  dead jump).
- Rename/delete lifecycle: rename a network that has dimmed eval
  entries → the entries follow it under the new name; delete a
  network with entries, then create a new network with the same name
  → no ghost entries appear on the newcomer.
- Tree tab: same badges on leaves; folder roll-up dots reflect the
  merged list (a collapsed folder hiding an eval-error-only network
  shows a dot).
- F8: over a mixed validation/eval set cycles all errored nodes in
  order; tooltip contents list all messages per node.
- Direct-editing banner: a design with *only* eval errors does not
  trip the validation banner (blocking-only aggregate).
- `atomcad-cli` / `http_server` textual output ([ERROR: …] lines)
  spot-checked against the new list shape.

### Phase 5 — Root-cause navigation (D7)

`node_error_origins` recording with jump-ready `ErrorOrigin` values
(wire-resolution sites + the two custom-network boundary wrap sites);
root-cause filtering in the harvested snapshot and extension of the
harvest to cross-network root causes (D6's Phase-5 staging — addresses
come from terminal link values); derived-entry collapsing in the
picker; "Go to root cause" in the node context menu (Navigate section)
and picker rows; cross-network landing with the transient
original-error display.

#### Tests
- New: chain of three nodes → one root entry; the derived entries
  carry origin links pointing one hop upstream each.
- New: multi-root consumer — one sink fed by two independently
  failing sources → the sink records **two** origin links (input-pin
  order, deduped by address); both sources appear as top-level root
  rows; the sink is a derived entry, not a top-level row; "Go to root
  cause" from the sink deterministically lands on the first-pin
  source.
- New: origins recorded on pass-through nodes (walkers, if/switch,
  per-node early-return guards) as well as text-wrapped ones.
- New: error inside custom network C used by M → M-side chain
  terminates at a root whose `ErrorOrigin` addresses `(C, scope, X)`;
  the boundary link exists on the instance node; zone hops inside C
  produce the correct body `scope_path`.
- New: links are pass-scoped — a second refresh regenerates them; no
  stale link survives an edit that fixes the root cause.
- New: rename vs stored addresses — rename network C while another
  network's *snapshot* holds a root-cause row whose jump address
  names C → the stored `host_network` is rewritten (D6 key
  lifecycle) and the jump lands in the renamed C; deleting C instead
  drops the row at jump-validation time rather than jumping to a
  dead name.
- New: F8 cycles distinct errored nodes (a node with several entries
  is visited once).
#### Manual walkthrough
- Same-network chain: build source → middle → displayed sink where the
  source fails → panel shows **one** root-cause row (the source), not
  three; expanding/collapsed affordance reveals the derived entries;
  the canvas still badges all three nodes individually.
- "Go to root cause" from the context menu of the *sink* (a derived
  node) → lands on the source, selected and scrolled.
- Cross-network: make node X inside custom network C fail; use C from
  network M → M's panel shows the root-cause row with the provenance
  qualifier (`in C (via …)`); activating it jumps into C, selects and
  scrolls to X, and shows the transient original-error text +
  provenance (snackbar/status line). X may show no live badge in C —
  expected; the transient text is the context.
- After the cross-network jump, **Back** returns to M (navigation
  history records the hop).
- Multi-consumer root: one failing source feeding two displayed sinks
  → still exactly one root-cause row.
- Multi-root sink: one node fed by two independently failing sources
  → two root-cause rows, the sink collapsed as a derived entry; "Go
  to root cause" on the sink lands on its first-pin source.
- F8 with multiple entries on one node → each errored node visited
  once per cycle lap.
- Fix the root cause → after refresh, the whole chain (root + derived
  rows and all canvas badges) clears in one step.

### Phase 6 — Chain hygiene (D9 + violations)

Fix `lattice_symop` / `array` / `atom_composediff` / `apply_diff`
inner-cause loss; route `motif`/`materialize`/`motif_sub` errors into
the unified list; document the no-re-wrap convention in
`rust/src/structure_designer/nodes/AGENTS.md`.

#### Tests
- New: per fixed node, upstream error text survives the node's wrap
  (root cause reachable both textually and via origin links).
- New: `motif`/`materialize`/`motif_sub` parse errors appear in the
  unified list as non-blocking validation entries; their canvas badge
  behavior is unchanged.
- Expectation review (R3): the <10 test files matching error-text
  substrings — update each deliberately where wording changed.
- Corpus (R1): expected diff — `motif`/`materialize`/`motif_sub`
  fixtures may gain non-blocking rows; no `valid` flag may change.

#### Manual walkthrough
- Enter an invalid motif string on a `motif` node → its badge shows
  the parse error as before, **and** an amber (non-blocking) entry now
  appears in the panel list and navigates to the node; the network
  keeps evaluating.
- Same check for `materialize` (parameter-element string) and
  `motif_sub`.
- Trigger an element error inside an `array` node fed by a failing
  upstream → the array's error message now contains the upstream root
  cause text (previously replaced by ad-hoc prose); "Go to root
  cause" still lands on the true source.

## Deferred / follow-ups

- D10 structured `EvalError` payload; debug call-stack UI.
- Evaluation warnings (first candidate: relax non-convergence).
- Background evaluation for full error coverage
  (`doc/design_background_evaluation.md`).
- Connect-time cycle refusal in `can_connect_nodes`.
- The `Error in {network}` wrap *text* keeps only the type name; D7's
  boundary links carry full instance identity structurally, so
  navigation is unaffected. D10's frames additionally fix the display
  string itself.
- `describe_wire_source` returns `None` for `ZoneInput` sources; D7's
  origin links cover these structurally (link recorded at resolution),
  so the text gap stops mattering for navigation.
- Viewing a definition network in the actual call context of the
  originating instance (real argument values instead of parameter
  defaults after a cross-network root-cause jump) — the
  `doc/design_find_usages.md` "super advanced" follow-up; would let the
  landed-on root cause reproduce its badge live.
- Panel eval coverage for *inactive* networks' newly-introduced errors
  requires background evaluation; out of scope.
