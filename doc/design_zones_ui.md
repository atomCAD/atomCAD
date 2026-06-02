# Zones UI: Inline Editing of HOF Bodies in the Flutter Editor

## Scope

This document designs the **Flutter editor side** of the zones feature. The Rust side is covered by `doc/design_zones.md` (in flight, phase 6 of 6 at time of writing). Together they form the full picture.

Concretely, this branch extends the existing flat-DAG node editor so that the body of a higher-order function node (`map`, `filter`, `fold`, `foreach`) renders **inside** the HOF as an inline, bordered region containing its own nodes and wires. Iteration values come from inside-facing pins on the HOF's left edge; the body's return value flows into inside-facing pins on the right edge; captures are wires that visually cross the body's boundary.

In scope:
- Conceptual UX of inline zones
- Coordinate system, hit testing, pin position resolution under scope chains
- Per-body selection model and the "active body" concept
- Body sizing (manual + content auto-grow) and live resize during drag
- Rust→Flutter API extensions needed to surface zone data
- Refactoring of the existing editor that should happen first
- Implementation phases, each ending in `flutter run` working and the rust tests still green
- Out-of-scope boundaries

Out of scope:
- The Rust backend itself (covered by `doc/design_zones.md`)
- `.cnnd` migration from function-pin closures to zones (deferred until after both Rust and UI sides land — see "Out of phase plan" in `design_zones.md`)
- Auto-layout / collision avoidance between an HOF and other nodes in its parent network when the HOF resizes. Today nodes are freely positioned and may overlap without complaint; an HOF growing into a neighbor is the same situation a user gets dragging two nodes on top of each other. Deferred to a future task.
- Promote-to-named (extracting a body into a top-level named subnetwork) and the inverse demote-to-inline. Deferred per `design_zones.md` open questions.
- Per-iteration scrubbing of body display (only the last iteration's value is shown for inner-body displayed pins, per `design_zones.md` §UX). Future work.
- Text-format syntax for inline body authoring (deferred — covered separately once data model stabilises).
- Direct-editing mode interactions with zones.

## Scope of this branch — build/test contract

This document's verification gate is **the Flutter app launches with `flutter run`, can author and edit zone-bearing networks end-to-end, and `cd rust && cargo test` is still green**. The Rust workspace must stay compilable throughout (every API extension lands as a Rust patch first); FRB regenerates Dart bindings; Flutter code is brought back online phase by phase.

Where the "must compile and work" boundary sits at the end of each phase:

| Phase exit criterion | Detail |
|---|---|
| `cd rust && cargo test` green | The Rust API extensions land non-breakingly per phase |
| `flutter_rust_bridge_codegen generate` succeeds | New API types reach Dart automatically |
| `flutter run` launches and the editor is usable | The editor may not yet expose new zone capabilities, but is never broken |
| Existing UI capabilities (non-zone nodes) continue to work | Regression guard |
| `cargo clippy` clean and `dart format` / `flutter analyze` clean of *new* warnings | Standard hygiene |

Despite the parent `design_zones.md` build/test contract saying "Flutter consumer code in `lib/` is expected to be broken throughout this branch", the Flutter app currently launches and the editor works for non-zone authoring. The reason: the Rust API surface (`NodeView`, `WireView`, `connect_nodes`) was held at the pre-zones shape during Rust phases 1-6, so the zone fields on `Node` / `NodeType` / `Argument` exist internally but are not yet exposed through FRB. The "expose new Rust data to Flutter" work is therefore not a one-shot recovery step — it's distributed across the phases below, at the points where the editor needs each piece. Every phase ends with the app launching and existing (non-zone) editing functional.

**HOF gating through U3.** HOF types (`map`, `filter`, `fold`, `foreach`) are hidden from the Add Node popup throughout U1, U2, and U3 and re-enabled in U4, when body authoring lands. A freshly placed HOF has no body wires and would immediately fail the Rust-side zone validation rule "every zone-output pin has at least one incoming wire"; suppressing placement avoids that bricked-node state until the user has a UI path to fix it. U3's read-only body rendering doesn't lift the gate — visible body region, but the user still can't wire anything inside. Existing HOFs loaded from `.cnnd` fixtures or constructed programmatically by tests will still validate-error until they're authored via U4's body editor, which is acceptable since no user-facing fixtures contain zone HOFs on this branch yet.

## Motivation

Today, after the Rust-side zones work lands, every HOF node owns a body (`Node.zone: Option<Arc<NodeNetwork>>`) — but the Flutter editor only sees the flat top-level network. The user cannot view bodies, cannot edit them, cannot wire captures, and cannot construct any of the new HOF nodes meaningfully. The feature is unusable from the UI.

The fix is to teach the editor about scope: a body is another `NodeNetwork` that lives inside a node in another `NodeNetwork`, recursively. This affects coordinate math, hit testing, selection, wire drag, pin position resolution, rendering, and the API surface that brings node-network data to the Flutter side.

We want this without a wholesale rewrite of the existing editor. The plan is to refactor the editor's coordinate and hit-testing logic onto a scope-chain abstraction *before* introducing any zone visibility — so that when bodies appear, every code path is already prepared to handle them.

## Concept

### The visual model

A new `map` placed in a network renders as a node with a translucent **body region** inside it. The body's left edge shows the `element` pin facing **inward** (it sources values into the body); the body's right edge shows the `result` pin facing **inward** (it consumes values from the body). The body's interior is a small editable canvas where the user adds nodes and wires.

```
┌── map ────────────────────────────────────────┐
│ xs ●─────────┐                  ┌── ● Iter[U] │   ← external pins
│              │ (zone-input pin) │             │
│              ▼                  ▲             │
│        ┌────────────────────────────┐         │
│        │                            │         │
│  ●     │ element ●─→ [+1] ─→ ● result        │  ← inner pins
│        │                            │         │
│        └────────────────────────────┘         │
│              (translucent body region)        │
└───────────────────────────────────────────────┘
```

### Pin sets

The HOF has four pin sets, two on each side:

| Set | Position | Wire direction | Owner |
|---|---|---|---|
| External inputs | Outer-left | Wires arrive from outside | HOF node `arguments` |
| External outputs | Outer-right | Wires leave to outside | HOF node output pins |
| **Zone-input** (inner-left) | Inside the body, facing inward | Wires *leave* the pin into the body | Sourced by body wires |
| **Zone-output** (inner-right) | Inside the body, facing inward | Wires *arrive* at the pin from the body | HOF node `zone_output_arguments` |

The four pin sets are independent. External pins face the surrounding network; inner pins face the body. The two coexist on the same HOF node's screen footprint.

### Captures

Any wire whose source is *outside* the body and whose destination is *inside* the body is a **capture**. Visually, it's drawn from the source pin (in the outer scope) to the destination pin (in the body's scope) as a single bezier — the wire literally crosses the body's boundary. The user creates one by dragging from an outer pin to an inner pin; the editor sets `source_scope_depth ≥ 1` on the resulting wire.

```
            ┌── map ──────────────────────┐
gap ●───────────────────────┐             │
                            │   (capture wire crossing boundary)
                            ▼
                    element ●─→ [+ gap] ─→ ● result
                            └─────────────┘
```

### Nesting

A body can contain another HOF, which has its own body. Rendering is recursive — each HOF draws its body inside itself, regardless of depth. The editor doesn't impose a depth cap; in practice users will rarely exceed 2-3 levels. Captures can cross multiple boundaries (an inner body wiring to an outer-outer source — depth ≥ 2).

### The "active body"

Each body (and the top-level network) carries its own selection set. Clicking a node in body B makes B the **active body** and replaces B's selection with that one node. Keyboard shortcuts (Delete, Ctrl+C, Ctrl+D, etc.) operate on the active body's selection only. Other bodies keep their selection state but it's inactive until clicked into. There's no explicit "enter body" gesture — the active body simply follows the user's most recent click.

## Coordinate system

### Stored positions are body-local

A node lives inside exactly one `NodeNetwork` — either the top-level one or some HOF's body. Its `position: DVec2` is stored in **that** network's coordinate frame. The top-level network's frame is the editor's logical space (today's behavior, unchanged). A body's frame has its origin at the body's inner top-left.

This matches the data model already in place: `Node.zone` is a fully self-contained `NodeNetwork`, and body nodes' positions naturally live in that body's own coordinate space. Copy/paste of a body, future promote-to-named extraction, and standalone inspection of a body all work without coordinate fix-ups.

### The scope chain

A node is addressed by a **scope chain**: a `Vec<u64>` of HOF node IDs identifying the body it lives in. An empty chain means the top-level network; `[hof_42]` means the body of node 42 in the top-level network; `[hof_42, hof_99]` means the body of node 99 inside the body of node 42, and so on. Every coordinate transform, hit test, and API mutation that previously addressed "the active network" now addresses a scope chain.

### Logical → screen transformation

The current transformation `screen = (logical + panOffset) * scale` extends to:

```
For a node at scope chain [hof_1, hof_2, ..., hof_n] with local position p:
  screen_pos =
    (p + bodyN_origin_local + panOffset) * scale,
  where bodyN_origin_local is:
    hofN.position_in_bodyN-1
      + hofN_inner_offset (the body's top-left within hofN's chrome)
      + recursively continue up to the top-level network
```

In practice, this is implemented as a walk up the scope chain that accumulates a translation in the *outer* frame, then the standard `(logical + panOffset) * scale` to screen. The outer frame is always the top-level network — `panOffset` and `scale` apply once, at the end, never recursively.

Concretely, a `ScopedPosition` helper resolves a `(scope_chain, body_local_position)` pair to a screen position by walking the chain:

```dart
Offset scopedToScreen(
  List<BigInt> scopeChain,
  Offset bodyLocalPos,
  NodeNetworkView rootView,
  Offset panOffset,
  double scale,
) {
  Offset outerFrame = bodyLocalPos;
  NodeNetworkView current = rootView;
  for (final hofId in scopeChain) {
    final hof = current.nodes[hofId]!;
    final hofInnerOffset = computeBodyInnerOffset(hof);
    // hof.position is in `current`'s frame.
    // Add hof's own translation, then the body's inner-left within it.
    outerFrame = apiVec2ToOffset(hof.position) + hofInnerOffset + outerFrame;
    current = hof.zone!; // descend into the body's view
  }
  return logicalToScreen(outerFrame, panOffset, scale);
}
```

`computeBodyInnerOffset(hof)` is a function of the HOF's chrome layout: title bar height, padding, position of inner-left zone-input pins. Once known per zoom level, it's a constant — except for body sizing (see below) which affects where the right-edge inner pins sit.

### Screen → logical (inverse)

For hit testing and drag-drop, the inverse walks the scope tree to find which body a screen point falls into:

```
findContainingScope(screenPos):
  logicalPos = screenToLogical(screenPos, panOffset, scale)
  Walk top-level nodes for one containing logicalPos.
  If it's an HOF and the point falls inside its body region, recurse
  into the body with the body-local point.
  Stop when no deeper containment matches.
  Return (scope_chain, body_local_position).
```

The deepest containment wins — clicking inside an inner body's interior returns that body, not its outer body or the top-level.

## Body sizing

Each HOF stores `body_width` and `body_height` in its `NodeData` (defaulted to e.g. 320×180 logical pixels for new HOFs). The body's rendered size is:

```
body_size = max(stored_size, content_bbox + padding)
```

where `content_bbox` is the union of all body nodes' bounding rectangles in body-local coordinates (after applying current live positions during drag).

### Layout pass — bottom-up sizes, top-down origins

The formula above hides a recursive dependency: an HOF's outer-right external pin position depends on its body's right edge; the body's right edge depends on `body_size`; `body_size`'s `content_bbox` includes inner HOFs whose own footprints depend on *their* body sizes. Resolved naively, every `pinScreenPosition` call cascades down the tree — and `pinScreenPosition` is called from wire rendering, hit testing, the drag overlay, and selection-rect overlap, many times per element per frame.

The editor runs a single **layout pass per frame**, before any pin-position resolution. It produces a cache that `pinScreenPosition` reads in O(1):

```dart
class LayoutCache {
  /// HOF body size in logical pixels, keyed by full scope chain
  /// terminating at the HOF id. The full chain is required because
  /// `node_id` is per-network and can recur across scopes.
  final Map<List<BigInt>, Size> bodySizes;

  /// Body inner-top-left in screen coordinates, same keying.
  /// Folds in panOffset and scale.
  final Map<List<BigInt>, Offset> bodyOrigins;
}
```

Two phases, owned by `ScopeResolver`:

1. **Bottom-up sizes.** Depth-first walk of the network tree. For each HOF reached, recurse into its body first so inner body sizes are known on return. Compute `content_bbox` as the union of every body node's rect in body-local coordinates — using each node's *live* position from the model (drag positions land here for free) and, for body-internal HOFs, the just-computed body size plus chrome. Then `bodySizes[chain ++ [hof_id]] = max(stored, content_bbox + padding)`.

2. **Top-down origins.** With sizes known, walk top-down: the top-level frame's origin is `panOffset * scale`; each nested body's origin is `parent_body_origin + (hof.position + hof_inner_offset) * scale`. Insert into `bodyOrigins`.

`pinScreenPosition` consults `bodySizes` / `bodyOrigins` directly and never recomputes a size. The cache is rebuilt once per `build` — typically every frame during a drag — at total cost O(N) in the number of nodes in the network. Selective invalidation isn't worth the complexity for the realistic N (dozens of nodes, depth ≤ 3); profile only if a deep network shows up.

This discipline is what lets the §"Live resize during node drag" subsection treat live resize as "no special drag-state branch" — the layout pass runs every frame and naturally picks up the model's live positions.

### Resize handles

A small handle on the body's bottom-right corner (and possibly bottom and right edges) lets the user drag the body larger. The user can drag smaller too, but only down to `content_bbox + padding` — content is never hidden. The stored size persists across content additions/removals and across sessions.

### Live resize during node drag

The body is re-rendered every frame using the live (dragging) node positions, so it grows and shrinks in real time as a body node is dragged. No special drag-state branch: each frame's layout pass (see §"Layout pass") reads whatever positions are currently in the model, which is already live during drag (see today's `dragNodePosition` → `updateNodePosition` flow). Nested HOFs cascade naturally — the bottom-up phase resolves the inner body's new size first, and the outer body's `content_bbox` picks it up at the next level up in the same pass.

### Parent-network overlap (out of scope)

When an HOF grows, it may overlap nodes in its parent network. This is not handled — it's the same situation users already get dragging two nodes on top of each other. Auto-layout / collision avoidance is deferred to a future task.

## Data model — Rust API extensions

The Rust core's data structures (per `design_zones.md`) already carry every field zones need. What's missing is exposure through the API layer to Flutter. This section enumerates the additions.

### `NodeView` gains a `zone` field

```rust
pub struct NodeView {
    // ... existing fields ...
    /// Present iff this is an HOF node. Carries the entire body as a nested
    /// view — same shape recursively. None for non-HOF nodes.
    pub zone: Option<ZoneView>,
}

pub struct ZoneView {
    /// Zone-input pins (inner-left) declared by the HOF type.
    pub zone_input_pins: Vec<OutputPinView>,
    /// Zone-output pins (inner-right) declared by the HOF type.
    pub zone_output_pins: Vec<InputPinView>,
    /// All nodes inside the body, keyed by id. Positions are body-local.
    pub nodes: HashMap<u64, NodeView>,
    /// All wires inside the body — same shape as the outer wires Vec.
    pub wires: Vec<WireView>,
    /// Wires terminating at zone-output pins (the body's returns). Logically
    /// these live in the body's frame; they're surfaced separately because
    /// their destination is the HOF itself, not a body-internal node.
    pub zone_output_wires: Vec<WireView>,
    /// Stored width/height for body sizing. The renderer uses
    /// max(stored_size, content_bbox + padding).
    pub stored_width: f64,
    pub stored_height: f64,
    /// Body-internal selection / display / validation state, mirrored
    /// from the body's NodeNetwork.
    pub selected_node_ids: Vec<u64>,
    pub selected_wires: Vec<WireIdentifier>,
    pub validation_errors: Vec<String>,
}
```

`OutputPinView` for zone-input pins reuses the same struct as external output pins (zone-input pins are sources from the body's perspective). `InputPinView` for zone-output pins similarly reuses the input-pin struct.

The recursion bottoms out naturally: a body node that isn't an HOF has `zone: None`; an HOF node inside a body has `zone: Some(ZoneView { ... })` populated the same way.

### `WireView` carries source kind and scope depth

```rust
pub struct WireView {
    pub source_node_id: u64,
    pub source_pin: APISourcePin,        // NEW
    pub source_scope_depth: u8,          // NEW (0 = local, ≥1 = ancestor)
    pub dest_node_id: u64,
    pub dest_param_index: usize,
    pub destination_argument_kind: APIArgumentKind,  // NEW
    pub selected: bool,
}

pub enum APISourcePin {
    NodeOutput { pin_index: i32 },       // includes legacy -1 function pin
    ZoneInput { pin_index: usize },      // inside-facing source on a zone-owning node
}

pub enum APIArgumentKind {
    External,    // sourced from destination's `arguments`
    ZoneOutput,  // sourced from destination's `zone_output_arguments`
}
```

Existing wires up-convert with `source_pin = NodeOutput { pin_index }`, `source_scope_depth = 0`, `destination_argument_kind = External` — semantically identical to today.

### `WireIdentifier` extends symmetrically

Used by selection batch APIs:

```rust
pub struct WireIdentifier {
    pub source_node_id: u64,
    pub source_pin: APISourcePin,
    pub source_scope_depth: u8,
    pub destination_node_id: u64,
    pub destination_argument_index: usize,
    pub destination_argument_kind: APIArgumentKind,
}
```

### Mutation APIs grow a `scope_path` parameter

Every mutation API function that currently operates on the active top-level network grows a `scope_path: Vec<u64>` parameter — the chain of HOF node IDs identifying the target body. Empty path = top-level (today's behavior; default for non-zone calls).

The full list (cross-referenced against `structure_designer_api.rs`):

```
add_node(scope_path, node_type_name, position, drag_source) -> u64
move_node(scope_path, node_id, position)
duplicate_node(scope_path, node_id) -> u64
delete_selected(scope_path)
connect_nodes(scope_path, source_node_id, source_pin: APISourcePin,
              source_scope_depth: u8, dest_node_id,
              dest_param_index, dest_argument_kind: APIArgumentKind)
can_connect_nodes(...)              // same shape
auto_connect_to_node(...)           // same shape
get_compatible_pins_for_auto_connect(...) // includes scope_path on both sides
select_node(scope_path, node_id)
toggle_node_selection(scope_path, node_id)
add_node_to_selection(scope_path, node_id)
select_nodes(scope_path, node_ids)
clear_selection(scope_path)         // clears just that body's selection
set_node_display(scope_path, node_id, is_displayed)
toggle_output_pin_display(scope_path, node_id, pin_index)
set_return_node_id(scope_path, node_id)   // top-level only — body has no return
move_selected_nodes(scope_path, delta)
set_<TypeData>(scope_path, node_id, data)  // every node-property setter
```

For backwards compatibility within this branch, sources of the form `connect_nodes(source_node_id, source_output_pin_index, dest_node_id, dest_param_index)` can be retained as legacy wrappers that default the new fields. Or, since this branch is allowed to break Dart-side compatibility freely, we just port every call site at the time we land the API change.

The "active body" — the body keyboard shortcuts dispatch into — is a pure Flutter concern (see §"`ScopedSelection` model"). Rust does not track it: every mutation API takes `scope_path` explicitly, so a Rust-side mirror would be duplicate state with no consumer (and the usual two-sources-of-truth bugs when a keyboard shortcut fires between the Flutter update and the Rust push). Selection-clear-on-canvas-click is dispatched as `clear_selection(scope_path)` against the clicked scope, full stop.

### Body resize API

```
set_zone_size(scope_path, hof_node_id, width, height)
```

Updates the stored width/height on the HOF's data. Persists across saves. Undoable.

### Wire-creation API generalisation

`connect_nodes` learns to handle all four wire shapes from `design_zones.md`:

| Wire role | Stored in | `source_pin` | `source_scope_depth` | `destination_argument_kind` |
|---|---|---|---|---|
| Regular in-network | destination's `arguments` | `NodeOutput { pin_index }` | `0` | `External` |
| Capture (outside → inside body) | body-internal destination's `arguments` | `NodeOutput { pin_index }` | `≥ 1` | `External` |
| Iteration value (zone-input → body node) | body-internal destination's `arguments` | `ZoneInput { pin_index }` | `≥ 1` | `External` |
| Body return (body node → zone-output) | HOF's `zone_output_arguments` | `NodeOutput { pin_index }` | `0` (relative to body) | `ZoneOutput` |

The signature carries all of `source_pin`, `source_scope_depth`, and `destination_argument_kind` so a single call can construct any of the four. Validation (rules in `design_zones.md` §Validation) catches malformed combinations.

## Flutter-side model

### `PinReference` grows scope chain and pin kind

```dart
class PinReference {
  final BigInt nodeId;
  final List<BigInt> scopeChain;   // NEW — empty = top-level
  final PinKind pinKind;           // NEW — replaces PinType (or supplements it)
  final int pinIndex;
  final String dataType;
  // ...
}

enum PinKind {
  externalInput,    // today's PinType.input
  externalOutput,   // today's PinType.output (with pinIndex >= 0)
  functionPin,      // today's PinType.output with pinIndex == -1; deprecated
  zoneInput,        // inner-left source pin on an HOF
  zoneOutput,       // inner-right destination pin on an HOF
}
```

`PinKind` replaces the `(PinType, pinIndex == -1)` pair that today distinguishes function pins from regular outputs, and accommodates the new zone-pin kinds. `PinKind.functionPin` is a **transitional value** carried through U1-U6 so function-pin rendering, hit testing, and wire creation continue working unchanged while the parent Rust design's `DataType::Function` / `Closure` machinery is still live. U7 removes the `functionPin` arm from `PinKind` (and the corresponding `NODE_VERT_WIRE_OFFSET_FUNCTION_PIN` constant, title-bar `PinWidget`, painter `pinIndex == -1` arms, etc.) once the Rust dead-weight cleanup lands. Until then, every `switch (pinKind)` carries an arm for it. This is an intentional transitional smell scoped to the lifetime of this branch.

#### `scopeChain` convention

A pin's `scopeChain` is the scope of the pin's **owner node** — i.e., the network the node containing the pin lives in. This applies uniformly to all `PinKind`s: an `externalInput`/`externalOutput`/`functionPin` pin on node N has `scopeChain` equal to N's containing network; a `zoneInput`/`zoneOutput` pin on HOF H has `scopeChain` equal to H's containing network (**not** the body's scope, even though the pin "faces into" the body).

#### Computing `source_scope_depth` at wire creation

The destination's *evaluation scope* — the scope from which `evaluate_arg` walks up to find the source — is what `source_scope_depth` measures distance from. For most wires the evaluation scope equals `dest.scopeChain`. For body-return wires (`destination_argument_kind == ZoneOutput`), the wire is stored in the HOF's `zone_output_arguments` and evaluated against the body's scope, which is one level deeper than the HOF itself:

```dart
List<BigInt> effectiveDestScope = (dest.argumentKind == ZoneOutput)
    ? [...dest.scopeChain, dest.nodeId]
    : dest.scopeChain;

// source.scopeChain must be a prefix of effectiveDestScope; otherwise reject.
int sourceScopeDepth = effectiveDestScope.length - source.scopeChain.length;
```

Verifying against the four wire roles from §"Wire-creation API generalisation":

| Wire role | `source.scopeChain` | `dest.scopeChain` | `effectiveDestScope` | `source_scope_depth` |
|---|---|---|---|---|
| Regular | `S` | `S` | `S` | `0` |
| Capture (outer → inner body) | `S` | `S ++ [...]` (descendant) | `dest.scopeChain` | `≥ 1` |
| Iteration-value (zoneInput → body node) | `S` (HOF's scope) | `S ++ [hof_id]` (body) | `dest.scopeChain` | `1` (or deeper for outer-HOF references — those are captures of `ZoneInput` sources) |
| Body return (body node → zoneOutput) | `S ++ [hof_id]` (body) | `S` (HOF's scope) | `S ++ [hof_id]` (body) | `0` |

Rejection rule: if `source.scopeChain` isn't a prefix of `effectiveDestScope` (e.g. sibling-body source, or descendant source for a non-`ZoneOutput` destination), the connection is invalid.

### `WireView` extends in lockstep

The Flutter `WireView` (regenerated from FRB) gains `sourcePin`, `sourceScopeDepth`, `destinationArgumentKind` fields. Wire-rendering code reads them.

### `ScopedSelection` model

The model gains an "active scope chain":

```dart
class StructureDesignerModel extends ChangeNotifier {
  // existing:
  NodeNetworkView? nodeNetworkView;
  // NEW:
  List<BigInt> activeScopeChain = const [];  // empty = top-level
}
```

`activeScopeChain` is the body that keyboard shortcuts operate on. It lives Flutter-side only — Rust mutation APIs always receive `scope_path` explicitly from the call site, with no Rust-side default.

Each body's selection set lives Rust-side on its own `NodeNetwork.selected_node_ids`. The Flutter side reads it via the recursive `ZoneView.selected_node_ids` field on the API surface.

### Scope-aware coordinate helpers

A single `ScopeChainResolver` (or a set of free functions in `node_network.dart`) replaces today's inline `logicalToScreen` / `screenToLogical` calls wherever a scope chain is involved:

```dart
class ScopeResolver {
  final NodeNetworkView root;
  final Offset panOffset;
  final double scale;

  /// Cached body sizes and inner-top-left origins for every HOF in the
  /// tree. Rebuilt once per `build` by `runLayoutPass()`; all coordinate
  /// queries below read from it without recomputing. See §"Layout pass".
  final LayoutCache layout;

  /// Rebuild `layout` from the current `root` + `panOffset` + `scale` +
  /// live node positions. Called once per frame before any pin-position
  /// or hit-test query. O(N) in total nodes in the network.
  void runLayoutPass();

  /// Compute screen position of a body-local point at the given scope chain.
  Offset scopedToScreen(List<BigInt> scopeChain, Offset bodyLocal);

  /// Walk the screen point down through containing bodies; return the
  /// deepest scope chain that contains it and the body-local coordinate.
  ({List<BigInt> scopeChain, Offset bodyLocal}) findContainingScope(Offset screenPos);

  /// Compute screen position of a pin given its PinReference. Reads
  /// from `layout`; does not recompute body sizes.
  Offset pinScreenPosition(PinReference pin);
}
```

Today's coordinate code (in `node_network.dart`, `node_network_painter.dart`, `node_widget.dart`) is rewritten to go through this resolver. After Phase U1, every coordinate computation in the editor speaks scope chain — even when the chain is always empty, which it is for top-level work.

## Rendering

### Widget tree

The widget tree gains a level of recursion:

```
NodeNetwork (StatefulWidget — owns pan/zoom, focus, selection rect)
  Stack
    NodeNetworkInteractionLayer (paints top-level wires)
    Positioned(NodeWidget) × top-level nodes
      ├── (regular node) — same as today
      └── (HOF node) — contains:
           Container (HOF chrome — title, external pins)
             Stack
               ZoneBodyLayer (paints body wires + capture-wire stubs)
               Positioned(ZoneInputPinWidget) × zone-input pins (inner-left)
               Positioned(ZoneOutputPinWidget) × zone-output pins (inner-right)
               Positioned(NodeWidget) × body nodes
                 └── recursively, same shape as top-level NodeWidget
    SelectionRectangle (top-level overlay)
```

Each body has its own painter (`ZoneBodyLayer`) that draws the body's interior wires. Bodies don't have their own pan/zoom — they inherit the top-level `panOffset` and `scale` via the scope resolver. Body content is rendered at the same scale as everything else.

Wires that span scopes (captures) are painted by the **outermost relevant layer** — typically the top-level `NodeNetworkInteractionLayer`, which can reach pins at any depth via the scope resolver. This avoids the chicken-and-egg problem of a body painter needing to know about pins outside itself.

### Zoom levels

At normal zoom (1.0), bodies render fully — all body nodes visible and interactable. At medium zoom (0.6), bodies stay visible but at the same scale ratio; small bodies become harder to read but remain functional. At far zoom (0.35), bodies render as **collapsed**: the HOF shows just its title and external pin stubs, with an "[N nodes]" indicator where the body would be. Body content isn't drawn — too small to see anyway. Capture wires that would cross into the collapsed body terminate at a stub on the HOF's edge; the stub shows a small count indicator (e.g. "↪3" for three incoming captures) so the user can see at a glance that the collapsed body has cross-scope inflows without expanding it. This is the collapsed-state equivalent of the per-crossing boundary marker from §"Wire rendering across scopes".

The collapse threshold is configurable; the test of "is this body readable enough to render its content" is `body_screen_height >= some_minimum` (e.g. 60 px). Bodies that fail render collapsed even at normal zoom (a tiny body nested four deep, say).

### Pin position resolution

All pin position math goes through one function:

```dart
Offset pinScreenPosition(PinReference pin) {
  // Resolve the node's screen position via the scope chain.
  final ownerScopeChain = pin.scopeChain;
  final node = lookupNode(ownerScopeChain, pin.nodeId);

  switch (pin.pinKind) {
    case PinKind.externalInput:
      // node's local pos + (0, pin_vert_offset) → translate via chain
    case PinKind.externalOutput:
      // node's local pos + (node_width, pin_vert_offset) → translate via chain
    case PinKind.zoneInput:
      // pin lives on the inner-left of node (which is an HOF) — at
      // body's inner-left edge, computed from chrome layout
    case PinKind.zoneOutput:
      // pin lives on the inner-right of node — at body's inner-right edge,
      // which depends on body_size (= max(stored, content_bbox + padding))
  }
}
```

For zone-output pins, the body's inner-right edge moves as the body grows; `pinScreenPosition` reads the current size from `LayoutCache.bodySizes[chain ++ [hof_id]]` rather than recomputing it. The layout pass (see §"Layout pass" under §"Body sizing") has already run before any pin-position resolution this frame, so the lookup is O(1) and consistent across every call site (wire rendering, hit testing, drag overlay, selection-rect overlap). Same applies for zone-input pins if we choose to anchor them to the right of the chrome and let the chrome grow with the body height.

### Wire rendering across scopes

A wire's source pin and destination pin can live in different scopes. The painter resolves both to screen positions via `pinScreenPosition`, then draws the bezier as it does today. **Captures** — wires with `source_scope_depth ≥ 1` whose endpoints sit in different scopes — additionally render a small **boundary marker** at each body-boundary crossing along the bezier path: a filled circle ~6 logical px in the wire's data-type color, scaled with zoom. The marker is the primary affordance for "this is a capture"; relying on the bezier visibly crossing the translucent body region is unreliable, since bezier control points routinely route the curve such that it's ambiguous whether the wire enters the body. Multiple-boundary captures (depth ≥ 2 — a wire spanning outer scope into a deeply nested body) get one marker per boundary crossed. Iteration-value references (`zoneInput` source) and body-return wires (`zoneOutput` destination) don't cross any body boundary — their pin endpoints sit *on* the boundary — so they get no marker.

The marker's screen position is computed by intersecting the bezier with the body's rectangular bounds (or, more cheaply, by sampling the bezier and picking the first sample inside the body's rect — same sampling the selection-rect overlap test already does).

Drawing order: top-level wires first; then for each top-level HOF, its body's interior wires; then capture wires (drawn after both endpoints' scopes are laid out). The simplest implementation paints all wires from the outermost layer in one pass, because by then every node and every pin has a determined screen position — the recursive structure is for hit testing and authoring, not for rendering layering.

## Interaction model

### Selecting in a body

A click that lands inside body B's interior — and not on a node — makes B the active body and clears B's selection. A click on a node inside B makes B active and replaces B's selection with that node. The selection rectangle drawn from inside a body operates within that body. Modifier keys (Ctrl=toggle, Shift=add) work as today, just confined to the active body.

A click on the top-level canvas (outside all HOFs) makes top-level the active scope and clears its selection. Clicking on an HOF's *chrome* (title bar, external pin area, body's translucent boundary) selects the HOF in its parent scope — same as clicking any node.

### Wire creation

Drag from any pin to any pin works the same as today; the editor figures out the wire shape from the two pin references:

| Source pin scope | Dest pin scope | Source kind | Dest kind | Resulting wire |
|---|---|---|---|---|
| Same as dest | Same as source | NodeOutput | externalInput | Regular wire |
| Outer (ancestor) | Inner (descendant body) | NodeOutput | externalInput | Capture (`source_scope_depth ≥ 1`) |
| ZoneInput pin of body B | Inside body B | ZoneInput | externalInput | Iteration-value reference |
| Inside body B | ZoneOutput pin of body B | NodeOutput | zoneOutput | Body return |

`canConnectPins` extends with the corresponding combinatorial rules; `connectPins` packages the right `APISourcePin` / `APIArgumentKind` / `source_scope_depth` and calls the new `connect_nodes`.

Dropping a wire in empty space inside a body shows the Add Node popup filtered by the source's data type, with the new node created **in that body**. The existing `_handleWireDropInEmptySpace` path generalises: it passes the drop's scope chain to `createNode`.

### Add Node popup

The popup is invoked by right-clicking on empty canvas (or by dropping a wire). The new node is created in whatever scope was right-clicked into. The popup itself doesn't change — it's the scope that's passed into `createNode` that changes. The Rust-side `add_node` learns to add to a body when given a non-empty scope path.

### Drag-and-drop of nodes

Dragging a node within its scope works as today (live position updates via `dragNodePosition`, commit on release via `updateNodePosition`). The body resizes live during the drag per the body-sizing rules above. Dragging a node *across* scope boundaries (move from one body to another, or from a body to its parent) is **out of scope** for the first cut — we'll add it later if needed. For now, cross-scope moves require copy/paste or a future "promote to parent scope" / "demote to body" action.

### Copy/paste

Existing copy/paste works within a scope (Ctrl+C copies the active body's selection; Ctrl+V pastes into the active body at the cursor). Cross-scope paste — copy from body A, paste into body B — works as long as the pasted nodes don't carry cross-scope wires that would no longer resolve; broken wires are dropped on paste, matching today's behavior for cut-then-paste of partial selections.

Copy/paste of an HOF carries its body along (the Rust data model is already shaped this way). The visual effect is the entire HOF region appears in the paste destination.

### Undo/redo

Every mutation that takes a `scope_path` parameter gets an undo command that records the scope path alongside the operation. The existing undo system (per `doc/design_global_undo_redo.md`) extends naturally — commands grow a `scope_path: Vec<u64>` field; snapshot/restore routes through the body identified by the chain rather than the active top-level network.

Body resize via the drag handles uses the same begin/end coalescing pattern as node drag — `beginZoneResize` / `endZoneResize`, with one coalesced undo command per drag.

## Refactoring before features

Before any zone-specific UI lands, three refactors prepare the editor:

### R1. Coordinate transformation through a resolver

Replace inline `logicalToScreen` / `screenToLogical` calls with `ScopeResolver` methods. The chain is always empty at this point, but every call site now speaks the scope-chain shape. No visible behavior change.

### R2. Pin position math through `pinScreenPosition`

The current `_getPinPositionNormal` / `_getPinPositionZoomedOut` (in `node_network_painter.dart`) and the duplicate inline logic in `node_network.dart`'s `_getPinScreenPosition` (used by selection-rect overlap detection) consolidate into one `pinScreenPosition(PinReference)` on `ScopeResolver`. All wire rendering and hit testing go through it. No visible behavior change.

### R3. Hit testing via scope-aware containment

`_isClickOnNode` / `getNodeAtPosition` in `node_network.dart` become `findNodeAtScreenPosition(Offset)` returning `(scope_chain, NodeView)` or null. Today it walks top-level nodes only; after the refactor it's prepared to walk down into bodies (but with no bodies present, behavior is identical).

These three refactors land in Phase U1 and Phase U2 before any visible zone rendering.

## Implementation phases

Each phase ends with `cd rust && cargo test` green and `flutter run` launching a working editor. Phases are sequential; later phases assume earlier ones.

The branch starts on `zones` after Rust phase 6 lands. Baseline: ~2005+ Rust tests passing, zone data model and evaluation complete on the Rust side, Flutter UI launching and editing non-zone networks (zone data not yet exposed through FRB).

### Phase U1: Coordinate refactor (R1 + R2)

**Goal.** Land the `ScopeResolver` abstraction and route every coordinate and pin-position computation through it. Empty scope chains everywhere — no behavioral change.

**Scope.**
- Filter HOF types (`map`, `filter`, `fold`, `foreach`) out of `AddNodePopup` (the registry-driven type list). The Rust node-type registry still carries them — this is a UI-side filter only, reverted in U4 once body authoring lands. See §"Scope of this branch — build/test contract" → "HOF gating through U3" for rationale.
- Grow `PinReference`: add `scopeChain: List<BigInt>` (defaulted to `const []`) and `pinKind: PinKind` (replacing today's `PinType` + `pinIndex == -1` discriminator pair). `PinKind` values needed in this phase are `externalInput`, `externalOutput`, and `functionPin` (the legacy arm); `zoneInput` and `zoneOutput` are added in U3 when they become reachable. The `ScopeResolver` and `pinScreenPosition` consume these.
- Add `lib/structure_designer/node_network/scope_resolver.dart` with the helpers from §"Flutter-side model — Scope-aware coordinate helpers".
- Refactor `node_network.dart` — `logicalToScreen`, `screenToLogical`, `_getPinScreenPosition`, `_isClickOnNode`, `getNodeAtPosition`, `_wireOverlapsRect`, `_handleWireDropInEmptySpace`, the selection-rect handler — to use the resolver.
- Refactor `node_network_painter.dart` — `_getPinPositionNormal`, `_getPinPositionZoomedOut`, `findWireAtPosition` — to use the resolver.
- Refactor `node_widget.dart` — `build`'s `Positioned` math, `_handleNodeTap`, `_handleNodeDrag` — to express positions as `(scopeChain, bodyLocalPosition)`.

**Tests.** No new automated tests. Manual regression: every pan/zoom, hit-test, wire-render, drag, and selection-rect path still behaves identically to baseline.

**Verification.** `flutter run` launches and the editor is indistinguishable from baseline; `cargo test` still green.

**Gotchas.**
- The resolver depends on knowing the root `NodeNetworkView`. Pass it explicitly rather than via Provider lookups inside the resolver — keeps the resolver pure and testable.
- `pinScreenPosition` must handle the legacy function pin (`pinKind: functionPin`) — keep the existing `NODE_VERT_WIRE_OFFSET_FUNCTION_PIN` formula in that arm.
- Selection-rect overlap testing currently samples bezier curves at 20 points; nothing changes here except that pin endpoint resolution goes through the resolver.

### Phase U2: Hit-testing refactor (R3) + scope-aware mutation calls

**Goal.** Hit-test code walks a scope tree (still always one level deep), and the model's mutation methods accept a `scopeChain` parameter that they pass through to the Rust API. Empty chain everywhere → no behavioral change.

**Scope.**
- `findNodeAtScreenPosition(Offset)` on the resolver, returning `({List<BigInt> scopeChain, NodeView node})?`.
- `StructureDesignerModel` mutation methods grow optional `scopeChain` parameters (default `const []`): `createNode`, `moveNode`, `connectPins`, `deleteSelected`, `setSelectedNode`, etc. The methods pass them through to the Rust API.
- Rust API: every mutation function grows its `scope_path: Vec<u64>` parameter. Implementations route to the named body. For an empty path, behavior matches the existing single-network call.
- `activeScopeChain` field added to `StructureDesignerModel`, defaulted to `const []`. Keyboard shortcut handlers read it when dispatching Delete / Ctrl+C / Ctrl+V / Ctrl+D.

**Tests.** Existing Rust tests cover the empty-chain case. Add Rust unit tests that exercise the new `scope_path` parameter with a synthetic two-level network (top-level + a single HOF with a body) to confirm the dispatch works.

**Verification.** `flutter run` launches; everything still feels identical (active chain is always empty).

**Gotchas.**
- Some Rust mutations currently look up `active_node_network_name` and operate on the top-level network. The right pattern is: resolve `(active_network_name, scope_path)` to a `&mut NodeNetwork` via a helper that walks `Node.zone_mut()` down the path, then operate on it. This helper is shared by every scoped mutation.
- Undo commands grow a `scope_path` field. For empty paths, behavior is unchanged. For body paths, the snapshot/restore captures the body's state — `Arc::make_mut` on the body's `Arc<NodeNetwork>` handles the CoW correctly.

### Phase U3: Render HOFs with empty body region (read-only)

**Goal.** HOFs render as containers with a visible (empty for now) body region. Zone-input and zone-output pins are visible on the inner edges. Body nodes are NOT rendered yet (the `ZoneView.nodes` is shown as an opaque "[N nodes]" placeholder). The user can see that HOFs have bodies but can't yet edit them.

**Scope.**
- Rust API: implement the `NodeView.zone: Option<ZoneView>` field. Populate it from `Node.zone` recursively. `ZoneView` includes zone-input/zone-output pin definitions resolved from the HOF's `NodeType`, the body's `stored_width`/`stored_height`, and a node count for the placeholder.
- Add `set_zone_size(scope_path, hof_node_id, width, height)` API.
- Flutter: extend `NodeWidget` to detect `node.zone != null` and render the body region. Draw the translucent rectangle, the zone-input pins on inner-left, zone-output pins on inner-right, and a centered placeholder ("3 nodes inside"). No body-node rendering yet.
- Pin position resolution: `pinScreenPosition` learns `PinKind.zoneInput` and `PinKind.zoneOutput`. Wire-rendering for zone-output wires (from body to zone-output pin) is skipped until U4 — the wires exist Rust-side but aren't surfaced in `ZoneView.wires` yet.
- Layout cache (§"Layout pass"): add `LayoutCache` and `runLayoutPass()` on `ScopeResolver`. In U3, bodies have no content, so `bodySizes[chain] = stored_size` always; the bottom-up walk machinery is in place but trivial. `pinScreenPosition` reads from the cache from this phase forward. U4 extends the bottom-up phase with `content_bbox`.
- Body sizing infrastructure: even without body content, the body renders at `stored_size`. Resize handles can land in this phase or U4.

**Tests.** Add a Flutter widget test that places an HOF node in a test network and asserts the body region renders with the right pin layout.

**Verification.** `flutter run`; place a `map` node; see the HOF render with an empty body region with `element` and `result` pins visible on its inner edges.

**Gotchas.**
- The HOF's overall screen footprint now grows to include the body. External pin positions need to account for this (the right-edge `Iter[U]` pin sits past the body region). Update the external-pin position formulas accordingly.
- Container hit-testing: clicks on the body's interior should NOT select the HOF in this phase (since the body is read-only and there's nothing to interact with). Reserve the click for U4 when body content arrives.

### Phase U4: Edit inside body — add, move, delete, intra-body wires

**Goal.** The user can author body content end-to-end: add nodes inside, move them, delete them, draw wires between them, draw the body-return wire from a body node to a zone-output pin. Cross-scope wires (captures, zone-input usage) are NOT in this phase — they come in U5.

**Scope.**
- Re-enable HOF types (`map`, `filter`, `fold`, `foreach`) in `AddNodePopup` (filter removed in U1). With body authoring now functional, freshly placed HOFs are no longer bricked.
- `ZoneView.nodes` populated with the body's NodeViews. Body wires (`ZoneView.wires`) populated, with all wires confined to the body's scope.
- Flutter: body nodes render inside the body region via the recursive widget tree (a `Stack` of `Positioned(NodeWidget)` inside the HOF's chrome). Body wires render via a `ZoneBodyLayer` painter that uses the scope resolver.
- Hit testing: `findNodeAtScreenPosition` walks into bodies when the click lands inside one. Returns the deepest containing body's scope chain.
- Selection: clicking a body node sets `activeScopeChain = [hof_id]`, replaces the body's selection. Right-clicking on body empty space opens the Add Node popup with `createNode` parameterized by the body's scope.
- Drag: dragging a body node updates its body-local position; the body resizes live per §"Body sizing".
- Extend the layout cache's bottom-up phase (introduced in U3) to compute `content_bbox` as the union of every body node's rect; `bodySizes` becomes `max(stored, content_bbox + padding)` rather than always `stored`. Live drag of a body node triggers a per-frame cache rebuild — no special-casing needed since `runLayoutPass()` reads the model's live positions.
- Wire creation **within a body**: drag pin to pin works as today, using the scope-aware `connectPins`. Drag from a body node to a zone-output pin lands a body-return wire.
- Zone-output pin: needs to accept incoming wire drops (a `DragTarget`).
- Resize handles on the body's bottom-right corner, with begin/end coalescing for the resize-drag undo command.

**Tests.** A Flutter integration test that constructs a `map` node, opens its body, adds a node inside, wires it to `result`, and verifies the network evaluates correctly.

**Verification.** `flutter run`; create `range → map → collect`; open the map's body; add an `expr` node `element + 1`; wire it to `result`; display the collect node and confirm it produces `[1, 2, 3]`.

**Gotchas.**
- The body has its own `next_node_id`. The Rust `add_node` for a body scope path routes to the body's counter, not the top-level's.
- Selection rectangles inside a body should be confined to that body — they pick body nodes only, not nodes in other scopes. The rectangle is drawn in screen space but rectangle-overlap testing must happen against body-scope nodes after their screen positions are resolved.
- The function pin (the one in the title bar) is still present on HOF nodes per legacy, but it's never useful for zones — it should be visually suppressed or hidden on HOF nodes specifically. The clean removal happens at the post-migration cleanup phase per the parent design doc.

### Phase U5: Cross-scope wires — captures and zone-input usage

**Goal.** Authoring captures (drag from outside pin to inside pin) and iteration-value references (drag from zone-input pin to body node input) works.

**Scope.**
- Wire creation: `canConnectPins` allows source-outside / dest-inside pairs. `connectPins` computes the right `source_scope_depth` from the two pins' scope chains.
- A drag from a zone-input pin (which is a source pin on an HOF) to a body node input is just a pin-to-pin drag; `PinReference.pinKind = zoneInput` flows through and the resulting wire has `source_pin = ZoneInput { pin_index }`, `source_scope_depth = 1`.
- A drag from any outer-scope output pin to a body-internal input pin creates a capture: `source_pin = NodeOutput { pin_index }`, `source_scope_depth = depth_from_dest_to_source_scope`.
- Wire rendering: the top-level painter draws all wires, including those that span scopes. Pin endpoint resolution via the scope resolver handles both ends regardless of depth.
- Drag overlay: during a wire drag, `dragWire` records the start `PinReference` (which carries its scope chain). The drop target's `onWillAccept` checks `canConnectPins` against the start's scope-aware reference. Existing drag infrastructure works without modification because `PinReference` already carries the scope chain after U2.
- Drop in empty space inside a body, with the source being an outer-scope pin: opens the Add Node popup in the body's scope. The newly-created body node gets the outer source wired in as a capture.

**Tests.** A Flutter integration test: author `range → map(body: element + k) → collect` where `k` is a top-level `int` node captured into the body. Confirm evaluation yields `[k, k+1, k+2]`.

**Verification.** Capture wires render as bezier curves crossing the body's boundary. Removing a capture via Delete works in the active body. Validation errors from Rust (e.g. a depth-2 capture whose source got deleted) surface as red node borders correctly.

**Gotchas.**
- The drag overlay's `wireEndPosition` is in screen coordinates, so cross-scope dragging "just works" visually. The only subtle case is when the start pin and end pin are in different scopes — make sure `_getPinPositionAndDataType` for the dragged source resolves correctly (it does, via `pinScreenPosition`).
- Capture-wire endpoint resolution requires looking up an outer-scope node from inside a body. The scope resolver's `pinScreenPosition` does this by walking up the scope chain — make sure the chain stored on the source `PinReference` is the *source's* chain, not the destination's.
- `connectPins` computes `source_scope_depth` per the formula in §"`PinReference` grows scope chain and pin kind" → "Computing `source_scope_depth` at wire creation": `effectiveDestScope.length - source.scopeChain.length`, where `effectiveDestScope` is `dest.scopeChain ++ [dest.nodeId]` for `ZoneOutput` destinations and `dest.scopeChain` otherwise. Reject pairs where `source.scopeChain` isn't a prefix of `effectiveDestScope`. The body-return case (`ZoneOutput` destination) is the one that needs the `effectiveDestScope` adjustment — without it, body-return wires would compute a negative depth.

### Phase U6: Nested-zone rendering and authoring

**Goal.** An HOF placed inside a body renders its own inline body. Authoring inside the inner body works the same as authoring inside an outer body. Captures spanning multiple boundaries (depth ≥ 2) work.

**Scope.**
- Recursive `NodeView.zone` is already populated by U3's API. Recursive `NodeWidget` rendering handles depth naturally — a body node that has `node.zone != null` recursively renders its body.
- Scope-aware coordinate resolver handles arbitrary chain depths.
- Active scope chain can be arbitrary depth.
- Pin position resolution for inner-body pins walks the full chain.
- Body sizing cascades: inner body grows → outer body grows.
- Zoom-level collapsing per §Rendering — bodies that fall below the readability threshold render collapsed regardless of their nesting depth.

**Tests.** A Flutter integration test that authors `range → fold(body: range → map(body: element + acc) → fold) → display`, exercising nested HOFs end-to-end.

**Verification.** Place an HOF inside another HOF's body. Both bodies edit correctly. A capture from the top-level scope into an inner body renders as a wire crossing two boundaries.

**Gotchas.**
- The layout pass (§"Layout pass") is O(N) per frame in the total number of nodes in the network — each node is visited exactly once during the bottom-up size walk regardless of nesting depth. For realistic networks (dozens of nodes, depth 2-3) this is negligible even when running every frame during a drag. Profile if a deep network shows up.
- Nested-body hit-testing should not need to "tunnel" — Flutter's `Stack` natural child-before-parent containment handles it as long as body widgets are children of the HOF widget. Verify this with a Flutter test that clicks inside an inner body.

### Phase U7: Polish

**Goal.** UX polish, validation surface, deferred-feature placeholders.

**Scope.**
- Validation error rendering for zone-specific errors (rules 1-3 in `design_zones.md` §Validation). Map each error type to a node-border highlight + tooltip.
- Body resize handle visual polish (cursor changes, hover affordance).
- Selection rectangle inside a body: visually contained within the body region (clip the rectangle to the body bounds).
- Optional: "focus body" affordance (per Option C in design discussion) — a button on the HOF that temporarily zooms the body to fill the editor area.
- Optional: subtitle / display improvements for HOF nodes (per-iteration display is deferred).
- Copy/paste of HOFs containing bodies: verify visual paste lands correctly; bodies travel along.
- Function pin removal (coordinate with the parent Rust design's cleanup phase) — once Rust removes `DataType::Function`, `Closure`, and the `-1` pin convention, Flutter removes its function-pin rendering, the `NODE_VERT_WIRE_OFFSET_FUNCTION_PIN` constant, the `pinKind: functionPin` arm, and the title-bar `PinWidget` in `node_widget.dart`.

**Tests.** Validation-error surface tests; manual UX walkthrough.

**Verification.** Full editing experience for zone-bearing networks. Feature parity with what the design doc envisioned.

**Gotchas.**
- Focus mode (if implemented) should not break the recursive widget tree — it's a presentation filter (CSS-equivalent hide/show + transform), not a structural change.
- Validation errors on body-internal nodes need to be visible — they live on `ZoneView.validation_errors`, and the body painter should render the node's red border / tooltip exactly as the top-level painter does.

## Open questions

1. **Zone-input pin clustering.** When an HOF has more than one zone-input pin (only `fold` today, with `acc` + `element`), how should they be visually stacked? Vertically along the inner-left edge mirrors the external-pin convention. Confirm during U3.

2. **Drag-handle affordance.** Bottom-right is conventional, but does the body also want a right-edge handle (width-only) and a bottom-edge handle (height-only)? Decide during U4 based on usability.

3. **Cross-scope node moves.** Dragging a body node to another scope (move from body A to top-level, or to body B) is deferred. UX gesture if/when added: hold modifier + drop on target scope? Right-click → "Move to parent scope"? Defer until users ask.

4. **Capture wire visual style.** Should capture wires use a distinct dash pattern or color to flag boundary crossing? Today's design says no (the boundary crossing itself is the visual cue). Revisit during U5 if it's hard to read.

5. **Zoom-level body collapse threshold.** A 60-px-screen-height cutoff is a guess. Tune during U3 / U6 based on actual feel.

6. **Per-body camera settings.** `NodeNetwork.camera_settings` exists per network; bodies inherit the top-level camera. Bodies don't currently get their own pan/zoom (since they're inline). If a future "focus body" mode lands, it might want body-local camera state. Defer.

7. **Direct-editing mode.** The "simplified UI focused on a single atom_edit node" mode doesn't interact with zones in any obvious way today. Document explicitly during U7 that direct-editing mode is unchanged and bypasses zone UI.

8. **AI text-format integration.** The parent design defers text-format syntax for inline bodies; the UI text editor (in `network_text_editor.dart`) will need extensions when that lands. Not blocked by this UI work.

## Reuse map (summary)

**Reused unchanged:**
- `NodeNetwork`, `Node`, `Wire` Rust types (Rust-side data model already complete per parent design)
- The four HOF nodes' Rust eval logic
- Pan/zoom infrastructure, focus management
- Bezier wire rendering, data-type color coding, dash patterns for alignment
- Drag-and-drop infrastructure (`Draggable` / `DragTarget`)
- Add Node popup (`AddNodePopup`), drag-aware filter logic
- Selection-rectangle drawing and overlap math (other than pin position resolution)
- Validation error rendering at the node level (red border + tooltip)
- Comment node widget (unaffected — comments don't bear zones)
- The current `NodeWidget` for non-HOF nodes — its body-region rendering is conditional on `node.zone != null`

**Reused with extensions:**
- `PinReference`: gains `scopeChain`, `pinKind`
- `WireView` / `WireIdentifier`: gain `source_pin`, `source_scope_depth`, `destination_argument_kind`
- `NodeView`: gains `zone: Option<ZoneView>` and recursive structure
- `StructureDesignerModel`: gains `activeScopeChain` and scope-parameterized mutation methods
- All mutation API functions: grow `scope_path: Vec<u64>` parameters
- `pinScreenPosition` (new central resolver replacing inline pin-position calls in painter, widget, and selection-rect code)
- Coordinate transforms: routed through `ScopeResolver`
- Hit-test functions: routed through `findNodeAtScreenPosition` / `findContainingScope`
- Undo commands: grow `scope_path` field; routed through body-aware snapshot/restore

**New from scratch:**
- `ScopeResolver` (Dart)
- `ZoneView` (Rust API type + Dart generated)
- `APISourcePin` / `APIArgumentKind` enums (API surface)
- `ZoneBodyLayer` painter (Dart) — body interior wire rendering
- `ZoneInputPinWidget` / `ZoneOutputPinWidget` (Dart) — inner-edge pin widgets
- Body region rendering inside `NodeWidget` (Dart) — the translucent container + recursive body
- Body resize handles + drag-coalescing undo command
- `set_zone_size` API + `SetZoneSizeCommand` undo
- Flutter `activeScopeChain` bookkeeping (Flutter-only; no Rust-side mirror)

**Dead weight to remove at migration time** (coordinated with parent doc's cleanup phase):
- `NODE_VERT_WIRE_OFFSET_FUNCTION_PIN` constant in `node_network.dart`
- Function-pin `PinWidget` in `node_widget.dart` title bar
- `pin_index == -1` arms in painter and pin-position math
- `PinKind.functionPin` enum value
- Wire-render code paths handling `pinIndex == -1`

---

### Phasing summary

| Phase | Outcome |
|---|---|
| U1 | Coordinate + pin-position math through `ScopeResolver` (no visible change) |
| U2 | Hit-test + mutation APIs accept scope chains (no visible change) |
| U3 | HOFs render with visible empty body region + inner pins (read-only bodies) |
| U4 | Body editing: add/move/delete/intra-body wires, body sizing, resize handles |
| U5 | Cross-scope wires: captures and zone-input usage |
| U6 | Nested-zone inline rendering and authoring |
| U7 | Polish: validation surface, focus mode, function-pin removal |

Each phase has the same exit gate: `cd rust && cargo test` green AND `flutter run` launches a working editor. The user-visible value lands progressively from U3 onward; U1-U2 are foundational refactors with no UI change.

---

### Deferred: body-node display in the 3D viewport (and the bare-`u64` invariant it would break)

None of U1–U7 let a body node's output render in the 3D viewport. Scene generation iterates only the **top-level** network's `displayed_nodes` (`StructureDesigner::refresh_full`), and the per-pin display toggle is intentionally inert for body scopes:

```rust
// structure_designer_api::toggle_output_pin_display
if !scope_path.is_empty() { return; }   // body paths accepted but no-op
```

Because of this, the Flutter eye icon is **hidden** for body-node output pins (`node_widget.dart` `_buildOutputPin`, gated on `scopeChain.isNotEmpty`) — it would otherwise be a dead control. Body nodes *do* render in the **node editor** and *do* get evaluated (as part of an enclosing top-level node's pass), so they still show a per-pin **hover value** (last evaluated value).

**The latent trap.** A whole family of structures is keyed by **bare `u64` node id** and is correct *only* because "body nodes are never displayed":

- `StructureDesignerScene.node_data: HashMap<u64, NodeSceneData>` and its `invisible_node_cache` (only displayed = top-level nodes ever inserted).
- `raytrace_per_node` / viewport-pick hit results (iterate `node_data`).
- `StructureDesignerChanges.visibility_changed: HashSet<u64>` (documented top-level-only).
- The selected-node gadget eval cache (`selected_node_eval_cache` + `get_node_eval_cache(node_id)`), a viewport feature.

Per-body `next_node_id` counters both start at 1, so a body node and a top-level node routinely share a numeric id. The moment body-node display becomes real, every structure above can conflate the two and must move to the scope-aware `NodeRef { scope_path, node_id }` key (the type in `structure_designer/node_network.rs`).

**Precedent already set.** The *evaluation-time* per-node maps had exactly this latency and were already migrated to `NodeRef`: `NetworkEvaluationContext`/`NodeSceneData`'s `node_output_strings` and `node_errors` (keyed via a `context.eval_scope_path` accumulator pushed on HOF-body and custom-network entry; read through `StructureDesignerScene::get_node_output_strings(scope_path, node_id)` / `get_node_error(...)`, which `build_node_view`/`build_zone_view` thread the scope path into). That fix is the template — the regression test `zones_test::hover_value_body_node_not_clobbered_by_colliding_top_level_node` documents the failure mode. Anyone implementing body-node display should apply the same `NodeRef` treatment to the display-keyed structures listed above.
