import 'package:flutter/material.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart'
    show PIN_HIT_AREA_WIDTH, PIN_HIT_AREA_HEIGHT;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Per-frame caches keyed by full scope chain terminating at an HOF node id
/// (`chain ++ [hof_id]`). Populated by [ScopeResolver.runLayoutPass]; every
/// pin-position query reads from these maps in O(1).
///
/// U6 extends the cache with [collapsedBodies] so a body whose rendered
/// height falls below [BODY_COLLAPSE_HEIGHT_THRESHOLD] (a tiny nested body
/// at far zoom, typically) can be skipped from the widget tree and the
/// painter — its HOF still renders, but its content is hidden. See
/// `doc/design_zones_ui.md` §"Zoom levels".
class LayoutCache {
  /// HOF body size in logical pixels, keyed by full scope chain terminating
  /// at the HOF id.
  final Map<List<BigInt>, Size> bodySizes = {};

  /// Body inner-top-left in screen coordinates, same keying. Folds in
  /// `panOffset` and `scale`.
  final Map<List<BigInt>, Offset> bodyOrigins = {};

  /// Scope chains whose body renders too small to be readable — its body
  /// content should be hidden and the HOF widget should render a simplified
  /// placeholder. A scope is collapsed if its body's screen-space height is
  /// under [BODY_COLLAPSE_HEIGHT_THRESHOLD] OR any ancestor is collapsed
  /// (a collapsed outer body subsumes its inner bodies).
  final List<List<BigInt>> collapsedBodies = [];

  /// Scope chains whose owner HOF has its `f` (function) input pin wired, so
  /// the inline body is ignored at eval time (the wired closure drives it
  /// instead). Such bodies are hidden — their content is skipped and the HOF
  /// renders a "driven by `f`" placeholder. These chains are *also* added to
  /// [collapsedBodies] so the existing body-skip checks in the node walk and
  /// painter hide the content with no extra conditions. See
  /// `doc/design_closures.md` §"Editor (Flutter) changes" item 1.
  final List<List<BigInt>> functionOverriddenBodies = [];

  /// Scope chains whose owner HOF resolves to *compact* — its body region is
  /// hidden AND the node shrinks to a regular-node footprint. Driven by the
  /// Rust-resolved `zone.collapsed` bool (which already folds in the
  /// Auto/Collapsed/Expanded mode and `f`-connection). Unlike
  /// [collapsedBodies] / [functionOverriddenBodies], which keep the full body
  /// footprint and swap in a same-size placeholder, a compact HOF renders no
  /// body region at all. These chains are *also* added to [collapsedBodies] so
  /// the existing content/wire-skip + cascade machinery hides the interior for
  /// free. "Compact" — not "manually collapsed" — because the trigger includes
  /// Auto-derived collapse, not only an explicit override. See
  /// `doc/design_hof_node_collapse.md`.
  final List<List<BigInt>> compactBodies = [];

  Size? lookupSize(List<BigInt> bodyChain) {
    for (final entry in bodySizes.entries) {
      if (_listEq(entry.key, bodyChain)) return entry.value;
    }
    return null;
  }

  Offset? lookupOrigin(List<BigInt> bodyChain) {
    for (final entry in bodyOrigins.entries) {
      if (_listEq(entry.key, bodyChain)) return entry.value;
    }
    return null;
  }

  bool isCollapsed(List<BigInt> bodyChain) {
    for (final entry in collapsedBodies) {
      if (_listEq(entry, bodyChain)) return true;
    }
    return false;
  }

  bool isFunctionOverridden(List<BigInt> bodyChain) {
    for (final entry in functionOverriddenBodies) {
      if (_listEq(entry, bodyChain)) return true;
    }
    return false;
  }

  bool isCompact(List<BigInt> bodyChain) {
    for (final entry in compactBodies) {
      if (_listEq(entry, bodyChain)) return true;
    }
    return false;
  }

  static bool _listEq(List<BigInt> a, List<BigInt> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
}

/// A body whose rendered screen-space height falls below this threshold is
/// collapsed: its content is hidden and the HOF renders a placeholder. The
/// threshold is set so an HOF can still receive captures (its outer chrome
/// and pins remain interactive) at any zoom while bodies that would be
/// unreadably small disappear. See `doc/design_zones_ui.md` §"Zoom levels".
const double BODY_COLLAPSE_HEIGHT_THRESHOLD = 60.0;

/// Resolves scope-aware coordinate and pin-position queries for the node
/// network editor. Created fresh per frame from the current [root] view,
/// [panOffset], [scale], and [zoomLevel].
///
/// Phase U3 brings two capabilities online:
/// 1. The [LayoutCache] (built by [runLayoutPass]) that all pin-position
///    queries read from. Today's bodies have no rendered content so the cache
///    is trivial — `bodySize == storedSize`. U4 extends the bottom-up walk
///    with `content_bbox`.
/// 2. [PinKind.zoneInput] / [PinKind.zoneOutput] handling in
///    [tryPinScreenPosition] so wires that touch an HOF's inner-edge pins
///    can render.
///
/// See `doc/design_zones_ui.md` §"Layout pass" and §"Pin position resolution".
class ScopeResolver {
  final NodeNetworkView root;
  final Offset panOffset;
  final double scale;
  final ZoomLevel zoomLevel;
  final LayoutCache layout = LayoutCache();

  ScopeResolver({
    required this.root,
    required this.panOffset,
    required this.scale,
    required this.zoomLevel,
  }) {
    runLayoutPass();
  }

  /// Populate [layout] from the current `root`, transform, and live node
  /// positions. Must be called before any pin-position or hit-test query.
  /// O(N) in the total number of nodes in the network.
  ///
  /// Two phases:
  /// 1. **Bottom-up sizes.** For each HOF reached, recurse into its body
  ///    first so inner body sizes are known on return. Compute `content_bbox`
  ///    as the union of every body node's rect in body-local coordinates.
  ///    Then `bodySizes[chain] = max(stored, content_bbox + padding)`.
  /// 2. **Top-down origins.** Walk down the tree using known body sizes to
  ///    place each body's screen-space origin. The top-level frame's origin
  ///    is computed via `logicalToScreen`; nested bodies' origins are the
  ///    parent body's origin plus the HOF's position-in-parent times scale.
  ///
  /// See `doc/design_zones_ui.md` §"Layout pass — bottom-up sizes, top-down
  /// origins".
  void runLayoutPass() {
    layout.bodySizes.clear();
    layout.bodyOrigins.clear();
    layout.collapsedBodies.clear();
    layout.functionOverriddenBodies.clear();
    layout.compactBodies.clear();
    // Phase 1: bottom-up sizes.
    for (final node in root.nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      _computeBodySize(zone, [node.id]);
    }
    // Phase 2: top-down origins, starting from the top-level scope.
    for (final node in root.nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      final chain = [node.id];
      final hofPos = apiVec2ToOffset(node.position);
      final bodyTopLeftLogical = hofPos +
          Offset(hofBodyLeftOffset(node), BASE_HOF_BODY_TOP_OFFSET);
      final origin = logicalToScreen(bodyTopLeftLogical, panOffset, scale);
      layout.bodyOrigins[chain] = origin;
      _placeChildBodies(zone, chain, origin);
    }
    // Phase 3: collapse decisions, top-down. A body is collapsed if its
    // screen-space rendered height is below the readability threshold OR
    // any ancestor body is collapsed. Computed after sizes are known so the
    // cascade is correct.
    for (final entry in layout.bodySizes.entries) {
      final chain = entry.key;
      final size = entry.value;
      if (size.height * scale < BODY_COLLAPSE_HEIGHT_THRESHOLD) {
        layout.collapsedBodies.add(List<BigInt>.from(chain));
      }
    }
    // Phase 4: compact resolution. An HOF whose Rust-resolved `zone.collapsed`
    // is true renders compact — body hidden AND footprint shrunk to a regular
    // node. Record it in `compactBodies` (drives the size / pin-position /
    // hit-test gating) and *also* in `collapsedBodies` so the existing
    // content/wire-skip + cascade hides the body interior with no extra
    // conditions. `zone.collapsed` already folds in the Auto/Collapsed/
    // Expanded mode and the `f`-connection (resolved in Rust), so it is the
    // single source of truth here. See `doc/design_hof_node_collapse.md`.
    for (final chain in layout.bodySizes.keys) {
      final hofId = chain.last;
      final parentChain = chain.sublist(0, chain.length - 1);
      final zone = _resolveNode(parentChain, hofId)?.zone;
      if (zone == null || !zone.collapsed) continue;
      layout.compactBodies.add(List<BigInt>.from(chain));
      if (!layout.isCollapsed(chain)) {
        layout.collapsedBodies.add(List<BigInt>.from(chain));
      }
    }
    // Phase 5: function-pin override. An HOF whose `f` input pin is wired
    // ignores its inline body (the wired closure drives it), so hide the
    // body's content — treat it like a collapse so the node walk and painter
    // skip it — and flag it separately so the HOF can render a distinct
    // "driven by `f`" placeholder. Only the four HOF types declare an `f`
    // pin, so `closure` bodies are never flagged here. **Compact wins:** a
    // compact HOF shows no body region at all, so it never gets the "driven
    // by `f`" placeholder — skip chains already marked compact.
    for (final chain in layout.bodySizes.keys) {
      if (!_isFunctionPinConnected(chain)) continue;
      if (layout.isCompact(chain)) continue;
      layout.functionOverriddenBodies.add(List<BigInt>.from(chain));
      if (!layout.isCollapsed(chain)) {
        layout.collapsedBodies.add(List<BigInt>.from(chain));
      }
    }
  }

  /// Wires of the network that directly contains the body identified by
  /// [scopeChain] (the network the HOF at the chain's tail lives in). Returns
  /// `root.wires` for the top level; walks into nested bodies otherwise.
  List<WireView> _containingWires(List<BigInt> scopeChain) {
    if (scopeChain.isEmpty) return root.wires;
    Map<BigInt, NodeView> nodes = root.nodes;
    ZoneView? zone;
    for (final hofId in scopeChain) {
      zone = nodes[hofId]?.zone;
      if (zone == null) return const [];
      nodes = zone.nodes;
    }
    return zone!.wires;
  }

  /// True when the HOF owning the body at [bodyChain] (`[...parent, hofId]`)
  /// has its `f` (function) input pin wired. The `f` pin is an ordinary
  /// external input, so a wire to it lives in the HOF's containing-network
  /// wires regardless of the source's scope depth.
  bool _isFunctionPinConnected(List<BigInt> bodyChain) {
    if (bodyChain.isEmpty) return false;
    final hofId = bodyChain.last;
    final parentChain = bodyChain.sublist(0, bodyChain.length - 1);
    final hof = _resolveNode(parentChain, hofId);
    if (hof == null) return false;
    int fIndex = -1;
    for (int i = 0; i < hof.inputPins.length; i++) {
      if (hof.inputPins[i].name == 'f') {
        fIndex = i;
        break;
      }
    }
    if (fIndex < 0) return false;
    final fParam = BigInt.from(fIndex);
    for (final wire in _containingWires(parentChain)) {
      if (wire.destNodeId == hofId &&
          wire.destParamIndex == fParam &&
          wire.destinationArgumentKind == APIArgumentKind.external_) {
        return true;
      }
    }
    return false;
  }

  /// Effective body size for [node]'s zone, in logical pixels. Reads from the
  /// layout cache (populated bottom-up by [runLayoutPass]) so an inner body
  /// that grew past its stored size cascades into the outer body's content
  /// bbox. Returns [Size.zero] for non-HOF nodes; falls back to stored size
  /// when the cache hasn't been populated for the chain (e.g. the cache
  /// hasn't been run, or the chain points at a node that was just deleted).
  Size effectiveBodySize(NodeView node, List<BigInt> nodeScopeChain) {
    final zone = node.zone;
    if (zone == null) return Size.zero;
    final chain = [...nodeScopeChain, node.id];
    return layout.lookupSize(chain) ?? Size(zone.storedWidth, zone.storedHeight);
  }

  /// HOF widget footprint at [nodeScopeChain] in logical pixels (pre-scale).
  /// For non-HOF nodes, returns `getNodeSize() / scale`. For HOF nodes, uses
  /// the cached effective body size so the outer container resizes when an
  /// inner body grows — required for the nested-zone cascade. See
  /// `doc/design_zones_ui.md` §"Body sizing — Layout pass".
  Size effectiveNodeSizeLogical(NodeView node, List<BigInt> nodeScopeChain) {
    // Comment nodes carry their own footprint (not the generic pin-derived
    // node size), so a body's content bbox grows to wrap them. Mirrors the
    // comment special-case in the find-node hit-test path below.
    if (node.nodeTypeName == 'Comment') {
      return Size(node.commentWidth ?? 200.0, node.commentHeight ?? 100.0);
    }
    final zone = node.zone;
    // A compact HOF reports a regular-node footprint so a parent body's
    // `content_bbox` shrinks around it (size cascade). Read `zone.collapsed`
    // directly from the view — not `layout.isCompact`, which isn't populated
    // until after the bottom-up size pass that calls this. `getNodeSize`
    // applies the same compact branch, so the two stay consistent.
    if (zone == null || (zone.collapsable && zone.collapsed)) {
      final s = getNodeSize(node, zoomLevel);
      return Size(s.width / scale, s.height / scale);
    }
    final body = effectiveBodySize(node, nodeScopeChain);
    final width =
        hofBodyLeftOffset(node) + body.width + hofBodyRightGutter(node);
    const titleHeight = 30.0;
    final inputPinsHeight =
        node.inputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM;
    final outputPinsHeight =
        node.outputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM;
    final zoneInputPinsHeight =
        zone.zoneInputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM;
    final zoneOutputPinsHeight =
        zone.zoneOutputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM;
    const minOutputHeight = 25.0;
    final mainBodyHeight = [
      inputPinsHeight.toDouble(),
      outputPinsHeight.toDouble(),
      zoneInputPinsHeight.toDouble(),
      zoneOutputPinsHeight.toDouble(),
      body.height,
      minOutputHeight,
    ].reduce((a, b) => a > b ? a : b);
    final subtitleHeight =
        (node.subtitle != null && node.subtitle!.isNotEmpty) ? 20.0 : 0.0;
    const padding = 8.0;
    return Size(width, titleHeight + mainBodyHeight + subtitleHeight + padding);
  }

  /// Screen-space variant of [effectiveNodeSizeLogical]. Used by hit testing
  /// and the [NodeWidget] container so the rendered footprint matches the
  /// scope resolver's idea of where the node ends.
  Size effectiveNodeSizeScreen(NodeView node, List<BigInt> nodeScopeChain) {
    final logical = effectiveNodeSizeLogical(node, nodeScopeChain);
    return Size(logical.width * scale, logical.height * scale);
  }

  /// True when the body identified by [bodyScopeChain] (`[...parent, hofId]`)
  /// or any ancestor body is collapsed — its content should be hidden and
  /// the body region rendered as a placeholder. Always `false` for the
  /// top-level scope (empty chain).
  bool isBodyCollapsed(List<BigInt> bodyScopeChain) {
    if (bodyScopeChain.isEmpty) return false;
    for (int i = 1; i <= bodyScopeChain.length; i++) {
      if (layout.isCollapsed(bodyScopeChain.sublist(0, i))) return true;
    }
    return false;
  }

  /// True when the body at [bodyScopeChain] (`[...parent, hofId]`) is hidden
  /// because its owner HOF's `f` pin is wired. Exact-match (unlike
  /// [isBodyCollapsed], which cascades from ancestors): the HOF widget only
  /// consults this for its own body, to choose the "driven by `f`"
  /// placeholder over the generic collapse placeholder.
  bool isBodyFunctionOverridden(List<BigInt> bodyScopeChain) {
    return layout.isFunctionOverridden(bodyScopeChain);
  }

  /// Recursive bottom-up size computation. After this returns, `layout
  /// .bodySizes[chain]` holds the body's rendered size = `max(stored,
  /// content_bbox + padding)`. Recurses into nested HOF bodies first so
  /// their sizes contribute to the outer body's `content_bbox`.
  ///
  /// For inner HOF child nodes the footprint is taken from
  /// [effectiveNodeSizeLogical] rather than [getNodeSize] — the cached
  /// effective body size is already populated by the recursive call above,
  /// so an inner body that grew past its stored size cascades into this
  /// body's `content_bbox`. See `doc/design_zones_ui.md` §U6.
  void _computeBodySize(ZoneView zone, List<BigInt> chain) {
    // Recurse first so inner body sizes are in the cache before we read them
    // back as part of this body's content_bbox.
    for (final innerNode in zone.nodes.values) {
      final innerZone = innerNode.zone;
      if (innerZone == null) continue;
      _computeBodySize(innerZone, [...chain, innerNode.id]);
    }

    double maxRight = 0;
    double maxBottom = 0;
    for (final node in zone.nodes.values) {
      final pos = apiVec2ToOffset(node.position);
      final logicalSize = effectiveNodeSizeLogical(node, chain);
      final right = pos.dx + logicalSize.width;
      final bottom = pos.dy + logicalSize.height;
      if (right > maxRight) maxRight = right;
      if (bottom > maxBottom) maxBottom = bottom;
    }
    const padding = BASE_HOF_BODY_BOTTOM_PADDING;
    final contentWidth = maxRight + padding;
    final contentHeight = maxBottom + padding;
    final width = contentWidth > zone.storedWidth ? contentWidth : zone.storedWidth;
    final height =
        contentHeight > zone.storedHeight ? contentHeight : zone.storedHeight;
    layout.bodySizes[chain] = Size(width, height);
  }

  /// Recursive top-down origin placement for child bodies. The parent body's
  /// origin is [parentOrigin] (screen coords); each child body's origin is
  /// `parentOrigin + (hof.position + body-inner-offset) * scale`.
  void _placeChildBodies(
      ZoneView parent, List<BigInt> parentChain, Offset parentOrigin) {
    for (final innerNode in parent.nodes.values) {
      final innerZone = innerNode.zone;
      if (innerZone == null) continue;
      final innerChain = [...parentChain, innerNode.id];
      final hofPos = apiVec2ToOffset(innerNode.position);
      final bodyTopLeftLocal = hofPos +
          Offset(hofBodyLeftOffset(innerNode), BASE_HOF_BODY_TOP_OFFSET);
      final origin = parentOrigin + bodyTopLeftLocal * scale;
      layout.bodyOrigins[innerChain] = origin;
      _placeChildBodies(innerZone, innerChain, origin);
    }
  }

  /// Map a body-local point at [scopeChain] to screen space. In phase U3
  /// `scopeChain` is always empty at call sites (body content isn't authored
  /// yet); U4+ will resolve nested-body positions via the layout cache.
  Offset scopedToScreen(List<BigInt> scopeChain, Offset bodyLocal) {
    if (scopeChain.isEmpty) {
      return logicalToScreen(bodyLocal, panOffset, scale);
    }
    final origin = layout.lookupOrigin(scopeChain);
    if (origin == null) {
      // Body not in cache (the chain points at a non-HOF or an unknown id).
      // Fall back to top-level transform so we degrade gracefully rather than
      // returning a bogus origin.
      return logicalToScreen(bodyLocal, panOffset, scale);
    }
    return origin + bodyLocal * scale;
  }

  /// Inverse of [scopedToScreen] for the top-level (empty) scope.
  Offset screenToScopedLocal(List<BigInt> scopeChain, Offset screen) {
    if (scopeChain.isEmpty) {
      return screenToLogical(screen, panOffset, scale);
    }
    final origin = layout.lookupOrigin(scopeChain);
    if (origin == null) return screenToLogical(screen, panOffset, scale);
    return (screen - origin) / scale;
  }

  /// Walk the screen point down through containing bodies; return the deepest
  /// scope chain that contains it and the body-local coordinate.
  ///
  /// A click inside an HOF's body interior (not on a body node, not on the
  /// HOF's chrome) returns that body's scope. Used by right-click → Add Node
  /// and by the active-body click handler. See `doc/design_zones_ui.md`
  /// §"Coordinate system — Screen → logical".
  ({List<BigInt> scopeChain, Offset bodyLocal}) findContainingScope(
      Offset screenPos) {
    // Walk top-level nodes for any HOF whose body contains the point, then
    // recurse. Deepest containment wins.
    final result = _findContainingScopeIn(
      root.nodes.values,
      const <BigInt>[],
      screenPos,
    );
    if (result != null) return result;
    return (
      scopeChain: const <BigInt>[],
      bodyLocal: screenToLogical(screenPos, panOffset, scale),
    );
  }

  ({List<BigInt> scopeChain, Offset bodyLocal})? _findContainingScopeIn(
    Iterable<NodeView> nodes,
    List<BigInt> scopeChain,
    Offset screenPos,
  ) {
    for (final node in nodes) {
      final zone = node.zone;
      if (zone == null) continue;
      final bodyChain = [...scopeChain, node.id];
      // A compact HOF has no body region: its cached origin/size still exist,
      // but nothing is drawn there, so it must not claim containment of clicks
      // that should hit the compact node itself. (A zoom-collapsed or
      // `f`-overridden HOF keeps its footprint, so `isCompact` — not
      // `isBodyCollapsed` — is the right predicate.)
      if (layout.isCompact(bodyChain)) continue;
      final bodyOrigin = layout.lookupOrigin(bodyChain);
      final bodySize = layout.lookupSize(bodyChain);
      if (bodyOrigin == null || bodySize == null) continue;
      final bodyRect = bodyOrigin & (bodySize * scale);
      if (!bodyRect.contains(screenPos)) continue;
      // Try deeper first — nested bodies trump the outer one.
      final inner = _findContainingScopeIn(
        zone.nodes.values,
        bodyChain,
        screenPos,
      );
      if (inner != null) return inner;
      final bodyLocal = (screenPos - bodyOrigin) / scale;
      return (scopeChain: bodyChain, bodyLocal: bodyLocal);
    }
    return null;
  }

  /// Find the node containing [screenPos] anywhere in the tree. Returns the
  /// node and the scope chain its containing network occupies.
  ///
  /// U4: walks into bodies. The deepest containing node wins — a click on a
  /// body node returns that node, not its containing HOF.
  ({List<BigInt> scopeChain, NodeView node})? findNodeAtScreenPosition(
      Offset screenPos) {
    return _findNodeAt(root.nodes.values, const <BigInt>[], screenPos);
  }

  ({List<BigInt> scopeChain, NodeView node})? _findNodeAt(
    Iterable<NodeView> nodes,
    List<BigInt> scopeChain,
    Offset screenPos,
  ) {
    // First recurse into any HOF bodies — body content layers on top of its
    // HOF, so deeper hits win. Skip the recursion whenever the body's content
    // is hidden: a compact HOF (no body region), a zoom-collapsed body, or an
    // `f`-overridden body all have body nodes whose cached origins/sizes still
    // place them over the owner node's rect, so without this guard those
    // hidden nodes would be returned in preference to the HOF itself. Use
    // `isBodyCollapsed` — true for all three hidden states (compact, zoom,
    // function-override) — so hit-testing matches the render and wire walks.
    for (final node in nodes) {
      final zone = node.zone;
      if (zone == null) continue;
      final bodyChain = [...scopeChain, node.id];
      if (isBodyCollapsed(bodyChain)) continue;
      final hit = _findNodeAt(zone.nodes.values, bodyChain, screenPos);
      if (hit != null) return hit;
    }
    // Then check nodes at this scope.
    for (final node in nodes) {
      final nodeScreenPos =
          scopedToScreen(scopeChain, apiVec2ToOffset(node.position));
      Size screenSize;
      if (node.nodeTypeName == 'Comment') {
        screenSize = Size(
          (node.commentWidth ?? 200.0) * scale,
          (node.commentHeight ?? 100.0) * scale,
        );
      } else {
        // Use the cache-aware effective size so an HOF whose body grew past
        // its stored size still hit-tests against the full visible rect.
        screenSize = effectiveNodeSizeScreen(node, scopeChain);
      }
      final nodeRect = nodeScreenPos & screenSize;
      if (!nodeRect.contains(screenPos)) continue;
      // Special-case HOF nodes: clicks landing inside the body region are
      // *not* on the HOF itself — they're on (the body's empty interior of)
      // the body, which is logically empty space at that scope. Returning
      // the HOF here would short-circuit right-click → Add Node and the
      // body-empty-space click-to-activate handler. Body nodes are handled
      // by the recursion above, so by the time we reach this branch we know
      // no body node was hit.
      //
      // Exception: zone-input and zone-output pins are positioned at the
      // inner edges of the body region, so geometrically they sit inside
      // `bodyRect`. A click on one of those pins must NOT fall through to
      // "empty space" — otherwise the root pointer-down handler starts a
      // selection-rect drag in parallel with the pin's wire-drag Draggable,
      // and the rect overlay obscures the dragged wire. Report it as a hit
      // on the HOF.
      //
      // Gated on `!isCompact`: a compact HOF has no body region, so it must
      // fall through to `return (scopeChain, node)` and hit-test as an
      // ordinary node across its whole width. A zoom-collapsed or
      // `f`-overridden HOF keeps its full body footprint and must still treat
      // clicks there as body empty-space / zone-pin hits, so `isCompact`
      // (not `isBodyCollapsed`) is the right predicate. The compact rect is
      // already reflected in the `nodeRect` tested above via
      // `effectiveNodeSizeScreen`.
      //
      // Also gated on `ZoomLevel.normal`: only the normal zoom renders the
      // body region (with its zone pins and resize handle). In zoomed-out
      // modes the HOF is drawn as a single solid compact box
      // (`_buildZoomedOutNodeContent`) with no body region — so the whole
      // footprint must hit-test as the node. Without this gate, a click in the
      // center of a zoomed-out HOF/closure box falls through to "empty space",
      // and the outer Listener starts a rectangle-selection drag in parallel
      // with the node's own pan gesture; on release the selection's
      // refreshFromKernel clobbers the uncommitted drag position and the node
      // bounces back to where it started.
      if (node.zone != null && zoomLevel == ZoomLevel.normal) {
        final bodyChain = [...scopeChain, node.id];
        if (!layout.isCompact(bodyChain)) {
          final bodyOrigin = layout.lookupOrigin(bodyChain);
          final bodySize = layout.lookupSize(bodyChain);
          if (bodyOrigin != null && bodySize != null) {
            final bodyRect = bodyOrigin & (bodySize * scale);
            if (bodyRect.contains(screenPos)) {
              if (_isPositionOnZonePin(node, scopeChain, screenPos)) {
                return (scopeChain: scopeChain, node: node);
              }
              // Bottom-right resize handle. Geometrically inside `bodyRect`,
              // so without this check the click would fall through to "body
              // empty space" — and the outer Listener's pointer-down handler
              // (in `node_network.dart`) would start a rectangle-selection
              // drag in parallel with the handle's own pan gesture, leaving
              // a stray selection rect visible while the body resizes.
              //
              // The handle is only rendered when the body region is fully
              // shown — a zoom-collapsed or `f`-overridden body has its
              // content (incl. the handle) hidden but keeps its footprint,
              // so guard on `!isBodyCollapsed` (which folds in compact + zoom
              // + f-override).
              if (!isBodyCollapsed(bodyChain) &&
                  _isPositionOnResizeHandle(bodyRect, screenPos)) {
                return (scopeChain: scopeChain, node: node);
              }
              // Click is in body empty space — fall through to the next node
              // (the HOF is reported as "not hit"). Caller's right-click
              // handler will then see this as empty space and open Add Node
              // popup parameterized by the body's scope.
              continue;
            }
          }
        }
      }
      return (scopeChain: scopeChain, node: node);
    }
    return null;
  }

  /// True if [screenPos] lands on any node. Convenience wrapper over
  /// [findNodeAtScreenPosition] for hit tests that don't need the node.
  bool isPositionOnNode(Offset screenPos) =>
      findNodeAtScreenPosition(screenPos) != null;

  /// True if [screenPos] lands within the hit area of one of [hof]'s zone-input
  /// or zone-output pins. Used by [_findNodeAt] to keep zone-pin clicks from
  /// falling through the body-empty-space branch.
  bool _isPositionOnZonePin(
      NodeView hof, List<BigInt> scopeChain, Offset screenPos) {
    final zone = hof.zone;
    if (zone == null) return false;
    final hw = PIN_HIT_AREA_WIDTH / 2 * scale;
    final hh = PIN_HIT_AREA_HEIGHT / 2 * scale;
    bool hitsPin(PinKind kind, int index) {
      final ref = PinReference(
        nodeId: hof.id,
        scopeChain: scopeChain,
        pinKind: kind,
        pinIndex: index,
        dataType: '',
      );
      final center = tryPinScreenPosition(ref)?.$1;
      if (center == null) return false;
      return (screenPos.dx - center.dx).abs() < hw &&
          (screenPos.dy - center.dy).abs() < hh;
    }

    for (int i = 0; i < zone.zoneInputPins.length; i++) {
      if (hitsPin(PinKind.zoneInput, i)) return true;
    }
    for (int i = 0; i < zone.zoneOutputPins.length; i++) {
      if (hitsPin(PinKind.zoneOutput, i)) return true;
    }
    return false;
  }

  /// True if [screenPos] lands within the body's bottom-right resize handle.
  /// Mirrors the handle's `Positioned(right: 0, bottom: 0)` placement and
  /// `BASE_HOF_BODY_RESIZE_HANDLE_SIZE` square in `_BodyResizeHandle`
  /// (`node_widget.dart`) — must stay in lockstep with that widget or this
  /// hit area drifts off the visible handle.
  bool _isPositionOnResizeHandle(Rect bodyRect, Offset screenPos) {
    final double handleScreen = BASE_HOF_BODY_RESIZE_HANDLE_SIZE * scale;
    return screenPos.dx >= bodyRect.right - handleScreen &&
        screenPos.dx <= bodyRect.right &&
        screenPos.dy >= bodyRect.bottom - handleScreen &&
        screenPos.dy <= bodyRect.bottom;
  }

  /// Resolve a pin to its on-screen position and the data type the pin
  /// actually carries (effective resolved type for output pins, declared type
  /// for input pins). Returns null when the node or pin index is no longer
  /// valid — callers tolerate this for wires that point at deleted nodes.
  (Offset, String)? tryPinScreenPosition(PinReference pin) {
    final node = _resolveNode(pin.scopeChain, pin.nodeId);
    if (node == null) return null;
    switch (pin.pinKind) {
      case PinKind.externalInput:
        if (pin.pinIndex < 0 || pin.pinIndex >= node.inputPins.length) {
          return null;
        }
        break;
      case PinKind.zoneInput:
        final zone = node.zone;
        if (zone == null) return null;
        if (pin.pinIndex < 0 || pin.pinIndex >= zone.zoneInputPins.length) {
          return null;
        }
        break;
      case PinKind.zoneOutput:
        final zone = node.zone;
        if (zone == null) return null;
        if (pin.pinIndex < 0 || pin.pinIndex >= zone.zoneOutputPins.length) {
          return null;
        }
        break;
      case PinKind.externalOutput:
      case PinKind.functionPin:
        break;
    }
    return _pinScreenPosition(node, pin);
  }

  /// Like [tryPinScreenPosition] but asserts the pin resolves. Used by code
  /// paths that constructed the [PinReference] themselves from a live node.
  (Offset, String) pinScreenPosition(PinReference pin) {
    final result = tryPinScreenPosition(pin);
    if (result == null) {
      throw StateError('pinScreenPosition: unresolvable pin $pin');
    }
    return result;
  }

  NodeView? _resolveNode(List<BigInt> scopeChain, BigInt nodeId) {
    if (scopeChain.isEmpty) {
      return root.nodes[nodeId];
    }
    // Walk down the scope chain into nested HOF bodies. Returns null if any
    // step fails (the chain references a missing or non-HOF node) — wires
    // that point at deleted nodes degrade gracefully.
    Map<BigInt, NodeView> currentNodes = root.nodes;
    for (final hofId in scopeChain) {
      final hof = currentNodes[hofId];
      final zone = hof?.zone;
      if (zone == null) return null;
      currentNodes = zone.nodes;
    }
    return currentNodes[nodeId];
  }

  (Offset, String) _pinScreenPosition(NodeView node, PinReference pin) {
    if (zoomLevel == ZoomLevel.normal) {
      return _pinPositionNormal(node, pin);
    } else {
      return _pinPositionZoomedOut(node, pin);
    }
  }

  (Offset, String) _pinPositionNormal(NodeView node, PinReference pin) {
    final nodePos = apiVec2ToOffset(node.position);
    final zone = node.zone;
    // A compact HOF has no body region, so its outer right edge sits at the
    // regular `NODE_WIDTH` (not the body-width formula). The `externalInput`
    // arm is at `x = 0` regardless; the zone pin arms are unreachable for a
    // compact body (its pins aren't rendered and its wires are skipped).
    final bool compact = zone != null && zone.collapsable && zone.collapsed;
    switch (pin.pinKind) {
      case PinKind.functionPin:
        // Function pin sits at the HOF's outer right edge for symmetry with
        // regular nodes; the HOF's overall width grows to include the body.
        // Use the cached effective body width so the pin tracks the rendered
        // right edge (which grows when the body cascades past its stored
        // size).
        final nodeWidth = (zone != null && !compact)
            ? hofBodyLeftOffset(node) +
                effectiveBodySize(node, pin.scopeChain).width +
                hofBodyRightGutter(node)
            : NODE_WIDTH;
        final logicalPos =
            nodePos + Offset(nodeWidth, NODE_VERT_WIRE_OFFSET_FUNCTION_PIN);
        return (
          scopedToScreen(pin.scopeChain, logicalPos),
          node.functionType,
        );
      case PinKind.externalOutput:
        // External output pins live on the HOF's outer right edge — past the
        // body region. For non-HOFs (and compact HOFs) the right edge is
        // `NODE_WIDTH` as before. Use cached effective body width otherwise
        // (same cascade reasoning as the functionPin arm above).
        final nodeWidth = (zone != null && !compact)
            ? hofBodyLeftOffset(node) +
                effectiveBodySize(node, pin.scopeChain).width +
                hofBodyRightGutter(node)
            : NODE_WIDTH;
        // Closure: its single Function output renders in the title bar (the
        // legacy function-pin slot), so the endpoint sits at the title-bar
        // vertical offset, not in a right-edge output column. Mirrors the
        // `functionPin` arm above and the title-bar render in node_widget.
        final double vertOffset;
        if (zone != null && !compact && hofOutputPinInTitleBar(node)) {
          vertOffset = NODE_VERT_WIRE_OFFSET_FUNCTION_PIN;
        } else {
          vertOffset = NODE_VERT_WIRE_OFFSET +
              (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        }
        final String dataType;
        if (pin.pinIndex >= 0 && pin.pinIndex < node.outputPins.length) {
          dataType = node.outputPins[pin.pinIndex].effectiveDataType;
        } else {
          dataType = node.outputType;
        }
        final logicalPos = nodePos + Offset(nodeWidth, vertOffset);
        return (scopedToScreen(pin.scopeChain, logicalPos), dataType);
      case PinKind.externalInput:
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final logicalPos = nodePos + Offset(0.0, vertOffset);
        final dataType =
            (pin.pinIndex >= 0 && pin.pinIndex < node.inputPins.length)
                ? node.inputPins[pin.pinIndex].dataType
                : pin.dataType;
        return (scopedToScreen(pin.scopeChain, logicalPos), dataType);
      case PinKind.zoneInput:
        // Inner-left pin on the body region, facing into the body. The pin
        // widget is positioned at `left: 0` inside the body Container, so its
        // CENTER (where the wire endpoint sits) is at body's left edge +
        // PIN_HIT_AREA_WIDTH/2. This is the same inset on the other axis as
        // a regular input pin on a node, just on the inside-facing edge.
        final z = zone!;
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final logicalPos = nodePos +
            Offset(hofBodyLeftOffset(node) + PIN_HIT_AREA_WIDTH / 2,
                vertOffset);
        final dataType = pin.pinIndex < z.zoneInputPins.length
            ? z.zoneInputPins[pin.pinIndex].effectiveDataType
            : pin.dataType;
        return (scopedToScreen(pin.scopeChain, logicalPos), dataType);
      case PinKind.zoneOutput:
        // Inner-right pin on the body region, positioned at `right: 0`
        // inside the body Container, so its CENTER is at body's right edge
        // − PIN_HIT_AREA_WIDTH/2. Reads body size from the layout cache
        // (rebuilt every frame via `runLayoutPass` — incorporates
        // content_bbox and live-drag positions) so wire rendering stays
        // consistent with the body rect drawn by the painter.
        final z = zone!;
        final bodySize = layout.lookupSize([...pin.scopeChain, node.id]) ??
            Size(z.storedWidth, z.storedHeight);
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final logicalPos = nodePos +
            Offset(
                hofBodyLeftOffset(node) +
                    bodySize.width -
                    PIN_HIT_AREA_WIDTH / 2,
                vertOffset);
        final dataType = pin.pinIndex < z.zoneOutputPins.length
            ? z.zoneOutputPins[pin.pinIndex].dataType
            : pin.dataType;
        return (scopedToScreen(pin.scopeChain, logicalPos), dataType);
    }
  }

  (Offset, String) _pinPositionZoomedOut(NodeView node, PinReference pin) {
    final nodeSize = getNodeSize(node, zoomLevel);
    final nodeScreenPos =
        scopedToScreen(pin.scopeChain, apiVec2ToOffset(node.position));
    switch (pin.pinKind) {
      case PinKind.functionPin:
        // Legacy behaviour parity: in zoomed-out mode the function pin sits
        // at the right edge center and reports `outputType` (not
        // `functionType`). Preserved verbatim from the pre-refactor painter.
        final centerY = nodeScreenPos.dy + nodeSize.height / 2;
        return (
          Offset(nodeScreenPos.dx + nodeSize.width, centerY),
          node.outputType,
        );
      case PinKind.externalOutput:
        final rightEdgeX = nodeScreenPos.dx + nodeSize.width;
        final numOutputs = node.outputPins.length;
        if (numOutputs > 1 && pin.pinIndex >= 0) {
          final spacing = BASE_ZOOMED_OUT_PIN_SPACING * scale;
          final totalHeight = (numOutputs - 1) * spacing;
          final startY = nodeScreenPos.dy + (nodeSize.height - totalHeight) / 2;
          final outputY = startY + (pin.pinIndex * spacing);
          final dataType = pin.pinIndex < node.outputPins.length
              ? node.outputPins[pin.pinIndex].effectiveDataType
              : node.outputType;
          return (Offset(rightEdgeX, outputY), dataType);
        } else {
          final centerY = nodeScreenPos.dy + nodeSize.height / 2;
          return (Offset(rightEdgeX, centerY), node.outputType);
        }
      case PinKind.externalInput:
        final leftEdgeX = nodeScreenPos.dx;
        final numInputs = node.inputPins.length;
        final spacing = BASE_ZOOMED_OUT_PIN_SPACING * scale;
        final totalHeight = (numInputs - 1) * spacing;
        final startY = nodeScreenPos.dy + (nodeSize.height - totalHeight) / 2;
        final inputY = startY + (pin.pinIndex * spacing);
        final dataType =
            (pin.pinIndex >= 0 && pin.pinIndex < node.inputPins.length)
                ? node.inputPins[pin.pinIndex].dataType
                : pin.dataType;
        return (Offset(leftEdgeX, inputY), dataType);
      case PinKind.zoneInput:
      case PinKind.zoneOutput:
        // Zoomed-out mode collapses HOFs to their chrome (per the design's
        // collapsed-body rule for far zoom). The inner-edge zone pins have
        // no individual position at this scale; route their endpoint to the
        // HOF's center so wires don't disappear at the screen origin.
        // (Body-internal wires aren't surfaced in U3 anyway.)
        final centerY = nodeScreenPos.dy + nodeSize.height / 2;
        final centerX = nodeScreenPos.dx + nodeSize.width / 2;
        final zone = node.zone;
        final String dataType;
        if (zone != null) {
          if (pin.pinKind == PinKind.zoneInput &&
              pin.pinIndex >= 0 &&
              pin.pinIndex < zone.zoneInputPins.length) {
            dataType = zone.zoneInputPins[pin.pinIndex].effectiveDataType;
          } else if (pin.pinKind == PinKind.zoneOutput &&
              pin.pinIndex >= 0 &&
              pin.pinIndex < zone.zoneOutputPins.length) {
            dataType = zone.zoneOutputPins[pin.pinIndex].dataType;
          } else {
            dataType = pin.dataType;
          }
        } else {
          dataType = pin.dataType;
        }
        return (Offset(centerX, centerY), dataType);
    }
  }
}
