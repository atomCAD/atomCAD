# Function Pin Parameter Roles (Delay Overrides)

**Issue:** [#408](https://github.com/atomCAD/atomCAD/issues/408) — "add tickboxes
to specify which parameters to delay in the right sidebar for the function
output of a node"

**Related designs:** `design_function_pins.md` (the `-1` pin synthesizer),
`design_node_function_pin_captures.md` (wiring-aware params/captures),
`design_closures.md`, `design_currying.md`.

## Motivation

A user (mechadense) wants to author a list of `structure_move` nodes as
`Crystal -> Crystal` function values: only `input` delayed, with `translation`
and `subdivision` coming from the node's **stored property data** — edited
interactively via the drag gizmo — and baked into the function value. The
functions are applied later inside a target subnetwork.

Today this is inexpressible. The `-1` function pin's parameter/capture split is
derived **purely from wiring** (unwired pin = delayed parameter, wired pin =
frozen capture; `doc/design_node_function_pin_captures.md`). Stored node data
plays no role in the partition, so:

1. All pins unwired → type `(HasStructure, IVec3, Int) -> ?` — wrong arity, and
   the `same_as_input` return type resolves to `None` (no fallback), so the
   `-1` wire is rejected outright.
2. Wiring `translation` from a `vec3` node fixes the arity but kills the gizmo
   (a wired pin overrides stored data; the gizmo writes stored data).
3. Wrapping in a `closure` node types correctly, but closure-body nodes never
   contribute to the 3D scene (scene generation only iterates the top-level
   network's displayed nodes), so the gizmo is unreachable — and one wrapper
   per move node is heavyweight when the user needs many.

Additionally, a node whose `-1` pin is consumed is unconditionally skipped by
`generate_scene` and its eye is disabled, so even a correctly-typed function
node cannot show its pin-0 output or its gizmo locally. The user explicitly
wants local, interactive editing of a node that is *also* wired as a function.

## Goals

- Per-pin, user-settable override of the parameter/capture partition for the
  `-1` function pin, persisted, undoable, editable in the properties sidebar.
- A "supplied" pin without a wire bakes the node's **stored property value**
  (gizmo-edited) into the function.
- A "delayed" pin **with** a wire stays a parameter; the wire becomes a local
  **preview + type witness** — it feeds pin-0 evaluation (so the node can
  display and its gizmo works) and resolves the parameter/return types
  concretely, but is ignored when the function is invoked.
- Function-mode nodes (consumed `-1` pin) are **no longer force-hidden**: they
  follow the normal display policy and per-pin eyes like any other node.

## Non-Goals

- No change to `closure` / `apply` / HOF body semantics or to how inline-body
  captures work.
- No `forall` polymorphism in `DataType::Function` — the return type must still
  resolve to a concrete (or declared-abstract) type at validation time. The
  preview-wire witness is the supported way to make a `same_as_input` return
  concrete.
- No text-format authoring surface for roles in v1 (same status as
  `collapse_mode`).
- No new gizmo machinery — gizmo availability falls out of the display
  relaxation plus preview wires.

## Design

### The role enum

```rust
/// How one input pin participates in the node's `-1` function-pin view.
/// Stored sparsely on `Node`; an absent entry means `Auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionPinRole {
    /// Wiring decides (today's behavior): unwired = parameter, wired = capture.
    Auto,
    /// Always a parameter. A wire on the pin, if any, is a *preview / type
    /// witness*: it participates in pin-0 evaluation and type resolution but
    /// is ignored when the synthesized function is invoked.
    Delayed,
    /// Always pre-supplied (never a parameter). Wired: the wire is a frozen
    /// capture (same as Auto-wired). Unwired: the node's stored property
    /// value applies at invocation (the body-node argument is left empty, so
    /// `NodeData::eval`'s stored-data fallback fires — the gizmo case).
    Supplied,
}
```

Storage: `Node.function_pin_roles: BTreeMap<usize, FunctionPinRole>` (sparse,
keyed by input-pin index; `BTreeMap` for deterministic serialization order).

**Invariant: the map never contains an `Auto` entry** — `Auto` is represented
by absence. `Auto` exists in the enum only as the API/UI surface value; the
`StructureDesigner`-level setter normalizes `Auto` to entry-removal, and the
`.cnnd` loader heals hand-authored files the same way (prune `Auto` entries,
like the other loader-healing invariants). This keeps "no overrides" a single
canonical state (empty map ⇒ `skip_serializing_if` fires ⇒ byte-stable files)
and makes the undo command's `Option<FunctionPinRole>` (below) mirror entry
presence exactly.

Like `arguments`, the map is index-keyed and therefore shares the existing
pin-identity hazard when a custom node type's pin layout changes;
`repair_node_network` prunes out-of-range entries, and partition logic ignores
any that remain.

### Semantics (the partition table)

For each input pin `i` of node `N`, the **effective role** is:

| Stored role | Pin unwired | Pin wired |
|---|---|---|
| `Auto` (absent) | **parameter** (declared pin type) | **capture** (wire pre-evaluated & frozen at `-1` eval) |
| `Delayed` | **parameter** (declared pin type) | **parameter**; wire = preview + type witness (see below) |
| `Supplied` | **stored-value capture** (body argument left empty → `NodeData` stored-data fallback at invocation) | **capture** (identical to Auto-wired) |

Parameters keep ascending pin order. All-supplied → a legal `() -> R` thunk
(already allowed since captures Phase 1).

**Multi-wire (array) input pins.** `Argument.incoming_wires` is a list — array
pins accept several wires — so "wired" in the table means **≥ 1 incoming
wire**, and a role applies to the whole pin, never per wire. `Delayed`+wired
drops **all** of the pin's wires from the body (they are all preview);
`Supplied`+wired captures **all** of them (identical to Auto+wired, which
already pre-evaluates the full wire list). The type witness (next section)
applies only to single-wire pins; a multi-wire `Delayed` pin's parameter type
is the declared pin type (multi-wire pins are declared as concrete
`Array[T]`s, so there is nothing for a witness to resolve).

**One shared partition helper.** The split is computed in exactly one place —
a free function in `node_network.rs` (it needs only `Node` + `NodeType`, and
`node_type_registry.rs` must not depend on the evaluator, so `zone_closure.rs`
is the wrong home):

```rust
pub enum FunctionPinDisposition {
    Parameter,        // Delayed, or Auto+unwired
    CaptureWire,      // Supplied+wired, or Auto+wired
    CaptureStored,    // Supplied+unwired
}
pub fn function_pin_dispositions(node: &Node, node_type: &NodeType)
    -> Vec<FunctionPinDisposition>;
```

Both consumers — `resolve_output_type_detailed`'s `-1` arm
(`node_type_registry.rs`) and `build_node_function_closure`
(`evaluator/zone_closure.rs`) — call it. Divergence between the resolver and
the synthesizer would be a type-unsoundness bug; a single helper makes it
structurally impossible.

### Type resolution (the preview-wire type witness)

`resolve_output_type_detailed`, `-1` arm:

- **Parameter types:** for each parameter pin — if the pin has exactly one
  incoming wire (`Delayed`+wired), the parameter type is the **resolved type
  of the wire's source pin** (the same source-type resolution `validate_wires`
  uses), falling back to the declared pin type if resolution fails; if unwired
  — or multi-wired (see above) — the declared pin type.
- **Return type:** `resolve_output_type(node, network, 0)` — unchanged. This is
  where the witness pays off: a `Delayed` preview wire feeds `same_as_input`
  resolution, so `structure_move` with `enter_structure` previewed into `input`
  resolves to `(Crystal) -> Crystal`. An unresolvable return (`None`) still
  rejects the `-1` connection, as today.

Worked example (the issue's exact case): `structure_move` with `input` marked
`Delayed` + preview-wired from a `Crystal` source, `translation`/`subdivision`
unwired and marked `Supplied` → function type `(Crystal) -> Crystal`, stored
translation/subdivision baked in.

### Closure synthesis

`build_node_function_closure` partitions via the shared helper and builds the
one-node synthetic body as today. Per-disposition behavior (`CaptureWire` is
unchanged from today; the other two arms change):

- `Parameter` (including `Delayed`+wired): body argument =
  `ZoneInput { pin_index: dense_param_index }` at depth 1. **The original wire
  is dropped from the body** — this is what makes the preview wire
  invocation-inert.
- `CaptureWire`: unchanged — original wires rebased `+1`, pre-evaluated by
  `build_captures`.
- `CaptureStored`: body argument left as an **empty `Argument`**. The body node
  is a clone of `N` *including its `NodeData`*, and node `eval` implementations
  already fall back to stored data on unwired pins (e.g. `structure_move`'s
  `evaluate_or_default(pin, self.translation, ..)`), so the stored value
  applies at invocation with zero new evaluation machinery. Freeze timing:
  the data is cloned into the body when the `-1` pin is evaluated — the same
  once-per-consumer-eval timing as wire captures, and a gizmo drag dirties the
  node so consumers re-pull a fresh closure.

`ZoneClosure.param_types` must be produced by the same resolution as the
resolver's parameter types (witness-resolved where available) so
`infer_data_type` on the function value agrees with the wire-level type.

### Validation

- **New non-blocking warning** (`ValidationError::warning`, per the blast-radius
  litmus test — the runtime already localizes the failure): a pin marked
  `Supplied`, unwired, whose parameter is **required**
  (`get_parameter_metadata`) — the stored-data fallback does not exist for
  required pins, so invocation would yield a localized `NetworkResult::Error`.
  Surfacing it at the node beats a mystery error inside a distant HOF.
  The warning fires **only when the node's `-1` pin is consumed**
  (`function_pin_consumed`): on an unconsumed node the roles are inert, so
  warning there would be pure noise — and gating keeps every
  validation-visible effect of a role toggle confined to consumed nodes,
  which is exactly the condition the undo path's conditional revalidation
  keys on (see Undo). Consumption itself changes only via `-1` wire
  connect/delete, and those forward paths already revalidate (captures
  Phase 1), so the warning appears/disappears with consumption for free —
  the **undo** side of those wire edits needs verification (see the warning
  round-trip bullet in the Phase 1 tests).
  **Emission site:** the accumulating pass (`validate_zones_recursive`) —
  `validate_wires` / `validate_parameters` short-circuit on first error, so
  they can only host blocking rules (see `structure_designer/AGENTS.md`
  §"Validation errors: blocking vs non-blocking").
- Existing `-1` wire type-checking (`validate_wires` via the wiring-aware
  resolved type) covers everything else with no new code: toggling a role
  changes the resolved `-1` type, and a now-mismatched consumer wire is
  re-flagged on the next validate pass. Note this is a **blocking** error like
  every `validate_wires` mismatch (that pass short-circuits), so the network
  stops evaluating until the user fixes the role or the wire — standard
  type-mismatch behavior, not new severity.

### Revalidation & propagation

Toggling a role is type-visible to `-1` consumers (`apply` derived arg-pin
layouts, `map` output types, wire validity). The `StructureDesigner`-level
setter therefore runs `validate_active_network()` after the mutation (the
apply/map/zip layout post-passes already run on every revalidate). This mirrors
the connect/delete-wire triggers keyed on `function_pin_consumed` from captures
Phase 1 — same propagation paths, same tests to imitate.

Note the witness makes the `-1` type depend **transitively** on the preview
wire's upstream chain (rewiring `enter_structure`'s source can change the
resolved parameter/return types). No per-edit tracking is added for this:
type resolution is recursive at validate time, so any edit path that
revalidates picks it up. If an edit path is found that skips validation after
an upstream type-affecting change, extend it the same way captures Phase 1
extended the wire-edit paths.

### Undo

`SetFunctionPinRoleCommand`, structured like `SetCollapseModeCommand`
(`undo/commands/set_collapse_mode.rs`): `{ network_name, scope_path, node_id,
pin_index, old_role: Option<FunctionPinRole>, new_role: Option<FunctionPinRole>,
description }` (`None` = entry absent / `Auto`), resolving through
`ctx.network_in_scope_mut`. Setter no-ops (and pushes nothing) when the role
is unchanged.

**Refresh mode depends on the scope**, because the `NodeDataChanged` path is
top-level-only on both of its legs: `mark_node_data_changed` marks
`NodeRef::top(node_id)` (`structure_designer_changes.rs`), and the arm's
conditional revalidation checks `function_pin_consumed` against the **active
top-level network** (`apply_undo_refresh_mode`). For a body node it would
dirty the wrong node on an id collision (per-body `next_node_id` counters make
collisions routine) and skip the revalidation the `-1` consumer needs — the
consumer lives in the body network. So:

- `scope_path` empty → `NodeDataChanged(vec![node_id])`. The arm re-validates
  when the node's function pin is consumed (added in captures Phase 1 for
  exactly this class of change), which — given the `Supplied`-required warning
  is gated on consumption (see Validation) — covers every validation-visible
  effect of the toggle.
- `scope_path` non-empty → `UndoRefreshMode::Full`. It re-validates (recursing
  into bodies via `validate_zones_recursive`) and reapplies display policy,
  matching what the forward setter's `validate_active_network()` produced.
  Body role edits are rare enough that the blunter refresh is fine; do **not**
  ship the top-level `NodeDataChanged` mode for body scopes.

### Serialization

`SerializableNode` gains
`#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
function_pin_roles`. Additive + defaulted → old files load unchanged, files
without overrides serialize byte-identically (keeps `.cnnd` fixtures and
node-snapshot tests green), **no migration, no version bump**. Copy/paste,
`duplicate_node`, and body snapshots inherit the field automatically
(`Node::clone` / `SerializableNode`).

Text format: not represented in v1 (precedent: `collapse_mode`). Consequence to
document: a whole-network text edit that rebuilds nodes will drop roles —
same caveat as other non-text node state. Follow-up if it bites.

### Display relaxation (function-mode nodes are displayable)

Remove the two hard suppressions:

1. **Rust:** the `function_pin_consumed` early-return in
   `NetworkEvaluator::generate_scene` (`network_evaluator.rs:566`). A
   function-mode node is then treated like any other node: rendered iff the
   display policy / per-pin eyes say so, evaluated normally for pin 0.
2. **Flutter:** the greyed-out eye branch in `node_widget.dart`
   (`_buildOutputPin`, the `functionConsumed` conditional) — restore the normal
   eye toggle. `NodeView.function_pin_consumed` stays (it still gates other
   behavior and is useful for a future badge).

Consequences, all intended:

- Under the **Selected** display policy, clicking a function-mode
  `structure_move` shows its pin-0 output and — because top-level evaluation
  now runs and populates the eval cache — its **drag gizmo**. This is the
  user's "locally usable gizmo, applied later" workflow.
- Under **Frontier**, function-mode nodes stay auto-hidden: frontier-ness is
  "no downstream dependents" per `build_reverse_dependency_map`
  (`node_network.rs:884`), which registers every incoming wire regardless of
  pin index — so the consumer's `-1` wire counts and the function node is
  non-frontier (verified). Default visual noise does not increase.
- A displayed function-mode node with an unwired required `Delayed` pin shows a
  normal localized error ("missing input") — accurate and explicit; wire a
  preview or hide the pin's eye. This replaces silent invisibility with
  standard node semantics.
- Disconnecting `f` still restores everything for free — nothing is stored.

The `doc/design_function_pins.md` §"Display in function mode" rationale is
superseded by this section; update that doc with a pointer here.

### API & Flutter UI

**API** (`rust/src/api/structure_designer/`, both `scope_path`-taking per the
scoped-getters rule):

- `get_function_pin_roles(scope_path, node_id) -> Vec<APIFunctionPinRole>` —
  one entry per input pin: `{ pin_name, role, wired, effective:
  "parameter" | "capture-wire" | "capture-stored" }` (effective computed via
  the shared helper, so the UI never re-derives the table).
- `set_function_pin_role(scope_path, node_id, pin_index, role)`.
- FRB regen after.

**Flutter:** a generic **"Function output"** section rendered by
`node_data_widget.dart` for every selected node with ≥ 1 input pin, below the
per-node-type editor (scoped via `propertyEditorScopePath`, like every sibling
editor). Per pin: name + a compact three-way selector (Auto / Delayed /
Supplied) + the effective-disposition annotation (e.g. "parameter",
"captures wire", "uses stored value"). The section is collapsed by default
unless the node's function pin is consumed or any non-Auto role is set. The
title-bar `-1` pin tooltip already shows the function type and will reflect
role changes via the existing wiring-aware `NodeView.function_type`.

Polish (deferred unless cheap): a small marker on `Delayed`+wired input pins in
the node widget, so a "this wire is preview-only" state is visible in the graph.

### Reference guide

Update `doc/reference_guide/node_networks.md` (function-pin section): the role
table, the preview-wire idiom (with the `structure_move` gizmo example), and
the new display behavior of function-mode nodes.

## Phases

### Phase 1 — Backend core ✅ DONE

Implementation notes / deviations from the plan as written:

- **The synthesizer's return type is now resolved, not declared.** The plan only
  mentioned rewriting the resolver's parameter types, but
  `build_node_function_closure` read `node_type.output_type()` — which is
  `DataType::None` for a `same_as_input` pin, so it rejected `structure_move`
  outright regardless of roles. Both the params *and* the return now come from
  one shared `NodeTypeRegistry::resolve_function_pin_signature[_scoped]`, which
  returns the **un-canonicalized** `(params, return)` pair (the resolver wraps it
  in `FunctionType::new`; the closure's `param_types` must stay the body's actual
  frame size, which the flattened type would break when pin 0 returns a
  `Function`). This is the type-lock-step invariant's real home — the
  `function_pin_dispositions` helper only shares the *partition*.
- **The `-1`-consumer undo fix is `Full`, not an extended id-list.** The plan's
  prescribed "cheapest fix" (add the `-1` wire's source id to
  `ConnectWireCommand` / `DeleteWiresCommand`'s `NodeDataChanged` lists) does not
  work, and the predicted test caught it: the arm tests `function_pin_consumed`
  **after** the undo, so the leg that *removes* consumption always reads "not
  consumed" and skips the revalidation it needs — listing the source changes
  nothing. Both commands now report `UndoRefreshMode::Full` when the wire is a
  `-1` wire. (This also fixes the pre-existing staleness for consumers' derived
  `apply`/`map` layouts across an undone `-1` connect.)
- The non-blocking-warning test uses `materialize` rather than the design's
  `structure_move` example: `structure_move`'s `same_as_input` pin 0 leaves its
  required `input` unwired ⇒ it *also* trips the pre-existing **blocking**
  "polymorphic output could not be resolved" rule, which would mask the blast
  radius under test. `materialize` has the same shape (required `shape`) but a
  `Fixed(Crystal)` output.

`FunctionPinRole` + `Node.function_pin_roles` + serialization; shared
`function_pin_dispositions` helper; resolver `-1` arm + synthesizer rewrites;
`Supplied`-required-unwired warning (gated on `-1` consumption); repair
pruning + loader healing;
`StructureDesigner::set_function_pin_role` (+ revalidate) +
`SetFunctionPinRoleCommand`.

Tests (`tests/structure_designer/function_pin_test.rs` + `undo_test.rs` +
`cnnd_roundtrip_test.rs`):

*Partition & typing*
- Partition combos: each role × wired/unwired; all-supplied thunk.
- Multi-wire pin: an array pin with two incoming wires under each role —
  Auto/Supplied capture all wires, Delayed drops all wires from the body, and
  the parameter type is the declared `Array[T]` pin type (no witness).
- The issue's end-to-end case: `structure_move`, `input` Delayed+previewed,
  stored translation set, others Supplied → type `(Crystal) -> Crystal`;
  invoke via `apply` → output moved by the **stored** translation, preview
  wire ignored.
- Witness typing: param type from preview wire; declared-type fallback when the
  source type doesn't resolve.
- Transitive witness update: retype the preview wire's *upstream* (e.g. swap
  the source feeding the previewed `enter_structure`) → the `-1` type and its
  consumer re-derive on the next validate pass.
- Connection gating: before roles, `structure_move.-1` rejected by a
  `(Crystal) -> Crystal` consumer (`can_connect_nodes`); after
  Supplied/Delayed+preview setup, accepted. Reverting the role re-flags the
  existing wire on revalidate.
- An **erroring preview source** does not poison the function: Delayed+wired
  pin whose upstream evaluates to `Error` — the `-1` closure still builds and
  invokes correctly (the wire is dropped from the body), while pin 0 shows the
  error normally.

*Value freshness (dirty propagation)*
- Editing the stored data of a Supplied pin (simulating a gizmo drag via
  `set_node_data`) dirties the `-1` consumer: re-evaluating `apply` reflects
  the **new** stored translation, not a cached closure. (This exercises
  `data_changed` propagation across a `-1` wire — historically a
  hole-prone path, cf. the zones dirty-propagation fix.)

*Validation & repair*
- Supplied + unwired + required pin → warning is present, is
  **non-blocking** (`network.valid` stays true, unrelated nodes still
  evaluate), and invocation yields a localized `NetworkResult::Error`, not a
  panic.
- Warning gating on consumption: same node with **no** `-1` consumer → no
  warning; connect a consumer → warning appears on the triggered revalidate;
  delete the consumer wire → warning gone. Undo/redo of a role toggle on an
  **unconsumed** node leaves no stale warning (the roles are inert there, so
  there is nothing for the skipped revalidation to miss).
- Warning round-trips through **undo of the consumer connect** too: connect
  `-1` consumer (warning appears) → undo (warning gone) → redo (warning
  back). Caution: the wire commands' `NodeDataChanged` id-lists carry the
  **dest** (consumer) node, and the arm's revalidation checks
  `function_pin_consumed` on those ids — the node whose consumption changed
  is the wire's **source**. If the test shows the arm missing this, add the
  `-1` wire's source node id to `ConnectWireCommand` / `DeleteWiresCommand`'s
  refresh id-lists (cheapest fix, same shape as the captures-Phase-1 arm).
- Repair pruning: shrink a node's pin layout (custom-node-type change) →
  out-of-range role entries are pruned; partition ignores any that remain.
- Loader healing: a hand-authored `.cnnd` with an explicit `Auto` entry loads
  with the entry pruned (the map invariant holds after load); setting `Auto`
  via the setter removes the entry rather than storing it.

*Persistence & undo*
- Undo/redo on a **top-level** node restores the role and re-derives consumer
  state (`NodeDataChanged` arm revalidation); setter no-ops push no command.
- Scoped setter: role set on a node **inside a closure/HOF body**
  (non-empty `scope_path`) round-trips through undo via
  `network_in_scope_mut`, and — because body-scoped commands use the `Full`
  refresh mode — a **body-internal `-1` consumer's** derived type is
  re-derived after undo/redo. Assert the consumer's type, not just the role
  value: the top-level `NodeDataChanged` legs don't reach body scopes, so a
  role-only assertion would pass even with the wrong refresh mode.
- Serialization roundtrip with roles (top-level and body node);
  files without overrides serialize **byte-identically** (fixture diff);
  copy/paste and `duplicate_node` preserve roles.

### Phase 2 — Display relaxation

Remove the `generate_scene` skip and the Flutter grey-eye branch; update
`design_function_pins.md`.

Tests:
- Scene-level: a function-consumed node with its pin-0 eye on contributes
  scene output; the same node under the Frontier policy is auto-hidden (its
  `-1` wire makes it non-frontier).
- **Gadget availability, headless**: select a function-mode `structure_move`
  with a preview wire, run `generate_scene`, assert
  `selected_node_eval_cache` is populated (the gadget precondition —
  `provide_gadget` reads it). Existing precedent:
  `multi_output_unit_test.rs` / `continuous_minimization_test.rs` already
  assert this cache.
- Regression: the full existing `function_pin_test.rs` suite passes unchanged
  (the skip removal must not alter evaluation results, only visibility).

Manual walkthrough (`flutter run`): gizmo drag on a displayed function-mode
`structure_move` with preview wire; verify consumers re-evaluate with the new
translation, and that Frontier keeps the graph visually quiet.

### Phase 3 — API + sidebar UI + guide

API getter/setter, FRB regen, `node_data_widget.dart` generic section, manual
walkthrough (thin editor UI → manual verification per project convention),
reference-guide update.

Tests: one Rust-side test that `get_function_pin_roles`' `effective` field
matches `function_pin_dispositions` for a node mixing all three roles ×
wired/unwired (the UI renders this field verbatim, so it must not re-derive —
or silently disagree with — the partition). The API pair is otherwise a thin
scoped wrapper (test-exempt per `rust/AGENTS.md`); the editor UI is verified
manually per project convention.

## Risks & mitigations

- **Resolver/synthesizer divergence** → single shared partition helper (the
  design's one structural invariant).
- **Stale consumer types on toggle** → reuse the captures-Phase-1 revalidation
  paths; regression tests mirror that phase's propagation tests.
- **Pin-index keying vs. dynamic pin layouts** → same exposure as `arguments`;
  repair prunes; partition ignores out-of-range. Documented, not solved (the
  general fix is the stable-pin-id work tracked elsewhere, cf. issue #377).
- **Display relaxation surprises** (old files with stored display state on
  function-mode nodes start rendering again) → they render only what the
  policy/eyes already claim; worst case is an error badge the user can hide.
  Explicitly called out in the guide update.
- **Text-format round-trip drops roles** → documented v1 limitation, shared
  with `collapse_mode`.
- **Paste skips validation (issue #326)** → a pasted function node + `-1`
  consumer pair carries stale derived types until the next validate pass,
  exactly like pasted `apply`/`closure` today. The roles themselves ride
  along in the snapshot correctly; the staleness is the pre-existing #326
  class, not new state introduced here — noted so it isn't rediscovered as a
  roles bug.

## Open questions

1. Should the sidebar expose roles for pins that cannot meaningfully differ
   (e.g. `apply`'s derived arg pins, HOF `f` pins)? Proposal: hide the section
   for nodes whose type is derived (`apply`) and for zone-bearing nodes' `f`
   pin; revisit if someone asks.
2. Should a `Delayed` pin's preview wire render dimmed/dashed in the graph?
   Deferred to polish; needs a `NodeView` flag either way.
3. `same_as_input` disconnected-fallbacks (e.g. giving `structure_move` a
   `HasStructure` fallback like `atom_edit`'s `Molecule`) would let an
   unwitnessed `(HasStructure) -> HasStructure` type resolve. Orthogonal,
   cheap, but changes pin-0 typing generally — keep out of scope here.
