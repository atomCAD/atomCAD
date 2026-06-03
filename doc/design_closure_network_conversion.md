# Design: Convert Closure ⇄ Custom Node Network

## Summary

Add two mutually-inverse operations to the node editor:

- **Convert to Closure** (*Network → Closure*): right-clicking a **custom-network instance
  node** `I` whose function pin is being used (or that is unconsumed) replaces it with a
  `closure` node `C` whose inline body is a copy of `I`'s network `N`. `I`'s **wired** input
  pins become **captures** inside the body; its **unwired** input pins become the closure's
  **parameters** (zone-input pins).
- **Extract to Network** (*Closure → Network*): right-clicking a `closure` node `C` creates a
  new named custom network `N` from `C`'s body and replaces `C` with an instance `I` of `N`.
  The closure's **parameters** (zone-input pins) and its **captures** both become **parameter
  nodes** of `N`; `I`'s capture-pins are wired to the original capture sources, and `I` is
  used through its **function pin**.

The two are exact inverses (up to fresh ids / names). They are the closure-aware analogue of
*Factor Selection into Subnetwork* (`selection_factoring.rs`) and *Inline a Custom Node*
(`node_inlining.rs`, `doc/design_inline_custom_node.md`), and they reuse that machinery
wherever possible.

## The semantic bridge: the function pin

These conversions only make sense because a **function value** has two equivalent
representations in the graph, and the function pin (`output_pin_index == -1`) is the bridge
between them.

Recall (`doc/design_node_function_pin_captures.md`, and the shipped
`build_node_function_closure` in `evaluator/zone_closure.rs`): a node's function pin yields a
`Function` whose parameters are the node's **unconnected** input pins (in ascending pin
order, densely renumbered) and whose **connected** input pins are frozen as **captures**. The
old "function pin and input pins are mutually exclusive" rule is **gone** — wired inputs on a
node whose `-1` pin is consumed are the normal capture idiom (see the comments in
`node_network.rs::can_connect_nodes` and `network_validator.rs`). The wiring-aware type is
`resolve_output_type(node, …, -1) = (unwired pin types) -> (pin-0 type)`.

So for a custom-network instance `I` of `N`:

| `closure` node `C`                        | instance `I` of `N`, used via its function pin `-1` |
|-------------------------------------------|-----------------------------------------------------|
| zone-input pin `p` (a closure parameter)  | **unwired** input pin → parameter of the `-1` value |
| a capture wire in the body                | a **wired** input pin → capture of the `-1` value   |
| zone-output (result) wire                 | output pin 0 of the return node                     |
| `C`'s output pin 0 (the `Function` value) | `I`'s function pin `-1` (the `Function` value)       |

Both produce a `Function` of type `(unwired/zone-input types) -> (result type)` with the same
captures frozen at the same time (both `C` and `I` live in the same host scope `H`, so the
freeze cadence — once per evaluation of `H`, i.e. once per outer iteration when `H` is a loop
body — is identical). The conversions are graph rewrites that move between these two
representations; the function-pin machinery is the proof they are semantically equivalent, not
something we invoke at conversion time.

**Consumer redirection.** Because the two representations expose the `Function` on **different
pins** (`C` on pin `0`, `I` on pin `-1`), the single externally-visible change is: every wire
that consumed the old function-value pin must point at the new one. We **reuse the node's id**
(replace the `Node` entry in place), so this reduces to flipping the `source_pin` pin index
`0 ⇄ -1` on consuming wires — across the host scope *and* its sub-bodies (a sibling/inner HOF
body can capture the function value).

### Terminology

- `C` — the `closure` node; `B` — its inline body (`C.zone`, a `NodeNetwork`).
- `I` — the custom-network instance node; `N` — its definition (`registry.node_networks[name]`).
- `H` — the **host scope**: the network or HOF body directly containing `C` / `I`, resolved by
  `scope_path` (empty = top-level active network). The conversions never add a scope level
  *to* `H`; *Network → Closure* introduces one new body `B` one frame **below** `H`.
- **nesting `k`** — for a wire living on a node inside a freshly-built/copied tree, the number
  of body frames between that node and the tree's top. The tree top is `k = 0`; each body
  descent increments `k`.

---

## Wire model recap (depths and captures)

A wire is `IncomingWire { source_node_id, source_pin: SourcePin, source_scope_depth: u8 }`
where `SourcePin ∈ { NodeOutput { pin_index: i32 }, ZoneInput { pin_index: usize } }`.

Inside a body, `source_scope_depth = d` means "walk `d` ancestor frames up from the body that
holds this wire":

- `NodeOutput`, `d == 0` — intra-body wire (source in the same network).
- `NodeOutput`, `d ≥ 1` — **capture** of an ancestor node's output.
- `ZoneInput { pin }`, `d == 1` — per-iteration reference to the **immediately enclosing** HOF's
  zone-input pin (`element` / `acc`). Source is the owning HOF/closure node id. **Not** a
  capture.
- `ZoneInput { pin }`, `d ≥ 2` — capture of a deeper enclosing HOF's iteration value.

`is_capture` (`zone_closure.rs`): `NodeOutput ⇒ d > 0`; `ZoneInput ⇒ d > 1`.

`CaptureKey { source_node_id, source_scope_depth, source_pin }` identifies a capture at the
**runtime/relative** level; for conversion we need an **absolute** identity (see below).

Only B-top (top-of-tree) nodes ever get fresh ids when copied; nested body `Arc`s are cloned
**verbatim**, so deeper ids are preserved. This is the invariant `copy_content_into`
(`node_inlining.rs`) already maintains and it is load-bearing for all the depth gates here.

---

## Direction A — Network → Closure (custom instance ⇒ closure)

This is *Inline a Custom Node* placed into a fresh closure body, plus a parameter
classification step. We reuse `node_inlining::copy_content_into` verbatim and a
**closure-flavoured splice**.

### Gate (orchestrator)

1. `I.node_type_name` resolves to a custom network: `registry.is_custom_node_type(name)`.
   This rejects built-ins, HOFs, `apply`, and `closure` in one check (none are custom types).
   Else error *"Only custom node instances can be converted to a closure"*.
2. `I` must be used as a **function**, not a value: **no wire anywhere consumes `I`'s normal
   output pins (index ≥ 0)**. (Consumers of `I`'s `-1` pin are fine — they get redirected.
   No consumers at all is also fine — the resulting closure is left unconsumed.) Computed by a
   Descent-B-style walk of `H` and all its sub-bodies. Else error *"This node is used as a
   value, not a function; only a node consumed through its function pin can be converted to a
   closure"*.
3. `N` must have a `return_node_id` (a closure must deliver a result). Else error *"The custom
   network has no return node"*.

### Build `C`

`C` reuses `I`'s id and position. We clone `N` (`registry.node_networks[name].clone()`) and
read from it while mutating `H`.

**Parameter classification.** Partition `N`'s `parameter` nodes by whether `I`'s corresponding
input pin is wired:

- **unwired pin `p`** → a **closure parameter**. Assigned a dense closure-param index `cp` in
  **ascending pin order** (so the closure's zone-input pin order matches `resolve_output_type(-1)`
  for the round-trip).
- **wired pin `p`** → a **capture**. `I.arguments[p]`'s wire(s) `iw` are the capture
  source(s).

The closure's shape: `params = (types of unwired pins, in cp order)`, `ret = N.output pin-0
type`. Build `ClosureData` via the existing `closure_data_for_signature(params, ret)` helper in
`nodes/closure.rs` (make it `pub(crate)`), which picks a preset `ClosureKind` when the shape
matches and `Custom` otherwise. `calculate_custom_node_type` then derives `C`'s
`zone_input_pins` (the parameters) and single `zone_output_pin` (the result).

> *Naming note:* presets relabel parameters to `element`/`acc`. To **preserve `N`'s parameter
> names**, prefer `ClosureKind::Custom` with `param_names` = the unwired parameter nodes' names.
> Either is functionally identical (function-type compatibility is structural, kind-independent);
> the design recommends `Custom` for name fidelity and notes the preset path as the
> drag-consistent alternative.

**Body `B`.** `B = NodeNetwork::new_empty()`. Copy `N`'s non-`parameter` nodes into `B` with
`copy_content_into(&mut B, &N, anchor=ZERO, content_min)` → `id_mapping` (B-top nodes get fresh
ids from `B.next_node_id`; nested body `Arc`s verbatim). Then splice (below). Finally
`C.zone = Some(Arc::new(B))`, `C.zone_output_arguments = [Argument { incoming_wires: [result
wire] }]`, set `C.data = ClosureData`, recompute `C.custom_node_type` via
`set_custom_node_type`/`ensure_zone_init` against the `closure` `NodeType`.

### The Network → Closure splice (per wire, at nesting `k` in `B`)

For every wire `w` on every copied node's `arguments` **and** the wire(s) that will become
`zone_output_arguments` (initially `N`'s return reference), classify by reference target. Let
`d_I(p)` denote the source depth of the wired pin `p`'s instance wire `iw` (`0` for a normal
same-scope wire; `≥ 1` if `I` itself captures).

- **Internal reference** — `w.source_node_id ∈ id_mapping` **and `w.source_scope_depth == k`**:
  reference reaches `B`-top (the copied nodes with fresh ids). Remap
  `source_node_id = id_mapping[old]`; keep `source_pin`, keep depth. (Covers `NodeOutput` to a
  sibling and a nested HOF's `ZoneInput` pointing at its copied owner at `B`-top.)
- **Internal reference, deeper** — `w.source_scope_depth < k`: points into an intermediate
  verbatim-cloned sub-body (preserved ids). **Leave verbatim.**
- **Reference to a `parameter` node of `N`** — `w.source_node_id` is one of `N`'s parameter
  nodes (read from `N`, depth-independent because parameter references in `N` are always at the
  parameter's own scope): this is the boundary. Split by the pin's classification:
  - **closure parameter `cp`**: replace `w` with
    `IncomingWire { source_node_id: C.id, source_pin: ZoneInput { pin_index: cp }, source_scope_depth: k + 1 }`.
    (From nesting `k`, `C` — the body's owner — is `k + 1` frames up.)
  - **capture pin `p`**: replace `w` with the instance wire(s) `iw`, each rebased to reach the
    same physical source from inside the body:
    `IncomingWire { source_node_id: iw.source_node_id, source_pin: iw.source_pin, source_scope_depth: (k + 1) + iw.source_scope_depth }`.
    (The `+1` beyond the inline formula `k + iw.depth` is exactly the extra frame `B` adds below
    `H`.) Multiple wires on a multi-input pin replicate; an empty instance pin drops `w`.

> **Why parameter references are matched by id against `N` directly (not gated on depth):** in
> `N`, every wire that reads a parameter node does so at the parameter's scope — `parameter`
> nodes only live at `N`-top, so a body-internal wire reading a parameter is a *capture inside
> `N`* with depth = its own nesting. When copied into `B`, those wires' `source_node_id` is the
> parameter node's `N`-id, which is disjoint from `id_mapping`'s copied-node values (parameters
> aren't copied) and from `C.id`. So the three classes — copied node / parameter node / nothing
> else — never alias *within a single nesting frame's wires* once we apply the
> `source_scope_depth == k` gate to the copied-node class. (This is the same hazard
> `node_inlining.rs` documents; the gate resolves it.)

**Return / result wire.** `N.return_node_id = R`. The closure's `zone_output_arguments[0]` wire
is derived from "read `R`'s output pin 0":
- If `R` is a copied node: `IncomingWire { source_node_id: id_mapping[R], NodeOutput { 0 }, depth: 0 }`.
- If `R` is itself a `parameter` node (the network just passes a parameter through): route it
  the same way the splice routes a reference to that parameter at `k = 0` — to `ZoneInput { cp }`
  depth 1 (closure param) or to the instance wire (capture). This is the one place the result
  wire can be a non-`NodeOutput` source; it is valid (a closure may return its argument).

**Redirect consumers.** Flip every wire that read `(I.id, NodeOutput { 0/1/… })`... no — `I`
had **no** function-value-producing pin 0 distinct from the network output; its *function* value
was on `-1`. Concretely: every wire consuming `(I.id, NodeOutput { pin_index: -1 })` (across `H`
and sub-bodies, any depth) becomes `(C.id, NodeOutput { pin_index: 0 })` at the same depth.
Since `C.id == I.id`, only `pin_index` changes (`-1 → 0`).

### Undo (Network → Closure)

`N` is **not** modified; only `H` changes. Mirror *Inline*:
- top-level `H`: `InlineNodeCommand`-shaped before/after `SerializableNodeNetwork` of the active
  network (rename to a shared `ReplaceNodeCommand` or add `ConvertToClosureCommand`; the
  before/after-one-network shape is identical).
- body `H`: `snapshot_zone_body` / `push_zone_body_command` (`EditZoneBodyCommand`).

---

## Direction B — Closure → Network (closure ⇒ custom instance)

This is the inverse: lift `C`'s body `B` into a new standalone network `N` (with parameter
nodes for both the closure's parameters and its captures), and replace `C` with an instance `I`
wired so that `I`'s `-1` value reproduces `C`'s.

### Gate (orchestrator)

1. `C.node_type_name == "closure"`. (HOFs/`apply` rejected — extracting an HOF's *inline* body
   is a separate future feature; the closure node is the one that produces a reusable function
   value.) Else error *"Only closure nodes can be extracted to a network"*.
2. `C` has a result wire (`C.zone_output_arguments[0]` non-empty). Else *"The closure has no
   result"* (malformed).
3. The result wire's source pin is **not** a secondary output (`NodeOutput { pin_index: i }`,
   `i > 0`) of a multi-output body node — that can't map to a single return-node-pin-0 cleanly.
   Else *"The closure result comes from a secondary output pin"*. (Rare; clean rejection rather
   than synthesizing a passthrough.)
4. A network name (collected by a small dialog, like factoring — but only the **name**, since
   parameter names are auto-derived).

### Collect captures (the heart of the depth handling)

Walk `B` recursively, tracking nesting `k` (B-top `k = 0`). For every wire `w` (on every body
node's `arguments`, plus the result wire), classify:

- **Intra-body** — `NodeOutput`, `w.source_scope_depth ≤ k`, **or** any `ZoneInput`/`NodeOutput`
  whose `source_node_id` is a node living **inside `B`** at the frame `k − w.source_scope_depth`:
  internal to the closure body. (After lifting to `N` these stay internal — id-remapped for
  B-top sources, verbatim for deeper.) Not a capture.
- **Closure parameter** — `ZoneInput { pin_index: p }`, `source_node_id == C.id`
  (necessarily `w.source_scope_depth == k + 1`): a reference to `C`'s own zone-input pin `p`.
- **Capture** — everything reaching **at or above `H`** (`C`'s host scope): either
  `NodeOutput` with `w.source_scope_depth ≥ k + 1`, or `ZoneInput` with `source_node_id ≠ C.id`
  and reaching above `C`. Its **external level** is `e = w.source_scope_depth − (k + 1) ≥ 0`
  (`e = 0` → source in `H`; `e ≥ 1` → `e` frames above `H`).

**Absolute capture identity** = `(e, source_node_id, source_pin)`. Two wires (possibly at
different nestings `k`, hence different `source_scope_depth`) denote the **same** capture iff
their absolute identity matches — at external level `e` the referenced ancestor scope is fixed,
so `source_node_id` is unambiguous there. Dedup captures by this key; assign each a stable order
(first-encounter in a deterministic walk: sorted node ids, then arg index, descending into
bodies).

> **This is the "capture depth > 1" case the design must get right.** A capture with `e = 0`
> reached from a wire at nesting `k` has `source_scope_depth = k + 1`; the same capture reached
> from a deeper sub-body (nesting `k′ > k`) has `source_scope_depth = k′ + 1` — *different*
> relative depths, *same* absolute capture, *same* `N` parameter, *same* `I` pin. A capture
> with `e ≥ 1` arises whenever a wire inside one of `B`'s nested bodies reaches past `H` (e.g. a
> `map` inside the closure body whose body references a grandparent constant). All of these are
> representable; see the correctness argument.

### Build `N`

A new `NodeNetwork` (like `create_subnetwork_from_selection`). Its `parameter` nodes, in order:

1. **Closure parameters** (param_index `0..m`), one per `C` zone-input pin, types from `C`'s
   zone-input pin types, names from the zone-input pin labels. (Ascending `cp` order → matches
   the round-trip.)
2. **Captures** (param_index `m..m+c`), one per distinct absolute capture, type = the resolved
   type of the capture source, name auto-derived from the source node (`custom_name` else
   `node_type_name`, suffixed `_cap`, de-duplicated).

`N.node_type.output_pins` = the return node's output pins (multi-output passthrough). Copy `B`'s
top-level nodes into `N` with fresh ids (`copy_content_into`-style; `B` has no `parameter`
nodes, so all top-level nodes copy), bodies verbatim.

### The Closure → Network splice (per wire, at nesting `k` in `N`)

- **`s == k` (`s = source_scope_depth`)** — reference reaches `N`-top: remap
  `source_node_id = id_mapping[old]`; keep `source_pin`, keep depth.
- **`s < k`** — intermediate verbatim sub-body reference: **leave verbatim.**
- **`s ≥ k + 1`** — boundary:
  - **closure parameter** (`ZoneInput`, `source_node_id == C.id`, so `s == k + 1`): rewire to
    `IncomingWire { source_node_id: paramnode[cp].id, NodeOutput { 0 }, source_scope_depth: k }`.
  - **capture** (`e = s − k − 1`): rewire to
    `IncomingWire { source_node_id: paramnode[capture].id, NodeOutput { 0 }, source_scope_depth: k }`.

(Both boundary classes rewire to a parameter node living at `N`-top, reached from nesting `k` at
depth `k`, on `NodeOutput` pin 0.) Set `N.return_node_id`:
- result wire reads a copied node `X`: `return_node_id = id_mapping[X]`.
- result wire reads a closure parameter / capture (a passthrough closure): `return_node_id =
  paramnode[…].id`.

### Build `I` and wire captures

`I` reuses `C`'s id and position, type `= N`'s name (`add_node_with_id` then
`set_custom_node_type` from the registry). For each parameter pin of `I`:

- **closure-parameter pin** (`0..m`): **leave unwired** — these become the `-1` value's
  parameters.
- **capture pin** (`m..m+c`): wire to the original capture source as seen from `H`. The capture
  has external level `e` and source `(source_node_id, source_pin)`; the wire on `I` is
  `IncomingWire { source_node_id, source_pin, source_scope_depth: e }`.
  - `e = 0` → a normal same-scope wire (`source_pin` is `NodeOutput`).
  - `e ≥ 1` → a **capture wire on `I`** (depth `e`). This requires `H` to have ≥ `e` ancestor
    frames — guaranteed, because an `e ≥ 1` capture only exists when `C` (hence `H`) is nested
    that deep (see correctness).
  - `ZoneInput` sources (iteration-value captures) carry through: `I`'s wire is
    `ZoneInput { pin_index }` at depth `e`.

**Redirect consumers.** Every wire consuming `(C.id, NodeOutput { 0 })` (across `H` and
sub-bodies, any depth) becomes `(I.id, NodeOutput { -1 })` at the same depth. `C.id == I.id`,
so only `pin_index` changes (`0 → -1`).

Register `N` (`registry.add_node_network(N)`), then
`initialize_custom_node_types_for_network` on `H`'s top-level network (the split-borrow walk
used by inline) so `I` and the copied content get their cached types, and
`validate_active_network()`.

### Undo (Closure → Network)

`N` is **created** (registry change) **and** `H` is mutated — like *Factor Selection*:
- top-level `H`: reuse `FactorSelectionCommand` (source-network before/after + subnetwork
  snapshot; `do`/`undo` already add/remove the subnetwork and restore the source by name).
- body `H`: a new `ExtractClosureBodyCommand { network_name (of N), subnetwork_snapshot,
  scope_path, body_before: ZoneBodySnapshot, body_after: ZoneBodySnapshot }`. `undo`: restore
  body + remove `N`; `redo`: re-add `N` + restore body-after. (Mirrors `FactorSelectionCommand`
  but with `ZoneBodySnapshot` for the host instead of a named-network snapshot.)

---

## Correctness argument

**Claim.** Both conversions preserve the function value (`Function` type and runtime semantics)
and the rest of the graph, and they are inverses up to fresh ids/names.

1. **Type preservation.**
   - *Network → Closure:* `C`'s declared output is
     `Function((unwired pin types in cp order), N.output[0])`. `I`'s `-1` type was
     `resolve_output_type(I, -1) = Function((unwired pin types in pin order), N.output[0])`. The
     dense renumbering preserves the order, so the types are identical.
   - *Closure → Network:* `I`'s `-1` type after conversion is
     `Function((unwired = closure-parameter pin types in cp order), paramnode-free pin-0 type)`
     = `Function(C.zone_input_pin types, C.result type)` = `C`'s declared output type.
2. **Capture set & freeze cadence.** Both `C` and the post-conversion `I` live in the **same
   host scope `H`**, evaluated at the same cadence. The capture *sources* are unchanged
   (the same physical ancestor outputs / iteration values). `build_node_function_closure(I)`
   rebases `I`'s wired-pin wires `+1` into its synthetic body, exactly undoing the `+1`/`e`
   bookkeeping that placed them on `I` — so the synthesized closure's capture wires have the same
   absolute targets as `C`'s body capture wires. Frozen-once-per-`H`-eval holds for both.
3. **Depth bookkeeping is total (no impossible case for `NodeOutput` captures).** A capture at
   external level `e` exists only if some wire in `B` (at some nesting `k`) has
   `source_scope_depth = k + 1 + e`. A wire can only carry a depth that the live scope stack can
   satisfy, so `B`'s stack has ≥ `k + 1 + e` frames above the wire, i.e. `H` has ≥ `e` ancestor
   frames. `I` lives in `H`, so it can express a depth-`e` wire. Symmetrically, lifting a body
   capture to `I` always drops the relative depth by exactly one frame (`B` sits one frame below
   `H`), which is always ≥ 0. Hence **every** configuration is representable — the feared
   "capture depth > 1" case is handled by the uniform `e`/`k` arithmetic, not rejected.
4. **`ZoneInput` captures** (iteration values captured from an enclosing HOF, `e ≥ 1`) follow
   the same arithmetic; `I` gets a `ZoneInput` wire at depth `e`, valid by the same stack-depth
   argument, and round-trips through `build_node_function_closure`'s `+1` rebase.
5. **Inverse property.** *Closure → Network → Closure* reproduces `C` up to ids/param-node names
   and the (cosmetic) `ClosureKind` choice: parameters map back from unwired pins in the same
   order, captures map back from wired pins with the same absolute targets, the result maps back
   from `N`'s return. *Network-instance → Closure → Network* reproduces a network instance whose
   `-1` value equals `I`'s. (These are the strongest tests — see below.)

**Rejections** are reserved for genuinely ill-defined / lossy inputs, not for capture depth:
non-custom / non-closure node kinds; instance used as a value (normal-output consumers present);
no return / no result; and a result drawn from a secondary output pin of a multi-output body
node.

---

## Module layout

New file `rust/src/structure_designer/closure_network_conversion.rs` (declared in
`structure_designer/mod.rs`), holding the pure, registry-light building blocks; it leans on
`node_inlining::{copy_content_into, content_bounding_box, make_space_for_inline}` and the
factoring helpers for the parts that are unchanged.

```rust
// Network → Closure: produce the closure node `C` (id = instance id) to drop in
// place of `I`, plus the consumer-pin flip. Reads cloned `N`, returns the new
// `Node` and the list of consuming wires to repoint.
pub fn build_closure_from_instance(
    instance: &Node,            // I (read: id, position, arguments)
    source: &NodeNetwork,       // N (cloned)
    registry: &NodeTypeRegistry,
) -> Result<Node /* C */, String>;

// Closure → Network: produce the new network `N` from the closure body and the
// classification of `C`'s body wires into (closure params, captures).
pub struct ExtractionPlan {
    pub network: NodeNetwork,                 // N (parameter nodes + copied body)
    pub capture_wires: Vec<IncomingWire>,     // one per capture pin, in pin order, as seen from H
    pub closure_param_count: usize,           // m (leading unwired pins on I)
}
pub fn extract_network_from_closure(
    closure: &Node,             // C (read: id, zone, zone_output_arguments, data)
    network_name: &str,
    registry: &NodeTypeRegistry,
) -> Result<ExtractionPlan, String>;
```

The two splices (the per-wire depth classification) are private helpers, each a small recursive
walk modeled directly on `node_inlining.rs`'s `DescentA`/`descend_body`.

## Orchestrators (`structure_designer.rs`)

```rust
pub fn convert_instance_to_closure(&mut self, scope_path: Vec<u64>, node_id: u64)
    -> Result<(), String>;          // Network → Closure
pub fn extract_closure_to_network(&mut self, scope_path: Vec<u64>, node_id: u64,
    network_name: &str) -> Result<u64, String>;   // Closure → Network (returns instance id == node_id)
```

Both are **scope-aware** (`get_scope_network[_mut](&scope_path)`), snapshot for undo per the
top-level/body split above, run the builders, repoint consumers, then
`initialize_custom_node_types_for_network` (for the copied content) + `validate_active_network()`
+ `is_dirty = true; mark_full_refresh()`. `extract_closure_to_network` additionally validates the
name (`identifier::is_valid_user_name`, not already taken) and `registry.add_node_network(N)`.

## API + Flutter UI

`structure_designer_api.rs` (sync FFI, regenerate bindings):

```rust
pub fn convert_instance_to_closure(scope_path: Vec<u64>, node_id: u64) -> ConversionResult;
pub fn extract_closure_to_network(scope_path: Vec<u64>, node_id: u64, name: String) -> ConversionResult;
pub fn can_convert_instance_to_closure(scope_path: Vec<u64>, node_id: u64) -> bool; // menu gating
pub fn can_extract_closure_to_network(scope_path: Vec<u64>, node_id: u64) -> bool;  // menu gating
```

`ConversionResult { success: bool, error: Option<String> }` (modeled on `InlineResult`). On
`Ok`, the wrapper calls `refresh_structure_designer_auto`; on `Err`, returns the message for a
snackbar.

`node_widget.dart` context menu (`_handleContextMenu`), beside *Inline* / *Factor*:
- when `node.nodeTypeName == 'closure'` → **"Extract to Network…"** (opens a name dialog, then
  `model.extractClosureToNetwork(node.id, name, scopeChain: …)`).
- when `isCustomNodeType(node.nodeTypeName)` and `can_convert_instance_to_closure` → **"Convert
  to Closure"** (one click → `model.convertInstanceToClosure(node.id, scopeChain: …)`).

Model methods follow the established pattern: call the API with `_scopeChainToBytes(scopeChain)`,
then `refreshFromKernel()`, return the `ConversionResult` for snackbar handling. The name dialog
reuses the factoring dialog's name-field + validation widgetry (name only, no param rows).

---

## Testing

New `rust/tests/structure_designer/closure_network_conversion_test.rs` (registered in
`tests/structure_designer.rs`), reusing the body/closure builders from `closures_test.rs`
(`add_expr_to_body`, `wire_zone_input_to_body_node`, `wire_capture_to_body_node`,
`wire_body_node_to_zone_output`, `add_int_map_closure`) and the network builders from
`node_inlining_test.rs`.

**Network → Closure**

- Basic: instance of a 1-param network, pin unwired → closure of kind Map with one zone-input
  pin; result wire set; consumer flipped `-1 → 0`; assert `evaluate` of a downstream `map.f`
  yields the same stream as the original instance-as-function.
- Mixed pins: 1 wired + 1 unwired → closure with one zone-input param and one **capture wire**
  in the body pointing at the wired source (assert depth 1 at body top); unwired → zone-input.
- Multi-param, all unwired → `Custom` closure preserving param names/order.
- Instance whose wired pin is itself a capture (`d_I = 1`): assert the body capture wire depth
  `= k + 1 + d_I`.
- Inside a zone body (`scope_path` non-empty): the resulting closure's captures resolve against
  the correct enclosing scope; body-undo round-trip.
- Reject: non-custom node; instance with a normal-output consumer; network with no return.

**Closure → Network**

- Basic: `(Int) -> Int` closure `x + 1`, no captures → network with one parameter node, return
  set; instance created; consumer flipped `0 → -1`; `evaluate` matches.
- One capture (`e = 0`): network gains a capture parameter node; `I`'s capture pin wired (depth
  0) to the original source; closure-param pin left unwired.
- **Deep capture from a nested body (`e = 0`, `k = 1`)**: closure body contains a `map` whose
  body references `C`'s host-scope constant. Assert: one capture param node; the body wire (now
  at nesting `k = 1` in `N`) rewired to the param node at depth `k = 1`; `I`'s capture pin wired
  at depth `e = 0`.
- **Capture above host (`e ≥ 1`)**: closure nested inside a `fold` body, capturing a
  grandparent constant (`source_scope_depth = 2` at body top). Assert: `I`'s capture wire has
  `source_scope_depth = 1`; body wire in `N` at depth `k`.
- **`ZoneInput` capture (`e ≥ 1`)**: closure inside a `fold` body capturing the fold's `acc`
  iteration value. Assert `I`'s wire is `ZoneInput { pin }` at depth `e`.
- **Same capture referenced from two nestings** dedups to one parameter node / one `I` pin.
- Reject: non-closure node (`map`/`apply`); closure with no result wire; result from a secondary
  output pin.

**Round-trip property tests (the strongest correctness checks)**

- `closure → network → closure`: build a closure with parameters + a capture, extract to a
  network, then convert the resulting instance back to a closure. Assert the reconstructed
  closure's `function_type`, body wiring shape, capture targets, and zone-output wire match the
  original (normalize ids/param-node names). Cover the nested-body / `e ≥ 1` cases.
- `instance → closure → network → instance`: assert the final instance's `-1` resolved type and
  evaluated function value equal the original's.
- Undo/redo round-trip for both directions, top-level and body scope; network byte-identical
  after undo (`normalize_json`, as in the undo tests).

---

## Implementation phases

**Phase 1 — Network → Closure, top level.** `build_closure_from_instance` + the closure-flavoured
splice; orchestrator `convert_instance_to_closure` for empty `scope_path`;
`ConvertToClosureCommand` (before/after one network). Tests: basic, mixed pins, multi-param,
reject cases. (Reuses `copy_content_into` directly.)

**Phase 2 — Closure → Network, top level.** `extract_network_from_closure` + capture collection
+ the inverse splice; orchestrator `extract_closure_to_network` (registry add, name validation);
reuse `FactorSelectionCommand` for undo. Tests: basic, capture, deep capture (`e = 0, k = 1`),
dedup, rejects, **and the `closure → network → closure` round-trip**.

**Phase 3 — Body scope + `e ≥ 1` captures.** Both orchestrators handle non-empty `scope_path`
(`get_scope_network_mut`, `ZoneBodySnapshot` undo, new `ExtractClosureBodyCommand`). Tests:
`e ≥ 1` `NodeOutput` and `ZoneInput` captures; conversions inside `fold`/`map` bodies; body
undo round-trips.

**Phase 4 — API + Flutter UI.** FFI functions + `can_*` gates; `node_widget.dart` menu items +
name dialog; model methods; `flutter_rust_bridge_codegen generate`. Manual walkthrough: convert
a closure used in a `map.f`, extract it back; convert an instance used as `apply.f`; do both
inside a zone body; undo each.

## Files touched

- **New:** `rust/src/structure_designer/closure_network_conversion.rs`
- **New:** `rust/tests/structure_designer/closure_network_conversion_test.rs` (+ register in
  `rust/tests/structure_designer.rs`)
- **New:** `rust/src/structure_designer/undo/commands/convert_to_closure.rs` and
  `.../extract_closure_body.rs` (+ register in `undo/commands/mod.rs`); `FactorSelectionCommand`
  reused for top-level Closure → Network.
- `rust/src/structure_designer/mod.rs` — declare the module.
- `rust/src/structure_designer/structure_designer.rs` — the two orchestrators + undo wiring.
- `rust/src/structure_designer/nodes/closure.rs` — make `closure_data_for_signature`
  `pub(crate)`.
- `rust/src/api/structure_designer/structure_designer_api.rs` + `…_api_types.rs` — FFI functions
  + `ConversionResult`.
- Regenerate FFI: `flutter_rust_bridge_codegen generate`.
- `lib/structure_designer/node_network/node_widget.dart` — menu items + dispatch + name dialog.
- `lib/structure_designer/structure_designer_model.dart` — `convertInstanceToClosure` /
  `extractClosureToNetwork`.
