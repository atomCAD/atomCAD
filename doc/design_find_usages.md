# Design: Find Usages — "What is this called by?" navigation (issue #414)

**Issue:** https://github.com/atomCAD/atomCAD/issues/414 — "Add 'inverse
tabs' i.e. Add a 'What is this called by?' navigation option"
(mechadense).

Decoding the issue's terms:

- **"What is this called by?" / "dependents"** — the inverse of the
  existing *Go to Definition*: from a custom network (or an instance of
  it), navigate to the places that *use* it. In IDE terms: Find
  References / Find Usages.
- **"Inverse tabs"** — his UI sketch for the network-level variant: a
  strip of vertical tabs at the canvas edge, one per caller of the open
  network. "Inverse" because the focused context stays put and the
  *surroundings* swap when you jump outward.
- **His two quality goals** — minimal *visual jump* (land with the
  relevant node in a predictable place, ideally where you were already
  looking) and minimal *click friction* (right-click → menu item → pick
  target = 3 steps is too many; single-target jumps should be
  immediate).
- **"Super advanced maybe for later"** — viewing a definition network in
  the *actual* call context of a specific call site (real cached
  argument values instead of parameter defaults). Explicitly out of
  scope here; deserves its own issue.

## Motivation

Custom networks are atomCAD's functions. *Go to Definition* (context
menu on a custom node instance → `model.setActiveNodeNetwork(...)`,
`lib/structure_designer/node_network/node_widget.dart` ~1745) covers
inward navigation; there is no outward direction. Today, answering "who
calls this network?" means manually opening every network and scanning
for instances — the friction mechadense is pushing against.

## Current state (analysis)

**A usage is an instance node.** A custom network is referenced by nodes
whose `node_type_name` equals the network's name. This covers *all*
reference forms, including a network instance consumed as a function
value through its `-1` pin — that is still an instance node. Usages can
live inside HOF/closure zone bodies at any depth, so any collection walk
must be recursive (`walk_all_nodes` — see `rust/AGENTS.md` on the "bare
iteration skips body nodes" bug class).

**The reference walk already exists in boolean form.**
`StructureDesigner::check_delete_references`
(`structure_designer.rs` ~2032) walks every registry network with
`walk_all_nodes` to refuse deleting a referenced network. It reports
only host-network *names* — no scope path, no node id — and
`walk_all_nodes` (`node_network.rs:2289`) does not expose the chain, so
Find Usages needs a chain-tracking recursive walk (same shape as
`validate_zones_recursive`).

**Back/Forward already exists.**
`rust/src/structure_designer/navigation_history.rs` is a browser-style
stack of network *names* with a single recording point
(`set_active_node_network_name`, `structure_designer.rs` ~5032), so any
navigation routed through `setActiveNodeNetwork` is recorded for free.
`navigate_back/forward` restore the target network's per-network **3D
camera** (`NodeNetwork.camera_settings`) but nothing about the node
*canvas*: entries carry no focus node and no canvas pan/zoom.

**Canvas framing on network switch is recomputed, not restored — and
it is deferred.** `updatePanOffsetForCurrentNetwork` (`node_network.dart`
~768) frames the top-left-most node with a 20 px margin whenever the
active network changes. So even Back does not return you to where you
were looking — this is the "jarring visual jump" half of the issue, and
the existing history mechanism is one granularity notch too coarse to
fix it alone. Load-bearing detail: the framing is **not** synchronous
with the switch. It fires from *post-frame callbacks* — one scheduled
during `build` whenever `_currentNetworkName != nodeNetworkView.name`
(~1507), one from `initState` (~481) — and `_currentNetworkName` is the
only gate. Any pan set programmatically at switch time is silently
overwritten one frame later unless `_currentNetworkName` is updated (or
the callback otherwise suppressed) at the same moment. Both the
anchored jump (Phase 2) and the viewport restore (Phase 4) must
coordinate with this.

**Scroll-to-node exists, top-level only.** `_scrollToNode`
(`node_network.dart` ~500) centers a node by bare id via the
`model.onScrollToNode` callback (registered by `NodeNetworkState`); it
reads `nodeNetworkView.nodes[nodeId]`, so body nodes are unreachable.
Scoped selection exists: `model.setSelectedNode(nodeId, scopeChain)`.

**The context menu is a flat list** (`node_widget.dart` ~1661, ~10
items) but the divider + disabled-header grouping idiom is already used
(the "Body" collapse group), so grouping costs nothing new.

## Non-goals

- **No cached-call-context evaluation ("visual step tracing").**
  Separate future issue; the current evaluator shows a definition under
  parameter defaults and this design does not change that.
- **No global/cross-file scoping story.** Usages are bounded by the open
  `.cnnd` document. mechadense's tagging/namespace musings are
  explicitly deferred in the issue itself.
- **No literal vertical tab strip (v1).** The network-level entry point
  lives in the user-types panel ("Used by (n)"). If usage-hopping turns
  out to be a hot loop, a canvas-edge strip can be layered on top of the
  same API later.
- **No multi-column context menu.** Flutter's `showMenu` is
  single-column; grouped sections with headers deliver the same
  scanability.
- **No record-def usage search.** This design targets networks. Record
  defs already have their own delete-time reference check; surfacing
  their usages in the same UI is a natural follow-up, not part of v1.
- **No auto-expanding collapsed bodies on jump.** A usage inside a
  collapsed body is selected and its outermost visible container becomes
  the landing target (D6); collapse state is user intent and stays
  untouched.

## Design decisions

**D1 — Usage identity.** A usage is `(host_network_name, scope_path,
node_id)` — the same addressing triple the rest of the codebase uses
(`NodeRef` + network). Collection walks every registry network
recursively with a chain-tracking walk; clipboard contents are not
usages.

**D2 — Read-only backend API.** `get_network_usages(network_name)`
returns a list of usage views; it mutates nothing, so no undo command,
no refresh call, `#[frb(sync)]` like its siblings. Each view carries the
addressing triple plus display strings resolved Rust-side (host network
name, the instance node's display label, and a short body qualifier like
"in map body" when `scope_path` is non-empty) so Flutter renders without
re-deriving anything.

**D3 — Two entry points, one API.**
- *Node level (primary):* context menu on a custom node instance —
  **"Find Usages"** directly under "Go to Definition". Semantics: usages
  of the node's *type* (the classic IDE "symbol under cursor" reading).
  **The originating instance is excluded from the result set** — the
  clicked node is itself trivially a usage of its own type, so without
  this filter the "no usages" branch is unreachable, a sole usage means
  jumping to yourself, and the common one-other-caller case degrades
  into a two-item picker containing the node under the cursor. The
  filter is applied Flutter-side (drop the usage matching the active
  network + the clicked node's scope chain + node id) so the backend API
  stays generic; the panel counts stay unfiltered totals.
- *Network level:* on user-types panel rows — a "Find Usages" entry in
  the existing row context menu, plus a minimal trailing usage-count
  text (details in Phase 3; sidebar space is tight, so the count is
  the optional half). This is the panel-shaped answer to "who calls
  the network I'm editing".

**D4 — The jump, with anchored landing.** `jumpToUsage(usage)` on the
model: `setActiveNodeNetwork(host)` (history recorded for free by the
Rust setter) → `setSelectedNode(node_id, scopeChain)` → scoped
scroll-to-node. When the jump originates from a node's context menu,
the landing is **anchored**: the target instance node's *center* is
placed at the screen position the source node's center occupied at
invocation ("leave the node in the exact same place" — the issue's
core continuity ask). Center-to-center matching, not raw cursor
position, because source and target nodes differ in size (pin counts).
The zoom level is kept unchanged across the jump — for *every* usage
jump, anchored or not — for full visual continuity. When there is no
meaningful source position (the panel entry points, D3), the landing
falls back to centering the target in the viewport. Mechanically this is cheap: `_scrollToNode`'s pan
formula (`panOffset = screenPoint / scale − nodeAnchorLogical`,
`node_network.dart` ~500) already does the computation with
`screenPoint` hard-coded to viewport center; anchoring just makes
`screenPoint` a parameter. The anchor is captured once, at right-click
time (source node screen rect from its logical position + `getNodeSize`
+ current pan/scale, in the network widget's local coordinates), so a
jump made through the multi-usage picker (D5) still lands anchored.

**D5 — Single usage jumps immediately.** The 0/1/n branches run on the
entry point's result set — for the node-level entry that is the
*self-excluded* set from D3, so "one usage" means one *other* caller.
One usage → no picker, jump. Multiple → an anchored popup at the cursor
(a `showMenu`, not a modal dialog), one row per usage labeled
`host_network — node label` with the body qualifier, ordered by host
network name. This removes the "third step" in the common case, which
is mechadense's friction complaint. Zero → SnackBar, with entry-point-
specific wording because the sets differ: from the node context menu
**"No other usages of '<name>'"** (the clicked instance is a usage, just
a filtered-out one — "No usages" would be a lie); from the user-types
panel **"'<name>' is not used by any network"** (nothing was filtered,
the count really is zero).

**D6 — Scoped scroll.** `onScrollToNode` grows an optional `scopeChain`
parameter. For a body node the logical position is the accumulated body
origins along the chain plus the node's in-body position; the
`ScopeResolver`'s layout pass already computes these origins (screen
space — convert back via `screenToLogical`). If any ancestor body along
the chain is collapsed, the outermost top-level ancestor HOF becomes
the landing target instead, under the jump's normal landing rule
(anchored or centered, D4); the selection is still set, so opening the
body reveals it highlighted.

**D7 — Continuity via per-network canvas viewport, not history
widening.** The "land where you were" goal is served by persisting the
node-canvas viewport (pan offset + zoom level) *per network*, mirroring
the existing per-network `camera_settings` precedent: synced to Rust on
change, restored on any activation (Back/Forward included; the one
exception is a usage jump, which computes its own landing and takes
precedence — see Phase 4's precedence chain), serialized
in `.cnnd` with a serde-default so no migration is needed, and not
undo-tracked. This makes Back return you to the exact canvas view you
left — for *every* navigation path — while `NavigationHistory` stays a
simple name stack (no entry-struct widening, no changes to its
rename/remove maintenance). The top-left auto-framing remains the
fallback for networks without a stored viewport.

**D8 — Naming.** Menu item: **"Find Usages"** — two words, parallel to
"Go to Definition", familiar to anyone from IDEs. mechadense's "What is
this called by?" is friendlier but reads oddly as a *type-level* action
on an instance node (the node isn't called by anything; its type is).
The panel affordance uses his framing where it fits naturally: "Used
by (n)". Easy to bikeshed later; nothing structural depends on the
strings.

**D9 — Context menu grouping (his explicit suggestion, cheap).**
Restructure the node context menu into titled sections using the
existing header + divider idiom: *Navigate* (Go to Definition, Find
Usages), *Edit* (Duplicate, Copy, Cut), *Refactor* (Inline, Promote to
Parameter, Factor out, Convert to/from Closure), *Node* (Execute,
return-node toggle), then the existing *Body* group. Pure Flutter
reshuffle, no behavior change.

## Phases

### Phase 1 — Backend usage collection + API

- `StructureDesigner::network_usages(network_name) -> Vec<NetworkUsage>`
  (`NetworkUsage { host_network: String, scope_path: Vec<u64>, node_id:
  u64 }`), via a chain-tracking recursive walk over every registry
  network. Optionally refactor `check_delete_references` onto it —
  nice-to-have, not required, and **not a naive substitution**: that
  function takes a *set* of targets and exempts intra-set references
  (bulk namespace delete), so it needs a set-aware variant of the walk,
  not a per-network usage list per target.
- API: `get_network_usages(network_name) -> Vec<APINetworkUsage>` in
  `rust/src/api/structure_designer/` with the display fields from D2;
  plus `get_network_usage_counts() -> HashMap<String, u32>` (one walk
  producing all counts) for the panel badges, so the list view doesn't
  issue N calls per rebuild. Run `flutter_rust_bridge_codegen generate`.
- Tests (`rust/tests/structure_designer/find_usages_test.rs`):
  top-level usage; body usage at depth ≥ 2 (scope path correctness);
  zero usages; usages spread across several hosts; instance consumed as
  a function value via its `-1` pin still reported; counts map matches
  per-network queries.

### Phase 2 — Flutter: context-menu action + jump

- "Find Usages" `PopupMenuItem` under "Go to Definition"
  (`isCustomNode`-gated) in `node_widget.dart`.
- Handler: capture the landing anchor (source node's center in
  network-widget-local screen coordinates, per D4) **before** showing
  any menu; fetch usages; **drop the originating instance** (active
  network + clicked node's scope chain + node id, per D3); then on the
  filtered set: 0 → SnackBar "No other usages of '<name>'"; 1 → jump;
  2+ → anchored `showMenu` picker per D5, then jump with the originally
  captured anchor.
- `StructureDesignerModel.jumpToUsage(...)` per D4, threading the
  optional anchor through to the scroll callback.
- Scoped scroll per D6: extend `onScrollToNode` / `_scrollToNode` with
  the `scopeChain` and an optional `screenAnchor` parameter (default =
  viewport center, preserving existing callers' behavior). Usage jumps
  — anchored and centered alike — keep the current zoom level and
  compute the pan explicitly; the target network's stored viewport
  restore (Phase 4) must not run for them, which falls out of the
  gate-closing in the next bullet (see Phase 4's precedence chain — do
  not implement a separate skip).
- Sequencing — suppress the deferred auto-framing. The top-left framing
  runs from post-frame callbacks gated on `_currentNetworkName !=
  nodeNetworkView.name` (see Current state), so a pan applied in the
  jump handler survives the switch frame but is **overwritten one frame
  later** unless the gate is closed. The jump's pan application —
  anchored *and* centered alike, so this belongs in the shared scroll
  path — must therefore also set `_currentNetworkName` to the target
  network's name (equivalently: a one-shot "pan already resolved for
  this network" flag consumed by
  `updatePanOffsetForCurrentNetwork`). Note: the
  click-to-activate scroll path is *not* a precedent to copy —
  `_scrollToNode` there never crosses a network switch, so it never
  races the framing callback.
- Reference guide: add Find Usages next to the Go to Definition entry in
  `doc/reference_guide/ui.md`.
- Verification: manual walkthrough (thin editor UI — same policy as
  other navigation features): usage at top level, usage inside a body,
  usage inside a *collapsed* body, single-usage instant jump,
  multi-usage picker, an instance that is the *only* usage of its type
  shows the "No other usages" SnackBar (no self-jump), anchored landing
  (target node center lands where the source node center was, zoom
  unchanged, and the pan **stays put on subsequent frames** — the
  deferred framing must not overwrite it), panel-badge jump falls back
  to centering, Back returning to the origin network.

### Phase 3 — Network-level entry point ("Used by (n)")

Sidebar space is tight (dense `ListTile`s, long namespaced names), so
the row affordance is minimal:

- **Row context menu first:** add "Find Usages" to the existing
  right-click menu on network rows (Rename / Duplicate / Delete,
  `node_network_list_view.dart` ~117) — zero pixels, and the feature
  stays fully reachable even without the count. No self-exclusion here
  (there is no originating instance); zero usages → SnackBar "'<name>'
  is not used by any network" (D5).
- **Trailing count (optional half):** a small grey count text as the
  `ListTile.trailing` (just the number, no icon — the number *is* the
  information), rendered only when the count is > 0 (from the batched
  counts API), so zero-usage networks and record defs reserve no
  space. Tooltip "Used by n nodes"; clicking opens the usage picker
  anchored at the row.
- Both paths share the jump from Phase 2 (viewport-center landing —
  no source node to anchor on).
- Tree view rows get the context-menu entry too; the trailing count
  can follow there later if it proves useful in the list view.
- Reference guide: user-types panel section.

### Phase 4 — Continuity: per-network canvas viewport (D7)

- Rust: `NodeNetwork.canvas_viewport: Option<CanvasViewport { pan_x,
  pan_y, zoom_level }>`; scoped like `camera_settings` (top-level
  networks only — bodies scroll with their host). API getter/setter
  synced from Flutter (mirror the `sync_camera_to_active_network`
  pattern, including the dirty flag); serialized in `.cnnd` with
  `#[serde(default)]`; not undo-tracked.
- Flutter: implement the restore **inside**
  `updatePanOffsetForCurrentNetwork` — stored viewport when present,
  the existing top-left framing as the fallback branch. Putting it
  there (rather than in a separate switch hook) means it inherits the
  `_currentNetworkName` gate and the post-frame timing for free, and
  the anchored-jump suppression from Phase 2 (which closes that gate)
  automatically wins over the restore as well — one precedence chain:
  anchored jump > stored viewport > top-left framing. Sync on pan/zoom
  settle (end of gesture, not per-frame).
- Tests: `.cnnd` roundtrip of the viewport field; old files load with
  `None`. Restore behavior verified manually (Back/Forward now lands
  where you left).
- This phase is independent of Phases 1–3 and benefits all navigation;
  it can ship first if convenient.

### Phase 5 (optional) — Context menu grouping (D9)

- Reorganize `_handleContextMenu` items into the D9 sections. Verify all
  existing conditional items (canFactor, canConvertToClosure, …) keep
  their gating. Pure UI; manual walkthrough.

## Deferred / follow-ups

- Canvas-edge caller tabs ("inverse tabs" literal form) — same API,
  new surface, if usage-hopping proves hot.
- Record-def usages in the same UI.
- Cached call contexts / visual step tracing — new issue to be filed;
  interacts with the evaluator, far beyond navigation.
