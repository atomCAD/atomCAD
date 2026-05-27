# `.cnnd` Migration: v4 → v5 (Zones / Closures / Function Pins)

## Scope

This document specifies the `.cnnd` save-file migration from version **4** (the
last version on `main`) to a new version **5** (the shape the `zones` branch
saves). Landing this migration is the final blocker before merging `zones` back
to `main`: until it exists, projects authored on `main` either won't load on
`zones` or will silently degrade into broken networks.

The migration is part of three landed designs:

- `doc/design_zones.md` — inline HOF bodies, the `zone` field on `Node`, the
  `IncomingWire` storage shape on `Argument`.
- `doc/design_closures.md` — the `closure` and `apply` nodes, the optional
  `f: Function` pin on the four HOFs, the structural-match `Function`-type
  conversion rule (replacing main's trailing-extras partial application).
- `doc/design_function_pins.md` — the revived `-1` function-pin convention,
  re-synthesized as a real `NetworkResult::Function` value with the strict
  "all source inputs must be free" rule.

In scope:

- The version bump from 4 to 5.
- An inventory of what's already handled by `#[serde(default)]` and the existing
  custom `Argument` deserializer — so the migration script does as little as
  possible.
- The one transformation the script must perform: rewriting legacy HOF `f`-wires
  whose source uses trailing-extras capture into the new `closure`-node shape.
- Detection logic, transformation logic, error handling.
- Test fixtures, following the v2→v3 / v3→v4 fixture-directory convention.
- Implementation phases (each ending in `cargo test` green).

Out of scope:

- Migration of `.cnnd` files older than v4 that contain *new-style* zones
  patterns. They don't exist — v2/v3 files predate everything on this branch.
- Reverse migration (v5 → v4). This branch is allowed to break compatibility in
  one direction only.
- UI work. The migration is Rust-side; the Flutter editor already handles every
  shape the migration produces (closure nodes, body wires, capture wires).
- Custom subnetworks with `Function`-typed `parameter` nodes (text-format-only
  on main — see §"Out-of-scope cases" for the rationale).

## Background: why `.cnnd` files break across the branch

A v4 `.cnnd` from `main` has, for every `map` / `filter` / `fold` / `foreach`
node, an `f: Function` argument that is **required** and almost always wired
from another node's `-1` output pin ("function pin"). On main, the type
conversion `Function((P, Q, A, B) -> R) → Function((P, Q) -> R)` was legal
under the **trailing-extras partial application** rule (see
`data_type.rs::can_be_converted_to` on the legacy main branch: "F contains all
parameters of G as its first parameters; F may have additional parameters
after G's parameters"): the source's first `K` inputs are *parameters*
(per-call bindings — must be **unwired**) and the trailing `N_total - K`
inputs are *captures* (pre-evaluated once at HOF eval time — must be
**wired**). The legacy `FunctionEvaluator` pre-evaluated the captures, then
ran the source's body once per element with the per-call values plugged into
the leading parameter pins.

On the `zones` branch HEAD:

- `FunctionEvaluator` is **deleted** (closures Phase 2).
- The `Function` arm of `DataType::can_be_converted_to` was simplified to a
  **structural same-arity** match (closures Phase 2). The trailing-extras rule
  has no remaining consumer.
- The `-1` function pin is re-synthesized (function-pins Phase 1) but with a
  **strict rule**: `build_node_function_closure` requires every input pin of
  the source to be **free** (`can_connect_nodes` line 1217). Every input becomes
  a function parameter; captures are zero.
- HOFs now have an *optional* `f` pin. When unwired, they fall back to their
  own inline `zone` body (`obtain_closure`).

Consequence: a v4 file whose HOF `f`-source has any wired inputs (the
trailing-extras legacy pattern) doesn't satisfy the new function-mode rule.
Without migration, such files load (serde defaults plus the custom `Argument`
deserializer make everything deserialize cleanly), but **validation rejects the
`f`-wire** and the user sees a broken HOF. The migration's one purpose is to
silently rewrite these legacy patterns into the new `closure`-node shape.

## Version bump

In `rust/src/structure_designer/serialization/node_networks_serialization.rs:24`:

```rust
const SERIALIZATION_VERSION: u32 = 5;
```

Extend the dispatch chain in `load_node_networks_from_file` (currently around
line 788):

```rust
if version < 3 { super::migrate_v2_to_v3::migrate_v2_to_v3(&mut root_value)?; }
if version < 4 { super::migrate_v3_to_v4::migrate_v3_to_v4(&mut root_value)?; }
if version < 5 { super::migrate_v4_to_v5::migrate_v4_to_v5(&mut root_value)?; }
if version < SERIALIZATION_VERSION {
    if let Some(obj) = root_value.as_object_mut() {
        obj.insert("version".to_string(), serde_json::Value::from(SERIALIZATION_VERSION));
    }
}
```

Add `pub mod migrate_v4_to_v5;` to `serialization/mod.rs`.

## What's already handled by serde (no migration code required)

The zones-branch serialization layer was built backward-compatibly from the
start. The following differences between v4 and v5 are transparent at load
time:

### Restructured field: `Argument` storage shape

**v4 (main):**
```json
"arguments": [
    { "argument_output_pins": { "<src_node_id>": <src_pin_index>, ... } },
    ...
]
```

**v5 (zones):**
```json
"arguments": [
    { "incoming_wires": [
        { "source_node_id": <src_id>,
          "source_pin": { "NodeOutput": { "pin_index": <pin_idx> } },
          "source_scope_depth": 0 }
    ] },
    ...
]
```

The custom `impl<'de> Deserialize<'de> for Argument` at
`node_network.rs:289-339` reads **either** shape: if `incoming_wires` is
present, use it; otherwise convert the old `argument_output_pins` map into a
`Vec<IncomingWire>` with `SourcePin::NodeOutput { pin_index }` and
`source_scope_depth = 0`. Entries are sorted by `source_node_id` for
determinism.

**Migration implication:** the v4→v5 script can leave any wire it doesn't
actively touch in the old shape. Serde converts on load. The script only needs
to emit the new shape for wires it newly creates (specifically: closure-body
captures and zone-input references, which use `source_scope_depth ≥ 1` or
`SourcePin::ZoneInput` and *cannot* be expressed in the old shape).

### New defaultable fields on `NodeType` (`SerializableNodeType`)

| Field | Default | Behavior on v4 load |
|---|---|---|
| `zone_input_pins: Vec<SerializableOutputPin>` | `[]` | Non-HOF nodes correctly load empty |
| `zone_output_pins: Vec<SerializableParameter>` | `[]` | Same |

Both have `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. No
migration action required.

### New defaultable fields on `Node` (`SerializableNode`)

| Field | Default | Behavior on v4 load |
|---|---|---|
| `zone: Option<SerializableNodeNetwork>` | `None` | Non-HOF gets `None`; HOFs get `None` (no inline body), validated against the `f`-wired rule below |
| `zone_output_arguments: Vec<Argument>` | `[]` | Empty on load |
| `body_width: f64` | `320.0` (via `default_body_width`) | Reasonable HOF body width |
| `body_height: f64` | `180.0` (via `default_body_height`) | Reasonable HOF body height |
| `collapse_mode: CollapseMode` | `Auto` (via `#[derive(Default)]`) | HOFs render as `Collapsed iff f wired, Expanded otherwise` under `Auto`, which is exactly the legacy main-branch visual since every legacy HOF has `f` wired |

All have `#[serde(default, ...)]`. No migration action required.

### Semantic guarantee: HOF body validity after migration

The zones validator (`network_validator.rs::validate_zones_recursive`) enforces
"every zone-output pin must have an incoming wire" — but rule 1 is **suspended
for any HOF whose `f` pin is connected** (closures Phase 5,
`function_input_pin_connected` gate). Since every legacy HOF on main has `f`
wired (it was required), and the migration either *preserves the `f`-wire* (in
the simple no-extras case) or *rewires `f` to point at a new closure node* (in
the trailing-extras case), every migrated HOF ends up with `f` wired. The
empty default inline body is benign — validation skips its missing
zone-output wire.

### Conclusion

**The only code the migration must write is the HOF `f`-wire rewriter for the
trailing-extras case.** Everything else is solved by existing serde
machinery. The migration script is small.

## What the migration must do

For every wire in every network whose destination is a HOF `f` pin and whose
source pin is `-1` (the function pin), bucket the wire by the source node's
input-pin wiring state and act accordingly:

| Source node's input wiring (input count = `N_total`, HOF expected arity = `K`) | Action |
|---|---|
| `N_total == K` and every input pin is unwired | **Keep wire as-is.** The new function-pin synthesizer (`build_node_function_closure`) handles this directly on the zones branch. |
| `N_total > K`, the first `K` input pins are all unwired (per-call parameters), the trailing `N_total - K` are all wired (captures) | **Closure-wrap.** Build a `closure` node, place a clone of the source in its body, wire captures across the boundary, wire parameters from the zone-input pins, rewire HOF.f to the closure. |
| Anything else (unwired pin after wired pin, `N_total < K`, etc.) | **Skip with warning.** Leave the wire untouched. The file may fail to validate on load; the user sees a clear error and fixes interactively. |

The HOF arity table is **frozen at v5**:

```rust
// In migrate_v4_to_v5.rs — hardcoded, not read from the live registry.
// Mirrors `ITERATOR_PINS_V4`'s rationale.
const HOF_F_PINS_V5: &[(&str, /*f_index*/ usize, /*arity*/ usize)] = &[
    ("map",     1, 1),
    ("filter",  1, 1),
    ("fold",    2, 2),
    ("foreach", 1, 1),
];
```

### Detection algorithm

For each `(network_name, network_json)` in `root["node_networks"]`:

1. For each node `H` in `network.nodes`:
   - If `H.node_type_name` is in `HOF_F_PINS_V5`, let `(f_index, K) = lookup`.
   - Read `H.arguments[f_index]` (the `f` pin's wire storage).
   - Read the single inbound wire `(src_id, src_pin_idx)`. Migration only acts
     if `src_pin_idx == -1`. Otherwise the wire was already a closure-shaped
     value source (impossible on v4 main but allowed defensively) — leave it.
   - Look up the source node `S` by `src_id` in this same network. (HOF `f`
     wires never cross scopes on v4 — there are no scopes on v4.)
   - Compute `N_total = S.arguments.length` and partition into prefix-unwired
     (parameter) and suffix-wired (capture) sets by walking
     `S.arguments[0..N_total]`. An argument is "wired" iff its
     `argument_output_pins` map (v4 shape) is non-empty. (The dual-shape
     helper `argument_is_wired` also accepts the v5 `incoming_wires` shape
     for robustness, but no v4-stamped input file will carry that shape —
     this pass is the first to write it.)
   - Apply the bucketing table above.

2. Record migration actions for the network in a `Vec<DetectedAction>` —
   *don't mutate yet*. Two-pass over each network so id allocation is
   deterministic (see §"Determinism").

3. After detection finishes **for this network** (steps 2–4 are per-
   network — each network has its own `next_node_id` counter), allocate
   fresh closure node ids in sorted order (sort actions by `(hof_id,
   f_index, src_id)`) starting from `network.next_node_id`. Bump
   `next_node_id` accordingly.

4. Execute each `ClosureWrap` action (rewire HOF.f, insert closure node,
   set up body); emit warnings for `Skip` actions; leave `NoOp` actions
   untouched. After all actions in the network finish, run
   `cleanup_orphan_sources` once per §"Source-node cleanup".

### Transformation algorithm (one action)

Inputs: network JSON value, HOF node `H` at index `f_index`, source node `S`
identified by `src_id`, suffix-wired capture count `C = N_total - K`,
HOF kind ∈ {map, filter, fold, foreach}.

#### Step 1 — Compute the closure's ClosureData

Read the HOF's `data` blob (a `serde_json::Value`) and look up its
type-key fields. The HOF kind → closure kind mapping is direct (confirmed
lossless against `closure.rs::ClosureKind`); the pseudocode below uses
struct-field notation for readability, but the implementation reads JSON
keys (`hof_data["input_type"]`, etc.) and inserts the values verbatim into
the closure's `data` field:

```rust
match hof_type {
    "map"     => ClosureData {
        kind: ClosureKind::Map,
        type_args: vec![hof.data.input_type, hof.data.output_type],
        param_names: vec![],
        custom_label: None,
    },
    "filter"  => ClosureData {
        kind: ClosureKind::Filter,
        type_args: vec![hof.data.element_type],
        param_names: vec![],
        custom_label: None,
    },
    "fold"    => ClosureData {
        kind: ClosureKind::Fold,
        type_args: vec![hof.data.accumulator_type, hof.data.element_type],
        param_names: vec![],
        custom_label: None,
    },
    "foreach" => ClosureData {
        kind: ClosureKind::Foreach,
        type_args: vec![hof.data.input_type],
        param_names: vec![],
        custom_label: None,
    },
}
```

`DataType` values are not parsed — they are copied as `serde_json::Value`
into the closure's `data.type_args` array unchanged. Defensive parsing of
missing keys is covered in Phase 3 Gotchas (fall back to `"None"` and warn).

#### Step 2 — Place the new closure node

The closure node's id was **pre-allocated** during Detection step 3 (in
sorted order, starting from `network.next_node_id`) and is handed to
`execute_action` as a parameter. This step just computes its on-canvas
position:

```rust
let position = [H.position[0] - 160.0, H.position[1]]; // to the HOF's left
```

The `160` offset mirrors v3→v4's `+130` convention for synthesized adapter
nodes; the negative sign keeps the closure visually upstream of the HOF.

#### Step 3 — Build the closure node JSON

```json
{
  "id": <new_closure_id>,
  "node_type_name": "closure",
  "position": [<position[0]>, <position[1]>],
  "arguments": [],
  "data_type": "closure",
  "data": {
    "kind": "Map",
    "type_args": [<type_args as embedded JSON>],
    "param_names": [],
    "custom_label": null
  },
  "body_width": 320.0,
  "body_height": 180.0,
  "collapse_mode": "Auto",
  "zone": <body NodeNetwork — see Step 4>,
  "zone_output_arguments": [
    { "incoming_wires": [
        { "source_node_id": 1,
          "source_pin": { "NodeOutput": { "pin_index": 0 } },
          "source_scope_depth": 0 }
    ] }
  ]
}
```

Notes on the field shapes:

- `data_type` is a `String` tag duplicating `node_type_name` for built-in
  node types (see `node_to_serializable` in
  `node_networks_serialization.rs` — line ~401: `(node_type_name.clone(),
  json_data)`). It is **not** a serialized `DataType`. For the closure
  node it is just the literal `"closure"`.
- `data.kind` is a serde-default enum variant name — `"Map"`, `"Filter"`,
  `"Fold"`, or `"Foreach"` (capitalized, no `Custom` arm from this
  migration). The unqualified `"Map"` above is a placeholder; substitute
  the variant matching the HOF kind being rewritten.
- `custom_name` is `#[serde(default, skip_serializing_if =
  "Option::is_none")]`, so omitting it for `None` is *required* (writing
  `"custom_name": null` would serialize fine but diverges from what the
  rest of the codebase emits and pollutes round-trip diffs).

The `zone_output_arguments` has one entry (the one zone-output pin every
preset closure kind defines), wired from body-local node id `1` (the cloned
source) pin 0.

#### Step 4 — Build the body NodeNetwork

The body contains exactly one node: a clone of `S` with body-local id `1`.
The body serializes as a `SerializableNodeNetwork`
(`node_networks_serialization.rs:184-196`) — its exact field set is
`next_node_id`, `node_type`, `nodes`, `return_node_id`,
`displayed_node_ids`, optional `displayed_output_pins`, optional
`camera_settings`. **There is no `name`, `selected_node_ids`,
`selected_wires`, `valid`, or `validation_errors` field on the serialized
shape** — those are runtime-only state on the in-memory `NodeNetwork`.

```json
{
  "next_node_id": 2,
  "node_type": {
    "name": "",
    "description": "",
    "summary": null,
    "category": "OtherBuiltin",
    "parameters": [],
    "output_pins": [
      { "name": "result", "data_type": "None" }
    ]
  },
  "nodes": [
    {
      "id": 1,
      "node_type_name": "<S.node_type_name>",
      "position": [40.0, 40.0],
      "arguments": [
        // For each input pin i in 0..N_total of the source:
        //   if i < K (parameter): incoming_wires = [ { source_node_id: <new_closure_id>,
        //                                              source_pin: { "ZoneInput": { "pin_index": i } },
        //                                              source_scope_depth: 1 } ]
        //   if i >= K (capture):  incoming_wires = [ { source_node_id: <orig parent src>,
        //                                              source_pin: { "NodeOutput": { "pin_index": <orig pin> } },
        //                                              source_scope_depth: 1 } ]
      ],
      "data_type": "<S.data_type>",
      "data": <verbatim copy of S.data>
    }
  ],
  "return_node_id": null,
  "displayed_node_ids": []
}
```

Notes:

- **`nodes` is a JSON `Vec`, not a map.** Each entry carries its own
  numeric `id`. `SerializableNode` also has
  `#[serde(skip_serializing_if = "Option::is_none")]` on `custom_name` and
  `#[serde(skip_serializing_if = "Vec::is_empty")]` on
  `zone_output_arguments` — omit those for the body clone (the cloned
  source is not itself a HOF, so it has no zone). Likewise omit `zone`
  (None) and let `body_width` / `body_height` / `collapse_mode` default
  via `#[serde(default)]` rather than emitting them explicitly. If `S`
  carried a `custom_name` on the original node, propagate it verbatim.
- **`node_type` is a structured `SerializableNodeType`, not a string
  placeholder.** Body networks created at runtime via
  `NodeNetwork::new_empty()` (see `node_network.rs:957`) start with a
  placeholder `NodeType` whose serialized form is exactly the object
  shown above: empty `name`/`description`, `summary: null`, category
  `"OtherBuiltin"`, no parameters, one output pin named `"result"` of
  type `"None"` (per `OutputPinDefinition::single(DataType::None)`).
  Emit this same object verbatim for every closure body — it matches
  what `ensure_zone_init` produces for live bodies and round-trips
  cleanly. The `zone_input_pins` / `zone_output_pins` / `output_type`
  fields on `SerializableNodeType` carry `skip_serializing_if`, so they
  are omitted from the body's `node_type` rather than written as empty.
- **`displayed_node_ids` is a `Vec<(u64, NodeDisplayType)>` (a JSON
  array of `[id, "DisplayType"]` two-tuples), not a map.** The body
  clone gets no display (the `result` pin's display is implicit via the
  HOF/closure machinery, not via per-node display state in the body), so
  emit an empty array. `displayed_output_pins` is `skip_serializing_if =
  "Vec::is_empty"` — omit it entirely.
- **`source_scope_depth = 1`** for both wire kinds. From the body's
  perspective the network stack reads `[..., (parent_network, ...), (body, ...)]`
  with the body on top, so "1 frame up" is the parent network. Captures
  reach a node *in* the parent network; ZoneInput references reach the
  body's owning HOF, which also *lives in* the parent network. Same depth
  by construction.
- **`source_pin` is a serde-tagged enum** — write it as
  `{ "NodeOutput": { "pin_index": N } }` or
  `{ "ZoneInput": { "pin_index": N } }` (externally-tagged form). The
  custom `Argument` deserializer at `node_network.rs:289-339` round-trips
  this exact shape.
- **Capture wires preserve `(source_node_id, pin_index)` from the original
  parent wire.** If `S.arguments[i]` was wired `{42: 0}` (parent node 42's
  pin 0) on main, the body clone's input `i` carries
  `IncomingWire { source_node_id: 42, source_pin: NodeOutput { pin_index: 0 },
  source_scope_depth: 1 }`. The original wire in the parent network is left
  alone — `S` itself stays in the parent (see Step 5) and continues to
  consume those inputs for any non-`-1` use of its outputs.
- **ZoneInput wires reference the new closure node by id.** The
  `source_node_id` of a `ZoneInput` wire is the HOF that owns the body
  whose zone-input pin is being read — here, that's the new closure node.
- **Source node `S` lifecycle — see "Source-node cleanup" under Step 5.**

#### Step 5 — Rewire the HOF's `f` pin

In the parent network, modify `H.arguments[f_index]` to point at the new
closure node instead of the original source's `-1` pin. Write in the **new
wire shape** (since the original wire used `pin_index = -1` which the new
synthesizer rejects anyway):

```json
"arguments": [
    // ... unchanged entries for 0..f_index ...
    { "incoming_wires": [
        { "source_node_id": <new_closure_id>,
          "source_pin": { "NodeOutput": { "pin_index": 0 } },
          "source_scope_depth": 0 }
    ] },
    // ... unchanged entries for f_index+1.. ...
]
```

If the original entry was in the v4 `argument_output_pins` shape, replace it
with the v5 `incoming_wires` shape. Other arguments on `H` can stay in their
original shape — serde converts at load.

##### Source-node cleanup

After every rewired HOF.f in the network has been processed, walk
`network.nodes` once and delete any source node `S` that meets all of:

1. `S` was the source of at least one HOF.f wire rewritten in this pass
   (track this during execution — keep a `HashSet<u64>` of touched source
   ids).
2. `S`'s only output consumers in the parent network are HOF.f wires that
   the pass *already rewired* (i.e. after rewriting, no remaining wire in
   `network.nodes` references `(S.id, *)` as a source). The check is a
   simple post-pass scan: iterate every `Argument` in every node's
   `arguments` and `zone_output_arguments`, count references to `S.id`,
   delete `S` if the count is zero.

When `S` is deleted, also drop any entries in `network.displayed_node_ids`
and `network.displayed_output_pins` keyed by `S.id`.

Rationale — without this, the common fanout pattern (one `expr.-1` feeding
two HOFs) leaves `S` plus its capture wires permanently visible on the
canvas as an apparently-disconnected leftover, on every migrated file. The
predicate is conservative: `S` stays if it has any non-`-1` consumer (its
regular output pin is wired into another node), and stays if any of its
`-1` consumers was *not* in the rewritten set (e.g. a `-1` wire that
landed in the "anything else → skip with warning" bucket — keep `S` so
the user can inspect and fix). Per-consumer fanout (§"Per-consumer
wrapping") still synthesizes one closure per HOF; the per-HOF closures
own their own body clones of `S`, and the parent-network `S` is what
this cleanup removes.

#### Step 6 — Insert the new closure node into the parent's `nodes`

`SerializableNodeNetwork.nodes` is `Vec<SerializableNode>` — a JSON array,
**not** a map keyed by id. Each node carries its own `id` field. Append
the closure node JSON (built in Step 3+4) to the end of `network.nodes`.
Bump `network.next_node_id` so future add-node operations don't collide;
`next_node_id` is always serialized.

### Per-consumer wrapping (fanout)

If one source node's `-1` pin is wired into multiple HOFs' `f` pins,
**synthesize one closure per consumer** — do not share. This mirrors v3→v4's
collect-per-consumer policy. Each closure gets a fresh id and a fresh body
clone of the source. The captures all reference the same parent-network
upstream sources by id, but live independently in each closure body. This is
correct: each HOF freezes captures at its own `eval`, so per-consumer closures
keep that semantics intact.

### Determinism

- IDs are allocated by sorting pending actions by
  `(hof_node_id, f_index, src_node_id)` before assignment.
- The body clone's input-pin order matches the source's original
  `arguments` order.
- `incoming_wires` vec entries are produced in pin-index order.

This produces byte-identical output across runs and makes
`cnnd_roundtrip`-style fixtures stable.

### Idempotence

The migration is idempotent on already-migrated values: on a v5 file (or a
post-migration value), `H.arguments[f_index]`'s source is the closure node's
pin 0 (`pin_index == 0`, not `-1`), so the detection condition
"`src_pin_idx == -1`" fails and no action fires. Direct re-invocation on the
post-migration JSON produces byte-identical output. The version dispatch
guard (`if version < 5`) skips the pass entirely for already-v5 files.

### What gets walked

Two distinct "walks" are at play; only one is needed:

1. **Across networks** — the migration iterates *every* entry in
   `root["node_networks"]` (top-level network plus every named custom
   subnetwork). This is required: a legacy HOF.f-with-extras pattern can
   sit in any of them. Covered by the outer loop in §"Detection algorithm".
2. **Into `Node.zone` bodies** — *not* required. v4 files predate zones
   entirely, so no node carries a `zone` body. v3 files chained through
   v3→v4→v5 don't gain zones either (the earlier passes never add them).
   The migration touches no body networks.

If a future v5+ ever needs body-recursion, the helper to add is
`walk_all_nodes`-style descent into `Node.zone.nodes` — see
`structure_designer/AGENTS.md` "Walking a network's nodes" for the live
pattern.

### Error handling

For each detection that doesn't fit the table (unwired-after-wired,
`N_total < K`, source node missing entirely):

- Emit a warning via `eprintln!("v4→v5: skipping HOF f-wire on network={}, hof_id={}, reason={}", ...)`
  (matches v2→v3's logging convention).
- Leave the wire untouched in the JSON.
- The file then loads but the wire fails validation on the zones branch (the
  function-pin synthesizer rejects sources with wired inputs at eval time, or
  the type-compat check rejects arity mismatches at validation). The user sees
  a clear error in the validation pane and fixes interactively.

No `MigrationError` propagation for these cases — the migration succeeds; the
file is just partially-broken on load.

`MigrationError` is reserved for *structural* problems (root not a JSON
object, `node_networks` not an array, etc.) — same as v2→v3 / v3→v4
convention.

### Out-of-scope cases (not detected at all)

The detection loop only considers HOFs (the four node types in
`HOF_F_PINS_V5`). Wires whose destination is some *other* `Function`-typed
pin are never inspected and pass through the migration unchanged:

- **Function-typed parameters on custom subnetworks.** On main these were
  authorable only via the text-format editor; the GUI's `APIDataTypeBase`
  enum has no `Function` variant, and `promote_to_parameter.rs` explicitly
  rejects `DataType::Function`. Files containing such patterns are
  vanishingly rare.
- **Non-HOF Function-typed input pins** generally. Same reasoning — on
  main, the only non-text-format `Function` pins are HOFs' `f`.

If such a wire happens to exist and uses the legacy trailing-extras
pattern, it will fail validation on load (same way malformed HOF.f wires
do — the user fixes interactively). The migration does not warn about
these because it can't cheaply distinguish them from any other
`Function`-typed wire without re-implementing pin-type resolution against
the registry, which the migration deliberately avoids.

## Test fixtures

Following the v3→v4 convention, fixtures live in
`rust/tests/fixtures/zones_migration/`. Hand-authored v4 `.cnnd` files (small,
focused on one pattern each):

| Fixture | Pattern | Expected post-migration |
|---|---|---|
| `simple_map_with_capture.cnnd` | `map.f` wired from `expr` with 1 free + 2 wired inputs | Closure of kind `Map` containing the `expr`, two capture wires crossing boundary, one zone-input wire |
| `simple_filter_with_capture.cnnd` | `filter.f` wired from `expr` with 1 free + 1 wired input | Closure of kind `Filter` |
| `simple_fold_with_capture.cnnd` | `fold.f` wired from `expr` with 2 free + 1 wired input | Closure of kind `Fold` |
| `simple_foreach_with_capture.cnnd` | `foreach.f` wired from `expr` with 1 free + 1 wired input | Closure of kind `Foreach` |
| `no_extras_preserved.cnnd` | `map.f` from `expr` with 1 free input only, no captures | Wire preserved unchanged (function-pin synthesizer handles it) |
| `fanout_creates_two_closures.cnnd` | One `expr.-1` wired into two `map.f` pins | Two closures synthesized, each with its own body clone |
| `custom_subnetwork_instance_source.cnnd` | HOF `f` wired from a custom subnetwork instance with trailing-extras wired | Closure wraps the instance node uniformly |
| `nested_custom_network.cnnd` | An HOF lives inside a custom subnetwork's definition (not at top level) | Migration descends into the custom network's nodes and migrates the inner HOF |
| `hof_source_for_hof_f.cnnd` | `map.f` wired from another `map`'s `-1` pin (the source is itself a HOF) | Closure wraps the source `map`; the source's own `f` argument is preserved verbatim in the body clone (with `source_scope_depth: 1` reaching the parent network's original f-source). Asserts load + validation outcome — see Open Questions §1 |
| `source_cleanup_fanout.cnnd` | One `expr.-1` wired into two `map.f` pins; `expr` has no non-`-1` consumers | Two closures synthesized, **and** the parent-network `expr` deleted by source-node cleanup (§"Step 5"). Asserts the parent network no longer contains the source id after migration |
| `source_cleanup_preserved.cnnd` | `expr.-1` wired into one `map.f` *and* `expr.0` (regular output) wired into a `print` consumer | Closure synthesized; `expr` preserved in the parent network because of the non-`-1` consumer |
| `bad_wired_after_unwired.cnnd` | `map.f` from `expr` with [wired, unwired, wired] inputs (no prefix) | Wire untouched (post-migration JSON shape identical to input). File loads; validation flags the HOF |
| `bad_too_few_inputs.cnnd` | `map.f` from a 0-input node | Wire untouched. File loads; validation flags the HOF |
| `already_v5.cnnd` | A v5-shaped file (closures in place, no `-1` `f`-wires) | Migration pass skipped entirely by the `if version < 5` dispatch guard — `migration_call_count() == 0` and output is byte-identical to input. (Idempotence of the pass *itself* — running it on already-v5 JSON — is covered separately by re-invoking it directly in a unit test.) |
| `v3_chained_through.cnnd` | A v3 file that — through v3→v4 — produces an HOF.f-with-extras pattern | All three passes run; final result correct |

Plus a regression fixture for the existing v3→v4 fixtures: every
`rust/tests/fixtures/iterator_migration/` fixture must continue to load
correctly under the new dispatch chain (v3→v4→v5). The existing
`iterator_migration_test.rs` tests cover this if the dispatch is wired
correctly — confirm by running `cargo test --test integration` after the
dispatch change lands.

## Implementation phases

Each phase is self-contained and ends with `cd rust && cargo test` green plus
`cargo clippy` clean. Phases are strictly sequential.

### Phase 1: Scaffolding — version bump + no-op migration

**Goal.** Land the v4→v5 migration as an inert pre-pass: version constant
bumped, dispatch wired, module exists with a stub implementation that touches
no JSON. Existing tests stay green.

**Scope.**

- `rust/src/structure_designer/serialization/migrate_v4_to_v5.rs` — new file:
  - `pub fn migrate_v4_to_v5(root: &mut Value) -> Result<(), MigrationError>` —
    walks `root["node_networks"]` and does nothing per network. Returns `Ok(())`.
  - `thread_local! { static MIGRATION_CALL_COUNT: Cell<u64> = ... }` plus
    `migration_call_count()` / `reset_migration_call_count()` helpers,
    matching the v2→v3 / v3→v4 instrumentation pattern.
  - `const HOF_F_PINS_V5: &[(&str, usize, usize)]` — the frozen HOF table.
  - Use `super::migrate_v2_to_v3::MigrationError` as the error type (same as
    v3→v4 — no new error enum).
- `rust/src/structure_designer/serialization/mod.rs` — add `pub mod migrate_v4_to_v5;`.
- `node_networks_serialization.rs`:
  - Bump `SERIALIZATION_VERSION` from `4` to `5`.
  - Add the dispatch arm `if version < 5 { super::migrate_v4_to_v5::migrate_v4_to_v5(&mut root_value)?; }`.

**Tests.**

- New `rust/tests/structure_designer/zones_migration_test.rs` registered in
  `tests/structure_designer.rs`:
  - `test_v5_file_skips_migration` — load a hand-authored v5-stamped file
    (`already_v5.cnnd`), assert `migration_call_count() == 0`.
  - `test_v4_file_triggers_migration` — load a hand-authored v4 file with no
    HOFs, assert `migration_call_count() == 1` and the network round-trips
    unchanged.
- Regression: every existing test in `cnnd_roundtrip_test.rs` and
  `iterator_migration_test.rs` continues to pass. Insta snapshot tests touched
  by the version bump (the version field shows up in JSON output) need
  `cargo insta review` to accept.

**Verification.** `cd rust && cargo test` green; `cargo insta accept` after
review.

**Gotchas.**

- The version field stamp at line 800ish moves up to 5. Snapshot fixtures
  contain `"version": 4` and will diff. Don't manually edit fixture JSON;
  re-run with `INSTA_UPDATE=auto` or use `cargo insta review`.
- The no-op skeleton can be a trivial loop; Phase 2 will refactor it to
  the two-pass (detect-then-mutate) shape v3→v4 uses. Don't over-engineer
  Phase 1.

### Phase 2: Detect + skip-with-warning

**Goal.** Implement the detection algorithm but only the skip-with-warning
branch. No closure synthesis yet. Files with HOF.f wires emit warnings; files
without are untouched.

**Scope.**

- In `migrate_v4_to_v5.rs`:
  - `fn detect_hof_f_actions(network: &Value) -> Vec<DetectedAction>` —
    walks `network.nodes`, finds HOFs by `node_type_name` in `HOF_F_PINS_V5`,
    inspects `arguments[f_index]` for `-1`-source wires, resolves the source
    by id, partitions its `arguments` into wired/unwired prefix, and produces
    `DetectedAction { kind: ClosureWrap | NoOp | Skip { reason: String }, ... }`.
  - For now, every `ClosureWrap` action is treated identically to `Skip` —
    log a TODO warning saying Phase 3 will synthesize.
  - Helper `fn read_argument_source(arg: &Value) -> Option<(u64, i32)>` —
    handles both v4 (`argument_output_pins`) and v5 (`incoming_wires`) shapes.
  - Helper `fn argument_is_wired(arg: &Value) -> bool` — same dual-shape
    handling.

**Tests.**

- Add `bad_wired_after_unwired.cnnd` and `bad_too_few_inputs.cnnd` fixtures.
  Test that loading them produces no panics, the file loads, and (optionally)
  the validation errors mention the unwired pin. These tests remain valid
  unchanged in Phase 3 — the skip-with-warning branch is permanent.
- Add `simple_map_with_capture.cnnd` fixture. **Phase-2-only assertion:**
  loading produces a warning (the TODO log from the temporarily-disabled
  `ClosureWrap` branch) but doesn't crash. Phase 3 replaces this assertion
  with the full migration check; do not write a snapshot test against the
  Phase 2 output of this fixture, because Phase 3 will mutate it.

**Verification.** `cd rust && cargo test` green.

**Gotchas.**

- `S.arguments` length is the source's input pin count *at save time on
  main*. If main's node-type-registry shape for that node has since changed
  (e.g. a built-in node gained an input pin between main and zones), the
  count is the older shape — exactly what we want. The migration treats the
  saved arity as authoritative.
- Watch out for missing `arguments` field (older variants sometimes elide
  empty arrays). Treat missing-or-empty as "no inputs", which means no
  function-mode wire could be valid anyway → skip.

### Phase 3: Closure synthesis (the main course)

**Goal.** Implement the closure-wrapping transformation for the
trailing-extras case. After this phase, fixtures with legacy capture
patterns load as fully-valid zones-branch networks.

**Scope.**

- In `migrate_v4_to_v5.rs`:
  - `fn execute_action(network: &mut Value, action: &DetectedAction, new_closure_id: u64) -> Result<(), MigrationError>` —
    builds the closure node JSON, rewrites the HOF's `f` argument, inserts
    the new node, updates `next_node_id` if present. Only the
    `ClosureWrap` action kind reaches this function; `NoOp` (keep wire
    as-is) and `Skip` (emit warning) are handled earlier in
    `migrate_v4_to_v5` and don't allocate a closure id.
  - `fn build_closure_node(...) -> Value` — produces the full closure node
    JSON per Step 3 of §"Transformation algorithm".
  - `fn build_closure_data(hof_type: &str, hof_data: &Value) -> Result<Value, MigrationError>` —
    extracts type fields from the HOF's `data` blob and constructs the
    `ClosureData` JSON.
  - `fn build_body_network(source_node: &Value, capture_count: usize, closure_id: u64) -> Value` —
    builds the body NodeNetwork with the cloned source and its rewired
    arguments per Step 4.
  - `fn build_body_arguments(source_args: &[Value], capture_count: usize, closure_id: u64) -> Vec<Value>` —
    pin-by-pin construction of the body clone's `incoming_wires`.
  - `fn next_node_id_for_network(network: &Value) -> u64` — reads
    `network.next_node_id` if present, else computes `max(node ids) + 1`.
  - `fn cleanup_orphan_sources(network: &mut Value, rewritten_sources: &HashSet<u64>)` —
    implements §"Source-node cleanup" under Step 5. For each id in
    `rewritten_sources`, scans every remaining `Argument` in the network
    (both `arguments` and `zone_output_arguments` on every node) for any
    reference to that id; if none remain, deletes the node from
    `network.nodes` and drops matching entries from `displayed_node_ids` /
    `displayed_output_pins`. Called once after all `execute_action` calls
    for the network finish.
  - In `migrate_v4_to_v5(root)`: outer loop over each
    `(network_name, network_json)` in `root["node_networks"]`. Inside the
    loop: detect actions for the network, sort them deterministically,
    allocate ids from `network.next_node_id`, execute, then run
    `cleanup_orphan_sources` — all per-network so id spaces stay
    independent.

**Tests.** Use all `simple_*_with_capture.cnnd` fixtures (one per HOF kind):
load, assert:
1. The HOF's `f` argument now references the new closure node's pin 0.
2. A new `closure` node exists in the network with the right `kind` and
   `type_args`.
3. The closure's body contains exactly one node (the source clone) at body-
   local id 1.
4. The body clone's arguments correctly partition into captures (depth=1,
   `NodeOutput`) and parameters (depth=1, `ZoneInput`).
5. The closure's `zone_output_arguments` has one wire from body node 1 pin 0.
6. The network *validates* on load (no validation errors on the HOF or
   closure).
7. Evaluation produces the same result as a hand-built equivalent zones-branch
   network.

`fanout_creates_two_closures.cnnd` asserts two distinct closure nodes
with independent body clones of the source (each clone's capture wires
reference the same parent-network upstream nodes by id, but the wires
themselves live on the body clones, not shared). Each HOF.f points at its
own closure's pin 0.

`source_cleanup_fanout.cnnd` asserts that when the source has no non-`-1`
consumers, the post-pass `cleanup_orphan_sources` (§"Step 5") deletes the
source from the parent network — `network.nodes` no longer contains the
source id, and `displayed_node_ids` / `displayed_output_pins` have no
stale references to it.

`source_cleanup_preserved.cnnd` asserts the inverse: when the source has
any non-`-1` consumer, it is preserved verbatim (only the HOF.f wire is
rewired; the source node, its capture wires, and the non-`-1` consumer
wire all survive).

`hof_source_for_hof_f.cnnd` asserts that nested-HOF sources clone
verbatim: the cloned inner HOF's own `f` argument is preserved with
`source_scope_depth: 1` reaching back into the parent network. The
fixture's expected outcome (loads + validates, or loads-but-validation-
flags-it) drives Open Question §1's decision.

`custom_subnetwork_instance_source.cnnd` asserts the cloned body node carries
the same `node_type_name` and `data` (which references the custom network by
name) as the original.

`nested_custom_network.cnnd` asserts the migration descended into the custom
network definition (not just the top-level network) and migrated an inner
HOF.

`v3_chained_through.cnnd` asserts the full v3→v4→v5 chain produces a
fully-valid network.

**Verification.** `cd rust && cargo test` green; `cargo clippy` clean;
manual `flutter run` smoke check: open a fixture, see the closure node
rendered with its body, run evaluation, get expected results.

**Gotchas.**

- The HOF `data` blob has different shapes per HOF type (`MapData` vs
  `FoldData` etc.). The migration reads serialized JSON keys
  (`"input_type"`, `"output_type"`, `"accumulator_type"`, `"element_type"`)
  with defensive parsing — if a key is missing, fall back to a sensible
  default (`DataType::None` serialized) and log a warning. Mirrors
  `migrate_v3_to_v4`'s defensive parsing of `MapData.output_type` /
  `FilterData.element_type` (project memory: Phase 7 Iterators).
- Body's `node_type` is a structured `SerializableNodeType`, not a string
  — emit the exact object documented in §"Step 4" (matches what
  `NodeNetwork::new_empty()` → `node_type_to_serializable` produces for
  live bodies). The runtime in-memory body is built by `NodeNetwork::new`
  from this placeholder type; mismatches show up as `repair_node_network`
  warnings at load.
- Runtime `displayed_nodes` is a HashMap (non-deterministic iteration
  order), but its **serialized** form `displayed_node_ids: Vec<(u64,
  NodeDisplayType)>` is a `Vec` written from the HashMap at save time.
  Our synthesized bodies write `[]`, so iteration order is moot here —
  but tests comparing roundtripped output to the synthesized JSON should
  still go through `normalize_json` (per project memory) because non-body
  networks in the same file will exercise HashMap ordering.
- The original source node on the parent network may itself be polymorphic
  (its `data` carries fields that resolve at registry time). The clone
  carries the same `data` verbatim — no semantic change. Phase 4 of zones
  introduced `repair_node_network` which runs on load and may further
  refine; that's expected and orthogonal.

### Phase 4: Hardening + comprehensive fixture coverage

**Goal.** Round out edge cases, real-world fixtures, and confirm zero
regressions across the broader test suite.

**Scope.**

- Add `rust/tests/fixtures/zones_migration/real_*.cnnd` files: at least one
  real-world project file from main (anonymized if necessary) per major HOF
  kind. These are the highest-confidence regression check.
- Add a test that loads every fixture under `iterator_migration/` through the
  v3→v4→v5 chain and confirms the v5 stamp is present, the file validates,
  and (where applicable) `cargo insta` snapshots are accepted.
- Confirm interaction with the `repair_node_network` pass that runs at load
  time: a migrated closure's body clone might have type mismatches if the
  HOF's `input_type` doesn't match the source's expected types (rare on a
  validly-saved main file but possible after the user edited types
  out-of-sync). Validate that repair *disconnects* incompatible wires
  cleanly rather than panicking.
- Document the migration in `serialization/AGENTS.md` and
  `serialization/CLAUDE.md` (a short section under "Version Migrations
  (chained dispatch)" listing the new pass).

**Tests.** Real-world fixtures load, validate, and evaluate.

**Verification.** `cd rust && cargo test` green; `cargo clippy` clean;
`flutter run` smoke check on a real-world fixture (open a real project from
main, confirm it renders correctly, evaluate one HOF chain).

**Gotchas.**

- Real-world fixtures may contain other v3-only patterns the v3→v4 migration
  already handles. The full chain v3→v4→v5 must pass.
- If a real fixture exercises a node type that no longer exists on the zones
  branch (unlikely but possible), the load will fail at registry lookup —
  unrelated to this migration. Note in the test which fixtures depend on
  which node types being present.

## Open questions

1. **HOF.f wires where the source is itself an HOF.** On main, an HOF's
   `-1` output pin technically existed and could be wired into another
   HOF's `f`. The function-pin synthesizer treats every input (including
   `xs`, `init`, `f`) as a parameter — the resulting function type was
   weird and likely never used in practice. The migration treats such
   sources uniformly (clone into closure body, wire as normal). Note that
   the cloned source — itself a HOF — still has its own `f` argument
   pointing at whatever the original f-source was; that wire becomes a
   capture (`source_scope_depth: 1`) reaching back into the parent
   network. Whether the resulting nested-HOF construct type-checks
   under the structural-arity rule is what the
   `hof_source_for_hof_f.cnnd` fixture (§"Test fixtures") pins down.
   Decision: if the fixture loads and validates, no code change; if it
   doesn't, document the limitation in the user-visible release notes
   rather than special-casing the migration — the pattern is rare
   enough that an interactive fix is acceptable.

## Implementation-time confirmations

The body `node_type` shape, `nodes` array shape, and serde field-skip
conventions are all pinned down in §"Step 4" / §"Step 6" against
`SerializableNodeNetwork` (`node_networks_serialization.rs:184-196`) and
`SerializableNode` (line 135 of the same file). The implementer should
diff the synthesized JSON against a roundtripped fresh-empty body before
the first integration test — any field-name or shape drift surfaces as a
serde error or a `repair_node_network` warning at load.

One thing worth re-checking against current code during Phase 3, since it
is the load-side dependency the migration is leaning on:

- **`repair_node_network` runs on every load.** It already does
  (post-deserialization, as part of the normal load sequence). The
  migration does not need to invoke it explicitly. Confirm by tracing
  the call chain in `load_node_networks_from_file`. (If the call site
  has moved, the migration's tolerance for slightly-wrong-but-loadable
  output collapses — the script becomes responsible for emitting
  byte-exact validated JSON.)

## Reuse map (summary)

**Reused unchanged:**
- The chained-dispatch pattern in `load_node_networks_from_file`.
- The `MigrationError` type from `migrate_v2_to_v3` (shared with v3→v4).
- The `thread_local!` migration-call counter convention.
- The `compute_*` pre-pass / mutation-pass borrow discipline.
- The hand-authored-fixture testing pattern (each fixture isolates one
  pattern; tests assert specific post-migration properties).
- The `normalize_json` test helper for HashMap-ordering-insensitive
  comparisons.

**New from scratch:**
- `migrate_v4_to_v5.rs` — the one new file.
- `HOF_F_PINS_V5` frozen table.
- `DetectedAction` and the detection + execution helpers.
- `zones_migration/` fixture directory.
- `zones_migration_test.rs` test module.

**Net effect:** one small file (~400 lines including extensive comments and
defensive parsing), one fixture directory, one test module. The serialization
machinery did most of the work via `#[serde(default)]` and the custom
`Argument` deserializer; the script only needs to handle the one transformation
that can't be expressed declaratively.
