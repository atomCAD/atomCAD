# Node Network Editor - Agent Instructions

Interactive visual node graph editor widget. Handles rendering, interaction, and manipulation of the node DAG.

## Files

| File | Purpose |
|------|---------|
| `node_network.dart` | Main editor widget: pan/zoom, selection, wire dragging, keyboard shortcuts |
| `node_network_painter.dart` | Custom painter: grid, wires (Bezier curves), pin hit testing |
| `node_widget.dart` | Individual node rendering: pins, title, drag, context menu, HOF body region |
| `scope_resolver.dart` | Per-frame `ScopeResolver` + `LayoutCache` for scope-aware coordinates, hit testing, and pin-position resolution |
| `comment_node_widget.dart` | Special rendering for Comment nodes |
| `add_node_popup.dart` | Node type picker dialog with category filtering |

## Coordinate System

Two spaces:
- **Logical space:** Where node positions are stored (pan-invariant)
- **Screen space:** Rendered pixel coordinates
- Conversion: `screen = (logical + panOffset) * scale`

## Zoom Levels

Three discrete levels with different detail:
1. **Normal (1.0):** Full detail — pins, labels, subtitles
2. **Medium (0.6):** Simplified — title only, smaller pins
3. **Far (0.35):** Minimal — text only, no pins

## Interaction Model

- **Pan:** Middle mouse drag, or Shift+right-click drag
- **Zoom:** Mouse wheel (zoom-to-cursor)
- **Select node:** Click (Ctrl=toggle, Shift=add)
- **Rectangle select:** Click+drag on empty space
- **Wire creation:** Drag from pin → drop on compatible pin
- **Auto-connect:** Drop wire in empty space → opens `AddNodePopup` filtered by type
- **Keyboard:** Ctrl+C/X/V (copy/cut/paste), Del (delete), Ctrl+D (duplicate)

## Wire Rendering

Wires use cubic Bezier curves with data-type-based coloring:
- Selected wires get a glow effect
- Hit testing uses expanded area for easier clicking
- Pin positions calculated differently per zoom level

## Data Type Colors

| Type | Color Family |
|------|-------------|
| Bool/Int/Float | Warm orange |
| Vec2/Vec3/IVec | Cool blue |
| Geometry2D/Blueprint | Purple |
| Crystal/Molecule | Green |
| LatticeVecs/Motif/Structure | Teal/cyan |
| Functions | Amber |

`DATA_TYPE_COLORS` in `node_network.dart` matches by substring, so array types `[T]` pick up the base `T` color. The abstract types `HasAtoms`, `HasStructure`, and `HasFreeLinOps` have **no** entry in the color map. Instead, an input pin declared with an abstract type is rendered as a pie-sliced circle, one equal slice per concrete satisfier, colored with that concrete's color (see `ABSTRACT_TYPE_CONCRETES` and `_PinPainter`). Output pins are always concrete and render single-colored; wires color from the source's concrete type.

## Node Widget States

- **Active:** Thick border, full glow (0xFFD84315)
- **Selected:** Medium border, partial glow (0xFFE08000)
- **Error:** Red border with glow
- **Normal:** Blue border

## Constants (must match Rust `node_layout.rs`)

- `BASE_NODE_WIDTH = 160`
- `BASE_NODE_VERT_WIRE_OFFSET = 33`
- `BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM = 22`

## Zones (inline HOF bodies)

The four HOF nodes (`map`, `filter`, `fold`, `foreach`) render a **translucent body region** inside the node, between the external input and output columns (`_ZoneBodyRegion` in `node_widget.dart`). The body region carries the zone-input pins (inner-left) and zone-output pins (inner-right) plus a bottom-right resize handle. Body-internal nodes are *not* nested inside the HOF widget; they're added as siblings to the top-level `Stack` in `node_network.dart` via the recursive `_appendNodesRecursive` walk, then positioned via the `ScopeResolver` against their scope chain. This keeps the widget tree shallow and lets every node share the same pan/zoom transform.

### Scope chains

A node's **scope chain** is the `List<BigInt>` of HOF node ids identifying the body it lives in:
- `const []` — top-level network.
- `[hof_42]` — body of node `42` at the top level.
- `[hof_42, hof_99]` — body of node `99` inside the body of node `42`.

`PinReference.scopeChain` carries the scope chain of the pin's owner node; every mutation method on `StructureDesignerModel` takes an optional `scopeChain` parameter (default `const []`) that it forwards to the Rust API's `scope_path: Vec<u64>`. Wires carry `sourceScopeDepth` (how many ancestor frames up the source lives) and `destinationArgumentKind` (`External` for normal wires, `ZoneOutput` for body-return wires) so cross-scope wires resolve correctly.

### `ScopeResolver` and `LayoutCache`

`ScopeResolver` is constructed per frame from the current `NodeNetworkView`, `panOffset`, `scale`, and `zoomLevel`. Its constructor runs `runLayoutPass()`, which builds a `LayoutCache`:

1. **Bottom-up sizes** — for each HOF (recursively, deepest first) compute `bodySizes[chain] = max(stored, content_bbox + padding)`. Body content includes nested HOFs' *effective* sizes (cached from the previous step), so an inner body that grew past its stored size cascades into the outer body's content bbox.
2. **Top-down origins** — for each HOF (recursively, outer first) compute `bodyOrigins[chain]` = parent body's screen origin + the HOF's position-in-parent times scale.
3. **Collapse decisions** — bodies whose rendered screen-space height falls below `BODY_COLLAPSE_HEIGHT_THRESHOLD` (60 logical px) are added to `collapsedBodies`. A collapsed body renders a `[N nodes]` placeholder instead of its content; nested bodies inherit the collapse (an ancestor collapse subsumes its descendants).

`pinScreenPosition(PinReference)` reads from the cache in O(1) — wire rendering, hit testing, drag overlay, and selection-rect overlap all share the same coordinate authority. `findNodeAtScreenPosition` walks the scope tree returning the deepest containing node + its scope chain. `findContainingScope` does the equivalent for empty space (used by right-click → Add Node).

### Pin kinds

`PinReference.pinKind` is one of `externalInput`, `externalOutput`, `functionPin` (the title-bar `-1` output — "the whole node viewed as a function of all its inputs"; suppressed on HOFs by `node_widget.dart`'s title-bar conditional), `zoneInput` (inner-left source on an HOF body), `zoneOutput` (inner-right destination on an HOF body). Wire creation routes through `canConnectPins`/`connectPins` which compute `source_scope_depth = effectiveDestScope.length - source.scopeChain.length` where `effectiveDestScope = dest.scopeChain ++ [dest.nodeId]` for `ZoneOutput` destinations.

The `functionPin` is a **real, working function value** (`doc/design_function_pins.md`) — *not* a dead legacy pin. Drag it into an HOF's `f` pin or `apply.f` and Rust synthesizes a `Function` closure from the node and all its inputs. The drag flows through the generic `PinWidget`/`canConnectPins`/`connectPins` machinery with no functionPin-specific code: it's same-scope (`sourceScopeDepth == 0`), so it routes through `connectNodes` with `sourceOutputPinIndex: -1`, which Rust accepts/stores. Rust is the authority on the type match and the mutual-exclusion rule (a node's function pin and its input pins can't both be wired). When the pin is **consumed**, `NodeView.functionPinConsumed` is true and `_buildOutputPin` greys the node's output-pin eye(s) (tooltip → `apply`), mirroring the Rust scene-skip — derived, not stored, so disconnecting `f` restores the eye.

Note that closures' `Function`-typed pins (the `closure` node's output, the HOFs' optional `f` input, the `apply` node's `f` input) are **ordinary** `externalInput` / `externalOutput` pins that happen to carry a `Function` data type (amber color) — they are *not* the `functionPin` kind, even though they too carry `Function` values. Dragging a `closure` output into an `f` input flows through the normal pin-to-pin path; Rust's `can_be_converted_to` does the structural compatibility check. See `doc/design_closures.md`.

### Active scope chain

`StructureDesignerModel.activeScopeChain: List<BigInt>` is the body that keyboard shortcuts (Delete, Ctrl+C / V / X / D) operate on. Clicking on a body's interior, on a body-internal node, or right-clicking inside a body sets the active scope. Clicking on the top-level canvas resets it to `const []`. The Rust side does not mirror this — every mutation API receives `scope_path` explicitly from the call site.

### Selection rectangle

Selection rectangles are scope-confined: the scope is captured at pointer-down (whatever scope the drag started in) and the final node/wire overlap test is restricted to that scope's nodes. The rendered rectangle is also clipped to the body's screen rect (U7 polish) so it doesn't visually escape the body region.

### Closures: the `f`-pin override and shared shape editor

The four HOFs gained an optional `f: Function` input pin and there are two new function-value nodes: `closure` (zone-bearing, exposes its body as a `Function` output) and `apply` (bodyless, calls a `Function` once). The `closure` node's body **renders and edits for free** through the generic zone machinery above — it is `has_zone()`, so the body region, inner zone pins, recursive body nodes, resize handle, collapse, and hit-testing are all inherited with no closure-specific rendering code.

The one new rendering rule is the **inline-body / `f`-pin toggle**: when an HOF's `f` pin is wired, its inline body is ignored at eval, so the editor hides it. `ScopeResolver.runLayoutPass` collects every such body chain into `LayoutCache.functionOverriddenBodies` (Phase 4 of the layout pass, via `_isFunctionPinConnected`) and *also* adds it to `collapsedBodies`, so the existing body-skip checks in the node walk and painter hide the content with no extra conditions. The HOF then renders a distinct `_ZoneFunctionOverridePlaceholder` ("driven by `f`") instead of the `[N nodes]` collapse placeholder — `node_widget.dart` checks `ScopeResolver.isFunctionOverridden(chain)`. Only the four HOF types declare an `f` pin, so `closure` bodies are never flagged. Disconnecting `f` restores the inline body.

The shared shape property editor (`ClosureShapeEditor`) for both `closure` and `apply` lives on the `node_data` side — see `node_data/AGENTS.md`.

Design doc: `doc/design_zones_ui.md`, `doc/design_closures.md`.
