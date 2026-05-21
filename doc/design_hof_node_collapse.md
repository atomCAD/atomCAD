# Collapsing Higher-Order Function Nodes

## Scope

This document designs a **per-node collapse mode** for the four higher-order
function (HOF) nodes — `map`, `filter`, `fold`, `foreach`. When an HOF is
*effectively collapsed* it hides its inline body region entirely and renders as a
compact, ordinary-looking node: just its title, its external input pins (`xs`,
`f`, and for `fold` the accumulator inputs) on the left, and its external output
pin on the right. When effectively expanded it renders exactly as today
(`doc/design_zones_ui.md`).

The motivation is the closures feature (`doc/design_closures.md`): users now
routinely drive an HOF by wiring a `closure` into its `f` pin, in which case the
HOF's own inline body is **ignored at eval time**. Today such an HOF still
occupies the full body-region footprint and shows a "body ignored — driven by
`f`" placeholder. Collapsing removes that dead space so a closure-driven pipeline
reads like a normal data-flow graph.

### The core idea: collapse tracks "is the body redundant?"

The reason to hide a body is that it is **redundant**, and an HOF body is
redundant exactly when its `f` pin is wired (eval ignores the inline body then).
So "should the body be hidden?" and "is `f` wired?" are the same question. The
effective collapse state is therefore **derived** from `f`-connection by default,
with an explicit per-node override for the minority of cases where the user wants
to deviate:

```
CollapseMode { Auto, Collapsed, Expanded }   // default Auto

effective_collapsed(node) =
    is_collapsable(node) && match node.collapse_mode {
        Auto      => f_pin_is_wired(node),   // the derived truth
        Collapsed => true,                   // override: always compact
        Expanded  => false,                  // override: always show body
    }
```

This makes the bad state from earlier drafts (a freshly dropped, empty HOF
rendering as an opaque red node whose unwired `result` pin is hidden inside the
collapsed body) **unrepresentable under the default**: a fresh HOF has no `f`, so
`Auto` resolves to expanded, and the user sees the empty body and its unwired pin.

In scope:
- A persisted, per-node `CollapseMode` enum, defaulting to **`Auto`** for all
  nodes.
- Rust-side resolution of `CollapseMode` → effective `collapsed` bool (Auto reads
  `f`-connection), exposed to Flutter alongside the raw mode.
- The compact rendering of an effectively-collapsed HOF.
- Authoring the override via a three-item group in the node's right-click context
  menu (no node-face icon, no dialog).
- The Rust data-model / API / serialization changes and the Flutter
  layout/hit-test/render changes.

Out of scope:
- The `closure` node is **never collapsable** — its body is where the closure is
  authored, so it always renders its body region. (`apply` has no zone, so it is
  unaffected.) `closure` has a zone but no `f` *input* pin, so it never satisfies
  the collapsable predicate.
- Any change to evaluation. Collapse is purely presentational — an effectively
  collapsed HOF with an inline body still evaluates that body; with `f` wired it
  still evaluates the wired closure. Validation is unchanged (an incomplete HOF
  still lights up red via the existing `node.error` path).

## Build/test contract

| Must pass | When |
|---|---|
| `cd rust && cargo test` green (accept new insta snapshots for the added serialized field) | Phase 1 |
| `flutter_rust_bridge_codegen generate` succeeds | Phase 1 |
| `flutter run` launches; Auto/Collapsed/Expanded selection works end-to-end; existing editing unaffected | Phase 2 |

## Concept

### What "collapsable" means

A node is **collapsable** iff it is one of the four HOFs. The clean structural
predicate is *"has a zone **and** declares an `f` input parameter"* — the four
HOFs declare `f`; the `closure` node is zone-bearing but has no `f` pin, so it is
not collapsable. Equivalently this is the fixed set of built-in type names
`{map, filter, fold, foreach}`. The implementation keeps one source of truth (a
small const name list) and treats the `f`-pin rule as its conceptual definition.

Only collapsable nodes ever resolve to collapsed; `closure` (and every non-HOF)
is always expanded. The context-menu override is offered only for collapsable
nodes, so `collapse_mode` on any other node stays at its `Auto` default and is
inert.

### Effective state resolution

`effective_collapsed` is computed **in Rust** (one place, `build_zone_view`) and
shipped to Flutter as a plain bool, so the UI never re-derives it (no two-source
drift). The raw `collapse_mode` is *also* shipped, but only so the context menu
can check-mark the current choice.

| `collapse_mode` | `f` wired? | effective | rendering |
|---|---|---|---|
| `Auto`     | no  | expanded  | body shown (or zoom `[N nodes]` placeholder if tiny) |
| `Auto`     | yes | collapsed | **compact** |
| `Collapsed`| any | collapsed | **compact** |
| `Expanded` | no  | expanded  | body shown |
| `Expanded` | yes | expanded  | body shown + "driven by `f`" placeholder |

Non-collapsable nodes (`closure`, every non-HOF): always expanded.

### The "never hide the cause of an error" property

Under the **default** (`Auto`), collapse coincides exactly with "the body is
irrelevant", so collapsing can never hide anything that matters:
- An `f`-wired HOF can only go red from `f`-side problems (a type-mismatched `f`
  wire, or a broken upstream `closure`) — all of which live *outside* the body
  and stay visible on the compact node.
- A body that could go red (zone-validation rule 1, an unwired `result` pin) only
  matters when `f` is **not** wired (the rule is suspended when `f` is connected)
  — and in that state `Auto` resolves to **expanded**.

So with the default, a collapsed HOF is never red-because-of-a-hidden-body. The
only way to reach "compact + red-from-hidden-body" is an explicit
`CollapseMode::Collapsed` on an incomplete inline body — a deliberate user
choice, and even then the red border **and the error tooltip still render on the
compact node**, so the error is never silent. We therefore do **not** special-
case-suppress `Collapsed` on error; no error state needs to flow into the
resolver.

### Compact vs. the two existing "hidden body" states

The zones/closures UI already hides body *content* in two situations, both via
`LayoutCache.collapsedBodies` (`scope_resolver.dart`), and both keep the HOF at
its full body-region **footprint** (they swap the body interior for a same-size
placeholder):
- **Zoom-level collapse** — a body too small to read renders a `[N nodes]`
  placeholder.
- **Function-override** — an `f`-wired HOF renders a "driven by `f`" placeholder.

The new **compact** case hides the body content **and shrinks the node's
footprint to a regular node**. It reuses the existing content/wire-skip machinery
(by joining `collapsedBodies`) but additionally changes size and pin-position
resolution.

Precedence: **compact wins.** An effectively-collapsed HOF renders as a plain
node regardless of zoom or `f`-connection — it shows neither placeholder. The
"driven by `f`" placeholder is therefore only seen in the one remaining
expanded-but-`f`-wired case: `CollapseMode::Expanded` with `f` connected (the
user explicitly asked to peek at a body that eval ignores).

### Default mode

Every node defaults to **`Auto`**:
- Newly created HOFs are `Auto` on creation → expanded until `f` is wired, then
  compact. No creation-time defaulting logic is needed (`Auto` is the natural
  unset value).
- An explicit `Collapsed` / `Expanded` the user picks is persisted and honored on
  reload.

## Data model (Rust)

### `CollapseMode` and `Node.collapse_mode`

Add the enum and a field to `Node` (`node_network.rs`, next to
`body_width`/`body_height`, ~line 474):

```rust
/// The user's choice for whether a collapsable HOF's inline body region is
/// shown. `Auto` (the default) derives the effective state from whether the
/// `f` pin is wired (compact when wired — the body is dead — expanded
/// otherwise); the two overrides force it. Meaningful only for collapsable
/// HOFs; inert (`Auto`) on every other node. See
/// `doc/design_hof_node_collapse.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CollapseMode {
    #[default]
    Auto,
    Collapsed,
    Expanded,
}

// On Node:
pub collapse_mode: CollapseMode,
```

### Collapsable predicate — one source of truth

```rust
/// Built-in HOF type names that are collapsable. Equivalent to "has a zone and
/// declares an `f` parameter"; kept as a name list so the load path (which only
/// has the type name) and the runtime path (which has the `NodeType`) agree.
pub const COLLAPSABLE_HOF_TYPE_NAMES: &[&str] = &["map", "filter", "fold", "foreach"];

pub fn collapsable_type_name(name: &str) -> bool {
    COLLAPSABLE_HOF_TYPE_NAMES.contains(&name)
}
```

### Resolving the effective collapsed bool

The Auto branch needs the "is `f` wired" predicate. That predicate already exists
as `function_input_pin_connected(node, node_type)` in `network_validator.rs:626`
(used by zone validation). **Move it to `node_network.rs` and make it `pub`** so
it lives in the core module with no layering back-edge, then add the resolver
beside it:

```rust
// node_network.rs — moved from network_validator.rs, now `pub`.
/// True if `node` has an input pin named `f` of `Function` type carrying at
/// least one incoming wire. The `closure` node has no `f` *input* pin, so this
/// is always false for it. See `doc/design_closures.md`.
pub fn function_input_pin_connected(node: &Node, node_type: &NodeType) -> bool { /* unchanged body */ }

/// Resolve a node's [`CollapseMode`] to the effective "body hidden + node
/// compact" bool. Always false for non-collapsable nodes (so a stray override
/// on a `closure` or a hand-edited file can never compact it).
pub fn resolve_body_collapsed(node: &Node, node_type: &NodeType) -> bool {
    if !collapsable_type_name(&node.node_type_name) {
        return false;
    }
    match node.collapse_mode {
        CollapseMode::Auto => function_input_pin_connected(node, node_type),
        CollapseMode::Collapsed => true,
        CollapseMode::Expanded => false,
    }
}
```

`network_validator.rs` updates its two call sites (lines 678, 701) to
`use crate::structure_designer::node_network::function_input_pin_connected;`.

### Threading the field through `Node` literals

Unlike the earlier draft, **nothing is injected into `ensure_zone_init`** — there
is no eager default to apply, because `Auto` *is* the default. This removes the
fragility of coupling a presentational default to a structural-repair routine
that runs repeatedly.

- **Fresh-construction sites** (`add_node`/`add_node_with_id` at
  `node_network.rs:1016/1052`, `promote_to_parameter.rs:122`,
  `selection_factoring.rs:503`) initialize `collapse_mode: CollapseMode::Auto` —
  the same way `body_width: DEFAULT_BODY_WIDTH` is seeded in the literal.
- **Copy sites** (`duplicate` ~`node_network.rs:949`, `node_network.rs:1853`,
  `selection_factoring.rs:451`) copy `collapse_mode` from the source node so a
  duplicate preserves the original's choice.

### Serialization

```rust
// node_networks_serialization.rs, in SerializableNode (~line 161):
#[serde(default)]
pub collapse_mode: CollapseMode,   // missing in older files → Auto
```

- `to_serializable` (~line 428): `collapse_mode: node.collapse_mode`.
- `from_serializable` (~line 481): `collapse_mode: serializable.collapse_mode`.

`#[serde(default)]` + `#[derive(Default)]` (Auto) means a missing field loads as
`Auto`, which resolves sensibly for any pre-existing node (compact iff `f` wired,
expanded otherwise) — no bespoke migration code. (This feature is days old on its
branch; no released files predate it, so the migration burden is nominal.)

Insta snapshots that include serialized nodes will gain the field — accept them
with `cargo insta review`. Update the `serialization/AGENTS.md` note that
documents `SerializableNode.body_width`/`body_height` to also mention
`collapse_mode`.

## API (Rust → Flutter)

### `ZoneView` gains `collapse_mode`, `collapsed`, `collapsable`

In `structure_designer_api_types.rs` (`ZoneView`, ~line 159):

```rust
/// The raw stored mode, for the context menu's check-mark. `APICollapseMode`
/// mirrors `CollapseMode`.
pub collapse_mode: APICollapseMode,
/// Effective "body hidden, node rendered compact" — already resolved Rust-side
/// (Auto reads `f`-connection). The renderer/layout reads only this.
pub collapsed: bool,
/// Whether this node type supports the override (true for the four HOFs, false
/// for `closure`). Gates the context-menu group.
pub collapsable: bool,
```

Define `APICollapseMode { Auto, Collapsed, Expanded }` in the API types.

Populate in `build_zone_view` (`structure_designer_api.rs:431`):

```rust
collapse_mode: node.collapse_mode.into(),               // CollapseMode → APICollapseMode
collapsed: resolve_body_collapsed(node, node_type),     // Auto already resolved
collapsable: collapsable_type_name(&node.node_type_name),
```

### `set_collapse_mode`

The API function is a thin wrapper; the mutation lives on `StructureDesigner` so
it can capture the before-state and push an undo command (per the undo
convention — commands are created inside `StructureDesigner`, not the API layer).

The *derived* part of collapse (Auto following `f`) needs **no command** — it
rides the `f` wire's own undo. Only an explicit mode change is a user action that
must be undoable.

```rust
// structure_designer_api.rs — thin wrapper, mirrors set_zone_size's call shape.
#[flutter_rust_bridge::frb(sync)]
pub fn set_collapse_mode(scope_path: Vec<u64>, hof_node_id: u64, mode: APICollapseMode) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .structure_designer
                .set_collapse_mode(&scope_path, hof_node_id, mode.into());
        });
    }
}
```

```rust
// structure_designer.rs — captures before-state, mutates, pushes the command.
pub fn set_collapse_mode(&mut self, scope_path: &[u64], hof_node_id: u64, mode: CollapseMode) {
    let network_name = match &self.active_node_network_name {
        Some(n) => n.clone(),
        None => return,
    };
    // Resolve the (possibly nested) body and read the old value, guarding so
    // only collapsable HOFs honor the mode.
    let old = {
        let Some(network) = self.get_scope_network_mut(scope_path) else { return; };
        let Some(node) = network.nodes.get_mut(&hof_node_id) else { return; };
        if !collapsable_type_name(&node.node_type_name) {
            return;
        }
        let old = node.collapse_mode;
        node.collapse_mode = mode;
        old
    };
    if old == mode {
        return; // no-op; don't push an empty command
    }
    self.push_command(SetCollapseModeCommand {
        network_name,
        scope_path: scope_path.to_vec(),
        node_id: hof_node_id,
        old_mode: old,
        new_mode: mode,
        description: "Set HOF collapse mode".to_string(),
    });
}
```

The standard scope-path dispatch makes this work for HOFs nested inside other
bodies. Undo is detailed in §"Undo".

## Flutter changes

### Generated `ZoneView`

After FRB regen, `ZoneView` carries `collapseMode`, `collapsed`, and
`collapsable`. A node is a collapsable HOF iff
`node.zone != null && node.zone.collapsable`; it is **effectively compact** iff
additionally `node.zone.collapsed`.

### `scope_resolver.dart`

- **`LayoutCache.compactBodies: List<List<BigInt>>`** plus an exact-match
  `isCompact(chain)` predicate, parallel to the existing
  `functionOverriddenBodies` / `collapsedBodies`. ("Compact" — not "manually
  collapsed" — because the trigger includes Auto-derived collapse, not only an
  explicit override.)
- **`runLayoutPass`** gains a phase: walk every body chain; for each whose owner
  node has `zone.collapsed == true`, add the chain to **both** `compactBodies`
  and `collapsedBodies`. Joining `collapsedBodies` gives, for free:
  - body-node skip in `_appendNodesRecursive` / `_appendZoneNodesRecursive`
    (`node_network.dart`, guarded by `isBodyCollapsed`),
  - body-wire skip in `_drawWiresInZone` (`node_network_painter.dart`) — already
    drops intra-body wires, zone-output wires, **and** capture wires (a capture's
    destination is a body node, so it lives in the body's `zone.wires`),
  - cascade: an inner body inside a compact outer body is hidden too, because
    `isBodyCollapsed` already returns true for any descendant of a collapsed
    chain.
- **Function-override phase** (existing): now also skip chains already in
  `compactBodies`. The "driven by `f`" placeholder must appear **only** when the
  body is expanded despite `f` being wired (the `CollapseMode::Expanded` + `f`
  case). Concretely the phase becomes:
  ```dart
  if (!_isFunctionPinConnected(chain)) continue;
  if (layout.isCompact(chain)) continue;   // compact wins — no placeholder
  layout.functionOverriddenBodies.add(List<BigInt>.from(chain));
  ```
- **`effectiveNodeSizeLogical(node, chain)`**: if the node is compact, return a
  compact size computed from the node's input/output pin counts with
  `width = NODE_WIDTH` (the regular-node formula) instead of the
  `BASE_HOF_BODY_LEFT_OFFSET + body.width + …` HOF formula. Because a compact
  child reports its compact size here, a parent body's `content_bbox` shrinks
  around it automatically (cascade in the size direction too).
- **`_pinPositionNormal`**: in the `externalOutput` and `functionPin` arms,
  compute `nodeWidth = NODE_WIDTH` when the owner node is compact (instead of the
  effective-body-width formula). `externalInput` already sits at `x = 0`. The
  `zoneInput`/`zoneOutput` arms are unreachable for a compact body (its pins
  aren't rendered and its wires are skipped) but should be left intact for the
  expanded case.
- **`_findNodeAt`** needs **two** guards — it both recurses into body nodes and
  then hit-tests nodes at the current scope, and each part takes a *different*
  predicate. Joining `collapsedBodies` does **not** cover hit-testing for free:
  only the render walks and `_drawWiresInZone` consult `isBodyCollapsed`;
  `_findNodeAt` does not.
  1. **Recursion guard (body-node skip).** The loop at the top of `_findNodeAt`
     recurses into every HOF's `zone.nodes` *unconditionally*, deepest-hit-wins.
     Without a guard, a compact HOF's hidden body nodes — whose cached
     origins/sizes still place them over the compact node's rect — are returned
     in preference to the HOF itself, defeating the whole feature. Skip the
     recursion whenever the body's content is hidden:
     ```dart
     final bodyChain = [...scopeChain, node.id];
     if (isBodyCollapsed(bodyChain)) continue;   // hidden content ⇒ not hittable
     final hit = _findNodeAt(zone.nodes.values, bodyChain, screenPos);
     ```
     Use **`isBodyCollapsed`** here, *not* `isCompact`: it is true for compact,
     zoom-collapse, **and** function-override — every state in which body nodes
     are not drawn — so hit-testing finally matches the render and wire walks
     (which already gate on `isBodyCollapsed`, `node_network_painter.dart`). This
     also closes a pre-existing latent bug: clicking a `[N nodes]` or "driven by
     `f`" placeholder could already select an invisible body node.
  2. **Body-region special-case gate.** Gate the
     `if (node.zone != null) { … bodyRect … }` block (which swallows clicks in
     body empty space and routes zone-pin hits) on **`!layout.isCompact(bodyChain)`**
     so a compact HOF falls through to `return (scopeChain, node)` and hit-tests
     as an ordinary node. Use **`isCompact`** here, *not* `isBodyCollapsed`: a
     zoom-collapsed or `f`-overridden HOF keeps its full body footprint and must
     still treat clicks in that region as body empty-space / zone-pin hits — only
     a *compact* HOF has no body region.

  This gating depends on the compact size landing first (`effectiveNodeSizeScreen`
  must report the shrunk rect — see `effectiveNodeSizeLogical` above) so the
  `nodeRect` tested in part 2 is the compact one.
- **`_findContainingScopeIn`**: skip bodies where `isCompact(bodyChain)` — the
  body's cached origin/size still exist, but nothing is drawn there, so the body
  must not claim containment of clicks on the compact node.

### `node_network.dart` — `getNodeSize`

`getNodeSize(node, zoomLevel)` is a free function with the node in hand, so it
reads the resolved state directly from the view: when
`node.zone != null && node.zone.collapsable && node.zone.collapsed`, treat the
node as regular — `baseWidth = BASE_NODE_WIDTH`, and drop the zone-pin and
`storedHeight` contributions to `mainBodyHeight` (so height is driven by the
external input/output pins). This keeps the compact footprint consistent across
both zoom paths and matches `effectiveNodeSizeLogical`.

### `node_widget.dart` — compact body, no title-bar toggle

- **Body branch.** In `_buildNormalNodeContent`, choose the body builder by the
  resolved state:
  ```dart
  final bool compactHof = isHof && node.zone!.collapsable && node.zone!.collapsed;
  // …
  if (compactHof)
    _buildRegularMainBody(context)   // input column (xs, f, …) + output column
  else if (isHof)
    _buildHofMainBody(context, resolver)
  else
    _buildRegularMainBody(context),
  ```
  `_buildRegularMainBody` already renders the node's external input pins (which
  include `f`, an ordinary `parameters` entry) and output pins, so the compact
  HOF gets correct, fully interactive pins with no new pin code. `_buildHofMainBody`
  (and its placeholders) is now only reached for expanded HOFs.
- **No title-bar toggle, no node-face icon.** The title-bar Row is unchanged (it
  still renders the legacy function pin only for non-HOFs). The override is
  authored entirely from the context menu (below), keeping the node face clean.

### `node_widget.dart` — context-menu group

The node context menu is a flat `showMenu` with string-`value` dispatch
(`_handleContextMenu`, ~line 1252). Add a gated three-item radio group with a
divider and a dim header; the check-mark sits on the current `collapseMode`, and
selecting `Auto` is the "stop overriding" path — no dialog, no submenu (the flat
`showMenu` API has no native cascade, and a dialog is too heavy for view state).

```dart
final bool isCollapsableHof = node.zone != null && node.zone!.collapsable;
// … inside the items: [ … ]:
if (isCollapsableHof) ...[
  const PopupMenuDivider(),
  const PopupMenuItem(enabled: false, child: Text('Body')),
  _collapseModeItem('collapse_auto',      'Auto (follow f)',  node.zone!.collapseMode),
  _collapseModeItem('collapse_expanded',  'Always expanded',  node.zone!.collapseMode),
  _collapseModeItem('collapse_collapsed', 'Always collapsed', node.zone!.collapseMode),
],
```

```dart
PopupMenuItem _collapseModeItem(String value, String label, APICollapseMode current) {
  final bool active = _valueMatchesMode(value, current);
  return PopupMenuItem(
    value: value,
    child: Row(mainAxisSize: MainAxisSize.min, children: [
      Icon(Icons.check, size: 16, color: active ? null : Colors.transparent),
      const SizedBox(width: 8),
      Text(label),
    ]),
  );
}
```

Dispatch in the existing `.then((value) { … })` chain:

```dart
} else if (value == 'collapse_auto') {
  model.setCollapseMode(scopeChain, node.id, APICollapseMode.auto);
} else if (value == 'collapse_expanded') {
  model.setCollapseMode(scopeChain, node.id, APICollapseMode.expanded);
} else if (value == 'collapse_collapsed') {
  model.setCollapseMode(scopeChain, node.id, APICollapseMode.collapsed);
}
```

### `structure_designer_model.dart`

Add the mutation method, following the existing `setZoneSize` shape (forwarding
`scopeChain` as `scope_path`):

```dart
void setCollapseMode(List<BigInt> scopeChain, BigInt nodeId, APICollapseMode mode) {
  sd_api.setCollapseMode(
    scopePath: scopeChain.map((e) => e.toInt()).toList(),
    hofNodeId: nodeId.toInt(),
    mode: mode,
  );
  refreshFromKernel();
  notifyListeners();
}
```

## Undo

An explicit mode change is **undoable** — it is a discrete, user-visible state
change and belongs in the global undo stack. (The Auto-derived part is not a
stored choice and needs no command; it follows the `f` wire, whose connect/
disconnect is already undoable. The sibling `set_zone_size` resize is currently
*not* undoable; that is a known gap to be fixed separately and is **not** a
precedent to follow here.)

Add `commands/set_collapse_mode.rs`, modeled on `SetNodeDataCommand` (the
existing scope-aware command):

```rust
use crate::structure_designer::node_network::CollapseMode;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Undo/redo for changing an HOF node's collapse mode. `scope_path` identifies
/// the body the HOF lives in (empty = top-level `network_name`), resolved via
/// `ctx.network_in_scope_mut` like `SetNodeDataCommand`.
#[derive(Debug)]
pub struct SetCollapseModeCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub old_mode: CollapseMode,
    pub new_mode: CollapseMode,
    pub description: String,
}

impl SetCollapseModeCommand {
    fn apply(&self, ctx: &mut UndoContext, mode: CollapseMode) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path) {
            if let Some(node) = network.nodes.get_mut(&self.node_id) {
                node.collapse_mode = mode;
            }
        }
    }
}

impl UndoCommand for SetCollapseModeCommand {
    fn description(&self) -> &str { &self.description }
    fn undo(&self, ctx: &mut UndoContext) { self.apply(ctx, self.old_mode); }
    fn redo(&self, ctx: &mut UndoContext) { self.apply(ctx, self.new_mode); }

    /// Collapse is presentational — no re-evaluation is needed, only a fresh
    /// view. `Lightweight` (same as node-move) updates the view without
    /// re-running the network.
    fn refresh_mode(&self) -> UndoRefreshMode { UndoRefreshMode::Lightweight }
}
```

Register it with `pub mod set_collapse_mode;` in `commands/mod.rs`. The command
is pushed by `StructureDesigner::set_collapse_mode` (shown in §"API"), which also
no-ops when the value is unchanged so a redundant selection never lands an empty
entry on the stack.

## Interactions & edge cases

- **Fresh HOF (Auto, no `f`, empty body).** Resolves to **expanded**: the user
  sees the empty body region and its unwired `result` pin. The node is still red
  (zone-validation rule 1) but the cause is visible and fixable — author the body
  or wire `f`. This is the case the earlier "default collapsed" draft got wrong.
- **`f` wired (Auto)** — the common closure case: resolves to **compact**, no
  placeholder, and **not** red (rule 1 is suspended when `f` is connected).
- **Inline body authored (Auto, no `f`)**: resolves to **expanded** — the body is
  the node's logic, so it stays visible.
- **`CollapseMode::Expanded` + `f` wired**: body shown with the existing "driven
  by `f`" placeholder, confirming the body is ignored (the explicit "let me peek"
  state).
- **`CollapseMode::Collapsed`** (e.g. tucking away a finished inline `fold`):
  compact regardless of `f`. If the body happens to be incomplete the compact
  node still shows its red border and error tooltip — collapse is the user's
  explicit choice and the error is not silenced.
- **Nested HOFs.** A compact outer HOF hides everything inside (cascade via
  `collapsedBodies`). A compact inner HOF inside an expanded outer body renders
  compact in place, and the outer body shrinks around it (size cascade in
  `effectiveNodeSizeLogical`). Both follow from the existing cascades.
- **Zoomed-out rendering** is unaffected beyond `getNodeSize` returning the
  compact size; nodes are already title-only at far zoom.

## Implementation phases

### Phase 1 — Rust data, resolution, serialization, API

- Add `CollapseMode` + `Node.collapse_mode`; thread it through all `Node { … }`
  literals (`Auto` at fresh-construction sites, copy at copy sites). No
  `ensure_zone_init` change.
- Add `COLLAPSABLE_HOF_TYPE_NAMES` / `collapsable_type_name`; move
  `function_input_pin_connected` into `node_network.rs` (now `pub`, update
  `network_validator.rs` references); add `resolve_body_collapsed`.
- Add `SerializableNode.collapse_mode: CollapseMode` with `#[serde(default)]`.
- Add `APICollapseMode` and `ZoneView.collapse_mode` / `collapsed` /
  `collapsable`; populate them in `build_zone_view`.
- Add `commands/set_collapse_mode.rs` (`SetCollapseModeCommand`, registered in
  `commands/mod.rs`) and the `StructureDesigner::set_collapse_mode` method that
  mutates + pushes it; add the thin `set_collapse_mode` API wrapper;
  `flutter_rust_bridge_codegen generate`.
- **Tests** (`rust/tests/structure_designer/`): a fresh `map`/`filter`/`fold`/
  `foreach` has `collapse_mode == Auto`; `resolve_body_collapsed` returns false
  for Auto with `f` disconnected, true for Auto with `f` connected, true for
  `Collapsed`, false for `Expanded`, and false for `closure` regardless of mode;
  `set_collapse_mode` flips the mode (and is ignored for `closure`); a
  serialize→deserialize round-trip preserves an explicit mode; a deserialize of a
  node fixture **without** the field yields `Auto`. In `undo_test.rs`: changing
  the mode then undo/redo restores the prior `collapse_mode`, including for an HOF
  nested in a body (non-empty `scope_path`). `cargo test` green; accept new
  snapshots.

### Phase 2 — Flutter rendering & interaction

- `scope_resolver.dart`: `compactBodies` + `isCompact`, the new `runLayoutPass`
  phase, the function-override phase `isCompact` skip, the size / pin-position
  gating, and the **two-part `_findNodeAt` gating** (recursion guard on
  `isBodyCollapsed`, body-region special-case on `!isCompact`) plus the
  `_findContainingScopeIn` `isCompact` skip above.
- `node_network.dart`: `getNodeSize` compact branch.
- `node_widget.dart`: compact body branch (no title-bar toggle) + the gated
  three-item context-menu group and its dispatch.
- `structure_designer_model.dart`: `setCollapseMode`.
- **Verification** (manual `flutter run`, per the thin-editor-UI convention): a
  new `map` appears **expanded** with its empty body; wire a `closure` into its
  `f` pin and confirm it goes compact and evaluates; disconnect `f` and confirm it
  expands again; right-click → set "Always collapsed" and confirm it stays
  compact with `f` disconnected; set "Always expanded" with `f` wired and confirm
  the "driven by `f`" placeholder shows; set "Auto" to return to derived
  behavior; confirm a `closure` node shows no Body group and always renders its
  body; confirm a nested compact HOF hides its content and the outer body shrinks
  around it. **Hit-test check:** click a compact HOF across its whole width —
  including the right portion, over where its (now-hidden) body nodes used to sit
  — and confirm the click selects/right-clicks the HOF itself, never an invisible
  body node.

## Reuse map

**Reused unchanged:** the `collapsedBodies` content/wire-skip + cascade in
`_appendNodesRecursive` / `_appendZoneNodesRecursive` / `_drawWiresInZone`; the
scope-path mutation dispatch (`get_scope_network_mut`); `_buildRegularMainBody`
and all pin widgets; the `set_zone_size` pattern; the `node.error` red-border /
tooltip path; the existing `_isFunctionPinConnected` / `functionOverriddenBodies`
detection.

**Reused with extensions:** `Node` (+`collapse_mode`); `SerializableNode`
(+`collapse_mode` with serde default); `function_input_pin_connected` (moved to
`node_network.rs`, made `pub`); `ZoneView` (+`collapse_mode`/`collapsed`/
`collapsable`); `build_zone_view`; the undo system (+`SetCollapseModeCommand`, via
the established scope-aware `network_in_scope_mut` pattern); `ScopeResolver`
(+`compactBodies`/`isCompact`, function-override `isCompact` skip, size/pin/hit-
test gating); `getNodeSize`; `_buildNormalNodeContent` (compact body branch);
`_handleContextMenu` (Body radio group); `StructureDesignerModel`
(+`setCollapseMode`).

**New from scratch:** `CollapseMode` + `APICollapseMode`;
`COLLAPSABLE_HOF_TYPE_NAMES` / `collapsable_type_name`; `resolve_body_collapsed`;
`set_collapse_mode` API + `StructureDesigner` method; `SetCollapseModeCommand`;
`LayoutCache.compactBodies` + `isCompact`; the context-menu Body radio group.
