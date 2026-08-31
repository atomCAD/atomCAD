import 'dart:math' as math;
import 'dart:ui' show ClipOp;

import 'package:flutter/material.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

// Dash patterns for wires carrying unaligned Blueprint/Crystal values.
// Long-dash for motif-unaligned (softer warning), short-dash for lattice-unaligned
// (more visually fragmented = more broken). See doc/design_blueprint_alignment.md §6.1.
const double WIRE_DASH_MOTIF_UNALIGNED_ON = 10.0;
const double WIRE_DASH_MOTIF_UNALIGNED_OFF = 4.0;
const double WIRE_DASH_LATTICE_UNALIGNED_ON = 3.0;
const double WIRE_DASH_LATTICE_UNALIGNED_OFF = 3.0;

// Comment-anchor leader lines (`doc/design_wire_annotations.md` Rendering).
// A short, tight dash in a muted grey, deliberately unlike every wire dash
// above: a leader line documents the drawing, it does not carry data, and it
// must never read as one.
const double ANCHOR_LEADER_DASH_ON = 4.0;
const double ANCHOR_LEADER_DASH_OFF = 4.0;
const double ANCHOR_LEADER_WIDTH = 1.2;
const double ANCHOR_LEADER_WIDTH_SELECTED = 2.0;
const Color ANCHOR_LEADER_COLOR = Color(0xFF8D8D8D);
const Color ANCHOR_LEADER_COLOR_SELECTED = Color(0xFFE08000);

/// Radius of the dot marking a leader line's target end. On a wire anchor it
/// is the only thing that says *which* wire; on a node anchor it sits on the
/// target node's border.
const double ANCHOR_LEADER_DOT_RADIUS = 3.0;

class WireHitResult {
  final BigInt sourceNodeId;
  final BigInt sourcePinIndex;
  final BigInt destNodeId;
  final BigInt destParamIndex;

  /// Scope the hit wire lives in (empty = top-level network). Drives the
  /// scope-aware wire-selection API call and the active-scope update so a
  /// click on a body wire selects it in the right body. See the single-scope
  /// invariant in `structure_designer.rs::clear_selection_in_other_scopes`.
  final List<BigInt> scopeChain;

  WireHitResult(this.sourceNodeId, this.sourcePinIndex, this.destNodeId,
      this.destParamIndex, this.scopeChain);
}

/// A boundary-crossing wire of a hidden (collapsed/compact) body, resolved to
/// drawable screen endpoints. `destPos` is the interior phantom position when
/// it falls inside the hidden container's rect, else the rect center. See
/// `NodeNetworkPainter._collectCollapsedCrossingWires`.
class _CollapsedCrossingWire {
  final WireView wire;

  /// The wire's storage scope (where it sits in the model).
  final List<BigInt> scopeChain;
  final Offset sourcePos;
  final String dataType;
  final Offset destPos;

  _CollapsedCrossingWire(
      this.wire, this.scopeChain, this.sourcePos, this.dataType, this.destPos);
}

// Grid appearance constants
const double GRID_MAJOR_SPACING = 100.0;
const double GRID_MINOR_SPACING = 20.0;
const Color GRID_MAJOR_COLOR = Color(0xFFDDDDDD); // Light grey
const Color GRID_MINOR_COLOR = Color(0xFFEEEEEE); // Very light grey
const double GRID_MAJOR_LINE_WIDTH = 1.0;
const double GRID_MINOR_LINE_WIDTH = 1.0;

class NodeNetworkPainter extends CustomPainter {
  final StructureDesignerModel graphModel;
  final Offset panOffset;
  final ZoomLevel zoomLevel;

  /// When false (default), this painter sits at the BOTTOM of the canvas stack
  /// and paints the grid + top-level wires only. When true, it sits at the
  /// TOP of the canvas stack (above the node widgets) and paints body wires
  /// + the dragged wire — both of which would otherwise be hidden by the HOF
  /// node widget's opaque body Container background.
  final bool overlay;

  /// Draw every wire as unselected. Set by the image export so a selected
  /// wire's glow — editing state, not meaning — stays out of the picture. See
  /// `NodeWidget.hideSelection`.
  final bool hideSelection;

  /// [repaint] should be the model's `dragRepaint` notifier: during the drag
  /// fast path (node drag / wire rubber-band drag) nothing rebuilds, so the
  /// painter must repaint from this listenable to track the live positions it
  /// reads out of the (in-place mutated) network view.
  NodeNetworkPainter(this.graphModel,
      {this.panOffset = Offset.zero,
      this.zoomLevel = ZoomLevel.normal,
      this.overlay = false,
      this.hideSelection = false,
      super.repaint});

  /// A wire's selection state as it should be **drawn**; hit testing keeps
  /// reading `wire.selected`.
  bool _drawSelected(bool wireSelected) => wireSelected && !hideSelection;

  /// Build a [ScopeResolver] for the current frame. Returns null when no
  /// network is active. Constructed once per `paint` and once per
  /// `findWireAtPosition` call so the painter can be a CustomPainter (no
  /// per-frame mutable state held on the painter instance).
  ScopeResolver? _makeResolver() {
    final view = graphModel.nodeNetworkView;
    if (view == null) return null;
    return ScopeResolver(
      root: view,
      panOffset: panOffset,
      scale: getZoomScale(zoomLevel),
      zoomLevel: zoomLevel,
    );
  }

  /// Build a [PinReference] for the source endpoint of [wire]. [scopeChain]
  /// is the wire's storage scope (where it sits in the model); the source's
  /// own scope is that scope with the last `source_scope_depth` elements
  /// dropped — captures (`source_scope_depth ≥ 1`) and iteration-value
  /// references (`ZoneInput` source) live in an ancestor frame.
  PinReference _wireSourcePin(WireView wire, List<BigInt> scopeChain) {
    // Defensive clamp: a wire whose `source_scope_depth` exceeds the storage
    // scope depth is structurally invalid (it would reference a non-existent
    // ancestor frame); rather than throw a RangeError from `sublist`, route
    // the pin to the top-level scope and let `tryPinScreenPosition` skip it
    // when the source node id can't be found there.
    final depth = wire.sourceScopeDepth.clamp(0, scopeChain.length);
    final sourceScope = depth == 0
        ? scopeChain
        : scopeChain.sublist(0, scopeChain.length - depth);
    final PinKind pinKind;
    final int pinIndex;
    final sourcePin = wire.sourcePin;
    if (sourcePin is APISourcePin_ZoneInput) {
      // Inside-facing zone-input pin on the HOF identified by `wire.sourceNodeId`.
      // The HOF lives in `sourceScope` (its containing network); the pin faces
      // into the body where the wire is stored.
      pinKind = PinKind.zoneInput;
      pinIndex = sourcePin.pinIndex;
    } else {
      // NodeOutput source (regular output or function pin). The legacy
      // `pin_index == -1` convention selects the function pin; everything else
      // is a regular external output.
      pinKind = wire.sourceOutputPinIndex == -1
          ? PinKind.functionPin
          : PinKind.externalOutput;
      pinIndex = wire.sourceOutputPinIndex;
    }
    return PinReference(
      nodeId: wire.sourceNodeId,
      scopeChain: sourceScope,
      pinKind: pinKind,
      pinIndex: pinIndex,
      dataType: '',
    );
  }

  /// Build a [PinReference] for the destination endpoint of [wire].
  ///
  /// For body-return wires (`destination_argument_kind == ZoneOutput`), the
  /// dest is the HOF's zone-output pin: the HOF itself lives in the *parent*
  /// of [scopeChain] (since [scopeChain] is the body's scope, terminating at
  /// the HOF). For all other wires the dest is a regular `externalInput` in
  /// the same scope as the source.
  PinReference _wireDestPin(WireView wire, List<BigInt> scopeChain) {
    if (wire.destinationArgumentKind == APIArgumentKind.zoneOutput) {
      // The HOF's containing scope is scopeChain with its terminal element
      // (the HOF's own id) chopped off.
      final hofScope = scopeChain.isEmpty
          ? const <BigInt>[]
          : scopeChain.sublist(0, scopeChain.length - 1);
      return PinReference(
        nodeId: wire.destNodeId,
        scopeChain: hofScope,
        pinKind: PinKind.zoneOutput,
        pinIndex: wire.destParamIndex.toInt(),
        dataType: '',
      );
    }
    return PinReference(
      nodeId: wire.destNodeId,
      scopeChain: scopeChain,
      pinKind: PinKind.externalInput,
      pinIndex: wire.destParamIndex.toInt(),
      dataType: '',
    );
  }

  @override
  void paint(Canvas canvas, Size size) {
    final resolver = _makeResolver();
    if (resolver == null) return;

    // Bottom layer: grid + top-level wires only. Top-level wires render under
    // the node widgets so external pin circles visually cover the wire ends
    // (the conventional look). Body wires and the dragged wire are deferred
    // to the overlay pass because they'd otherwise be hidden behind the HOF
    // node widget's opaque body Container.
    if (!overlay) {
      _drawGrid(canvas, size);

      Paint paint = Paint()
        ..color = Colors.black
        ..strokeWidth = WIRE_WIDTH_NORMAL
        ..style = PaintingStyle.stroke;

      _drawWiresAtScope(
          resolver, resolver.root.wires, const <BigInt>[], canvas, paint);
      _drawAnchorLeaders(resolver, resolver.root.nodes, resolver.root.wires,
          const <BigInt>[], canvas);
      return;
    }

    // Overlay layer: body wires (recursively into every HOF) and the dragged
    // wire. These need to paint *on top of* the node widgets so they're
    // visible inside HOF body regions.
    Paint paint = Paint()
      ..color = Colors.black
      ..strokeWidth = WIRE_WIDTH_NORMAL
      ..style = PaintingStyle.stroke;

    for (final node in resolver.root.nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      final innerChain = <BigInt>[node.id];
      if (resolver.isBodyCollapsed(innerChain)) {
        _drawCollapsedBodyCrossingWires(
            resolver, node, zone, const <BigInt>[], canvas, paint);
        continue;
      }
      _drawWiresInZone(resolver, zone, innerChain, canvas, paint);
    }

    // The anchor rubber band, above body wires like the dragged wire below.
    _drawDraggedAnchor(resolver, canvas);

    // Draw dragged wire on top of body wires.
    if (graphModel.draggedWire != null) {
      final startPin = graphModel.draggedWire!.startPin;
      final wireStart = resolver.tryPinScreenPosition(startPin);
      if (wireStart == null) {
        return;
      }
      final wireEndPos = graphModel.draggedWire!.wireEndPosition;
      final alignment = startPin.isOutput
          ? _getSourcePinAlignment(startPin.nodeId, startPin.pinIndex)
          : null;
      if (startPin.isOutput) {
        // start is source
        _drawWire(wireStart.$1, wireEndPos, canvas, paint, wireStart.$2, false,
            alignment);
      } else {
        // start is dest
        _drawWire(wireEndPos, wireStart.$1, canvas, paint, wireStart.$2, false,
            alignment);
      }
    }
  }

  /// Walk every body reachable from the top-level network and paint each
  /// body's interior wires via [_drawWiresAtScope]. Called from the overlay
  /// pass — top-level wires are painted by the bottom-layer pass directly,
  /// not here. See `doc/design_zones_ui.md` §"Wire rendering across scopes".
  ///
  /// Bodies that are collapsed (rendered too small to be readable per the
  /// U6 zoom-level rule, or compact) hide their interior wires, but wires
  /// *crossing* the boundary from a visible scope — captures, ancestor
  /// iteration-value references — still render, clipped so they terminate at
  /// the visible placeholder's border (issue #416). See
  /// [_drawCollapsedBodyCrossingWires].
  void _drawWiresInZone(
    ScopeResolver resolver,
    ZoneView zone,
    List<BigInt> scopeChain,
    Canvas canvas,
    Paint paint,
  ) {
    _drawWiresAtScope(resolver, zone.wires, scopeChain, canvas, paint);
    _drawAnchorLeaders(resolver, zone.nodes, zone.wires, scopeChain, canvas);
    for (final node in zone.nodes.values) {
      final inner = node.zone;
      if (inner == null) continue;
      final innerChain = [...scopeChain, node.id];
      if (resolver.isBodyCollapsed(innerChain)) {
        _drawCollapsedBodyCrossingWires(
            resolver, node, inner, scopeChain, canvas, paint);
        continue;
      }
      _drawWiresInZone(resolver, inner, innerChain, canvas, paint);
    }
  }

  /// Screen rect of the visible stand-in for a hidden body: the body-region
  /// placeholder at normal zoom, or the whole node rect for a compact HOF and
  /// for the zoomed-out solid-box rendering (which has no distinct body
  /// sub-region). Crossing wires are clipped against this rect so they
  /// terminate exactly at its border.
  Rect? _hiddenBodyClipRect(
      ScopeResolver resolver, NodeView hof, List<BigInt> parentChain) {
    final bodyChain = [...parentChain, hof.id];
    if (zoomLevel == ZoomLevel.normal &&
        !resolver.layout.isCompact(bodyChain)) {
      final origin = resolver.layout.lookupOrigin(bodyChain);
      final size = resolver.layout.lookupSize(bodyChain);
      if (origin == null || size == null) return null;
      return origin & (size * getZoomScale(zoomLevel));
    }
    final pos =
        resolver.scopedToScreen(parentChain, apiVec2ToOffset(hof.position));
    return pos & resolver.effectiveNodeSizeScreen(hof, parentChain);
  }

  /// True when [wire]'s source pin is rendered outside the hidden body
  /// subtree whose scope chain has length [hiddenDepth]. Only such wires have
  /// a visible portion to draw; wires sourced inside the hidden subtree
  /// (interior wires, the hidden HOF's own zone-input references, body-return
  /// wires) are fully hidden. [storageScope] is the wire's storage scope,
  /// which always extends the hidden chain.
  static bool _sourceVisiblyOutsideHiddenBody(
      WireView wire, List<BigInt> storageScope, int hiddenDepth) {
    final depth = wire.sourceScopeDepth.clamp(0, storageScope.length);
    final sourceScopeLen = storageScope.length - depth;
    if (wire.sourcePin is APISourcePin_ZoneInput) {
      // The pin faces into the body of the HOF at `sourceScope ++ [nodeId]`;
      // it is visible only when that body is a proper ancestor of the hidden
      // one (equal length means it IS the hidden body).
      return sourceScopeLen + 1 < hiddenDepth;
    }
    return sourceScopeLen < hiddenDepth;
  }

  /// Collect the boundary-crossing wires stored anywhere in a hidden body's
  /// subtree (the body's own wires plus nested bodies', which are hidden with
  /// it), resolved to drawable endpoints. Endpoints resolve through the
  /// layout cache, which is populated regardless of collapse, so a crossing
  /// wire follows the exact same Bezier it has when the body is expanded and
  /// nothing jumps when collapse toggles. The destination falls back to the
  /// clip rect's center when its phantom position lands outside the rect
  /// (compact HOFs keep no expanded-body layout under their shrunken rect).
  List<_CollapsedCrossingWire> _collectCollapsedCrossingWires(
    ScopeResolver resolver,
    ZoneView zone,
    List<BigInt> scopeChain,
    int hiddenDepth,
    Rect clipRect,
  ) {
    final result = <_CollapsedCrossingWire>[];
    for (final wire in zone.wires) {
      if (!_sourceVisiblyOutsideHiddenBody(wire, scopeChain, hiddenDepth)) {
        continue;
      }
      final source =
          resolver.tryPinScreenPosition(_wireSourcePin(wire, scopeChain));
      final dest =
          resolver.tryPinScreenPosition(_wireDestPin(wire, scopeChain));
      if (source == null || dest == null) continue;
      final destPos = clipRect.contains(dest.$1) ? dest.$1 : clipRect.center;
      result.add(_CollapsedCrossingWire(
          wire, scopeChain, source.$1, source.$2, destPos));
    }
    for (final node in zone.nodes.values) {
      final inner = node.zone;
      if (inner == null) continue;
      final innerChain = [...scopeChain, node.id];
      // A nested `f`-overridden body is inert regardless of the enclosing
      // collapse — keep its captures hidden, matching the expanded-state
      // behavior.
      if (resolver.isBodyFunctionOverridden(innerChain)) continue;
      result.addAll(_collectCollapsedCrossingWires(
          resolver, inner, innerChain, hiddenDepth, clipRect));
    }
    return result;
  }

  /// Draw the boundary-crossing wires of a hidden body, masked by
  /// `ClipOp.difference` so they terminate at the border of the visible
  /// stand-in instead of vanishing entirely (issue #416) or dangling over the
  /// placeholder. `f`-overridden bodies stay wire-free: the inline body is
  /// inert at eval time, so drawing its captures would misrepresent the data
  /// flow (the "driven by `f`" placeholder explains the state).
  void _drawCollapsedBodyCrossingWires(
    ScopeResolver resolver,
    NodeView hof,
    ZoneView zone,
    List<BigInt> parentChain,
    Canvas canvas,
    Paint paint,
  ) {
    final bodyChain = [...parentChain, hof.id];
    if (resolver.isBodyFunctionOverridden(bodyChain)) return;
    final clipRect = _hiddenBodyClipRect(resolver, hof, parentChain);
    if (clipRect == null) return;
    final crossing = _collectCollapsedCrossingWires(
        resolver, zone, bodyChain, bodyChain.length, clipRect);
    if (crossing.isEmpty) return;
    canvas.save();
    canvas.clipRect(clipRect, clipOp: ClipOp.difference);
    for (final c in crossing) {
      // Alignment dashes are top-level-only (see _drawWiresAtScope); body
      // captures render solid.
      _drawWire(c.sourcePos, c.destPos, canvas, paint, c.dataType,
          _drawSelected(c.wire.selected), null);
    }
    canvas.restore();
  }

  // ===== COMMENT ANCHOR LEADER LINES =====
  // doc/design_wire_annotations.md Rendering.

  /// Draw one dashed leader line per anchor of every comment stored at
  /// [scopeChain], from the note's border to what it documents.
  ///
  /// Anchors are scope-local (D5), so [nodes] and [wires] are that one scope's
  /// collections — a leader never leaves the network its note lives in.
  /// `NodeView` publishes anchors already **resolved** (dangling ones dropped,
  /// wire slot indices re-derived), so this only has to locate the target and
  /// draw.
  void _drawAnchorLeaders(
    ScopeResolver resolver,
    Map<BigInt, NodeView> nodes,
    List<WireView> wires,
    List<BigInt> scopeChain,
    Canvas canvas,
  ) {
    for (final comment in nodes.values) {
      if (comment.commentAnchors.isEmpty) continue;
      final commentRect = _commentScreenRect(resolver, comment, scopeChain);
      final selected = _drawSelected(comment.selected);
      for (final anchor in comment.commentAnchors) {
        final target = _anchorTargetPoint(
            resolver, anchor, nodes, wires, scopeChain, commentRect.center);
        if (target == null) continue;
        _drawLeader(canvas, commentRect, target, selected);
      }
    }
  }

  /// Screen rect of one note. Comments carry their own footprint rather than
  /// the pin-derived node size, which is exactly what
  /// `ScopeResolver.effectiveNodeSizeScreen` special-cases for them.
  Rect _commentScreenRect(
      ScopeResolver resolver, NodeView comment, List<BigInt> scopeChain) {
    final origin =
        resolver.scopedToScreen(scopeChain, apiVec2ToOffset(comment.position));
    return origin & resolver.effectiveNodeSizeScreen(comment, scopeChain);
  }

  /// Where an anchor's leader line ends: a point on the target node's border
  /// facing [commentCenter], or the midpoint of the target wire.
  ///
  /// Null when the target is not *drawable* in this frame — a node id absent
  /// from this scope's map, or a wire whose endpoints don't resolve. The
  /// resolved-anchor guarantee covers existence, not drawability.
  Offset? _anchorTargetPoint(
    ScopeResolver resolver,
    APICommentAnchor anchor,
    Map<BigInt, NodeView> nodes,
    List<WireView> wires,
    List<BigInt> scopeChain,
    Offset commentCenter,
  ) {
    switch (anchor) {
      case APICommentAnchor_Node(:final nodeId):
        final target = nodes[nodeId];
        if (target == null) return null;
        final rect = resolver.scopedToScreen(
                scopeChain, apiVec2ToOffset(target.position)) &
            resolver.effectiveNodeSizeScreen(target, scopeChain);
        return _borderPointToward(rect, commentCenter);
      case APICommentAnchor_Wire(anchor: final wireAnchor):
        final wire = _findAnchoredWire(wires, wireAnchor);
        if (wire == null) return null;
        final source =
            resolver.tryPinScreenPosition(_wireSourcePin(wire, scopeChain));
        final dest =
            resolver.tryPinScreenPosition(_wireDestPin(wire, scopeChain));
        if (source == null || dest == null) return null;
        // The wire's cubic Bezier at t = 0.5. Both control points are offset
        // from their own endpoint along x only, by the same amount in opposite
        // directions, so the midpoint reduces exactly to the endpoints' mean —
        // no need to walk the path.
        return (source.$1 + dest.$1) / 2;
    }
  }

  /// The wire an [APIWireAnchor] designates within one scope's wire list.
  ///
  /// A wire is identified from its destination side plus its source node id
  /// (D4); `source_pin` / `source_scope_depth` are properties of the resolved
  /// wire, not part of its identity. The index compared here is the one Rust
  /// resolved, so a pin reorder on a dynamic-arity destination is already
  /// folded in.
  WireView? _findAnchoredWire(List<WireView> wires, APIWireAnchor anchor) {
    for (final wire in wires) {
      if (wire.destNodeId == anchor.destinationNodeId &&
          wire.sourceNodeId == anchor.sourceNodeId &&
          wire.destinationArgumentKind == anchor.destinationArgumentKind &&
          wire.destParamIndex == anchor.destinationArgumentIndex) {
        return wire;
      }
    }
    return null;
  }

  /// The point on [rect]'s border along the ray from its centre toward
  /// [toward]. Used at both ends of a leader line so it starts and stops at
  /// the boxes' edges instead of disappearing underneath them.
  Offset _borderPointToward(Rect rect, Offset toward) {
    final center = rect.center;
    final d = toward - center;
    if (d.dx == 0 && d.dy == 0) return center;
    // Largest t for which center + t*d is still inside the box on both axes.
    final tx = d.dx == 0 ? double.infinity : (rect.width / 2) / d.dx.abs();
    final ty = d.dy == 0 ? double.infinity : (rect.height / 2) / d.dy.abs();
    return center + d * math.min(tx, ty);
  }

  void _drawLeader(
      Canvas canvas, Rect commentRect, Offset target, bool commentSelected) {
    final start = _borderPointToward(commentRect, target);
    final color =
        commentSelected ? ANCHOR_LEADER_COLOR_SELECTED : ANCHOR_LEADER_COLOR;
    final paint = Paint()
      ..color = color
      ..strokeWidth =
          commentSelected ? ANCHOR_LEADER_WIDTH_SELECTED : ANCHOR_LEADER_WIDTH
      ..style = PaintingStyle.stroke;
    final path = Path()
      ..moveTo(start.dx, start.dy)
      ..lineTo(target.dx, target.dy);
    canvas.drawPath(
        _dashedPath(path, ANCHOR_LEADER_DASH_ON, ANCHOR_LEADER_DASH_OFF),
        paint);
    canvas.drawCircle(target, ANCHOR_LEADER_DOT_RADIUS, Paint()..color = color);
  }

  /// The rubber band of an anchor drag in progress: the same dashed leader,
  /// from the note being anchored to the pointer. Painted in the overlay pass
  /// so it stays visible over HOF body backgrounds, and repainted off
  /// `model.dragRepaint` like every other drag-tracking canvas element.
  void _drawDraggedAnchor(ScopeResolver resolver, Canvas canvas) {
    final drag = graphModel.draggedCommentAnchor;
    if (drag == null) return;
    final comment = resolver.findNodeInScope(drag.scopeChain, drag.nodeId);
    if (comment == null) return;
    _drawLeader(canvas, _commentScreenRect(resolver, comment, drag.scopeChain),
        drag.endPosition, true);
  }

  void _drawWiresAtScope(
    ScopeResolver resolver,
    List<WireView> wires,
    List<BigInt> scopeChain,
    Canvas canvas,
    Paint paint,
  ) {
    for (var wire in wires) {
      final source =
          resolver.tryPinScreenPosition(_wireSourcePin(wire, scopeChain));
      final dest =
          resolver.tryPinScreenPosition(_wireDestPin(wire, scopeChain));
      if (source == null || dest == null) continue;
      // Alignment is only resolved for top-level nodes (no per-pin alignment
      // surfaced for body-scope evaluation in U4); body wires render solid.
      final alignment = scopeChain.isEmpty
          ? _getSourcePinAlignment(wire.sourceNodeId, wire.sourceOutputPinIndex)
          : null;
      _drawWire(source.$1, dest.$1, canvas, paint, source.$2,
          _drawSelected(wire.selected), alignment);
    }
  }

  /// Looks up the alignment carried by the given output pin on the source node.
  /// Returns `null` when the pin has no alignment (non-Blueprint/Crystal, or
  /// not yet evaluated) or when the pin index is the function pin (-1).
  APIAlignment? _getSourcePinAlignment(BigInt nodeId, int pinIndex) {
    if (pinIndex < 0) return null;
    final node = graphModel.nodeNetworkView?.nodes[nodeId];
    if (node == null || pinIndex >= node.outputPins.length) return null;
    return node.outputPins[pinIndex].alignment;
  }

  _drawWire(Offset sourcePos, Offset destPos, Canvas canvas, Paint paint,
      String dataType, bool selected, APIAlignment? alignment) {
    paint.color = getDataTypeColor(dataType);
    paint.strokeWidth = selected ? WIRE_WIDTH_SELECTED : WIRE_WIDTH_NORMAL;

    final path = _getPath(sourcePos, destPos);

    if (selected) {
      paint.color = WIRE_COLOR_SELECTED;

      // Draw glow effect for selected wire (always solid — alignment dashes
      // would be imperceptible under the wider glow stroke).
      final glowPaint = Paint()
        ..color = WIRE_COLOR_SELECTED.withValues(alpha: WIRE_GLOW_OPACITY)
        ..strokeWidth = paint.strokeWidth * 2
        ..style = PaintingStyle.stroke;

      canvas.drawPath(path, glowPaint);
    }

    final dashPattern = _dashPatternFor(alignment);
    if (dashPattern == null) {
      canvas.drawPath(path, paint);
    } else {
      canvas.drawPath(_dashedPath(path, dashPattern.$1, dashPattern.$2), paint);
    }
  }

  /// Returns `(onLength, offLength)` for the given alignment, or `null` for a
  /// solid wire (Aligned / no alignment info).
  (double, double)? _dashPatternFor(APIAlignment? alignment) {
    switch (alignment) {
      case APIAlignment.motifUnaligned:
        return (WIRE_DASH_MOTIF_UNALIGNED_ON, WIRE_DASH_MOTIF_UNALIGNED_OFF);
      case APIAlignment.latticeUnaligned:
        return (
          WIRE_DASH_LATTICE_UNALIGNED_ON,
          WIRE_DASH_LATTICE_UNALIGNED_OFF
        );
      case APIAlignment.aligned:
      case null:
        return null;
    }
  }

  /// Extracts a dashed sub-path of the Bezier by walking its arc-length via
  /// `PathMetric`, alternating `on` and `off` segments. No dependencies.
  Path _dashedPath(Path source, double on, double off) {
    final result = Path();
    for (final metric in source.computeMetrics()) {
      double distance = 0.0;
      bool draw = true;
      while (distance < metric.length) {
        final next =
            (distance + (draw ? on : off)).clamp(0.0, metric.length).toDouble();
        if (draw) {
          result.addPath(metric.extractPath(distance, next), Offset.zero);
        }
        distance = next;
        draw = !draw;
      }
    }
    return result;
  }

  Path _getPath(Offset sourcePos, Offset destPos) {
    final controlPoint1 = sourcePos + Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);
    final controlPoint2 = destPos - Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);

    return Path()
      ..moveTo(sourcePos.dx, sourcePos.dy)
      ..cubicTo(
        controlPoint1.dx,
        controlPoint1.dy,
        controlPoint2.dx,
        controlPoint2.dy,
        destPos.dx,
        destPos.dy,
      );
  }

  Path _getBand(Offset sourcePos, Offset destPos, double width) {
    final hw = width * 0.5;
    final off = destPos.dx > sourcePos.dx ? width : (-width);

    final sourcePos1 = Offset(sourcePos.dx, sourcePos.dy + hw);
    final sourcePos2 = Offset(sourcePos.dx, sourcePos.dy - hw);
    final destPos1 = Offset(destPos.dx, destPos.dy + hw);
    final destPos2 = Offset(destPos.dx, destPos.dy - hw);

    final controlPointStart1 =
        sourcePos1 + Offset(CUBIC_SPLINE_HORIZ_OFFSET - off, 0);
    final controlPointEnd1 = destPos1 - Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);

    final controlPointStart2 =
        sourcePos2 + Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);
    final controlPointEnd2 =
        destPos2 - Offset(CUBIC_SPLINE_HORIZ_OFFSET - off, 0);

    return Path()
      ..moveTo(sourcePos1.dx, sourcePos1.dy)
      ..cubicTo(
        controlPointStart1.dx,
        controlPointStart1.dy,
        controlPointEnd1.dx,
        controlPointEnd1.dy,
        destPos1.dx,
        destPos1.dy,
      )
      ..lineTo(destPos2.dx, destPos2.dy)
      ..cubicTo(
        controlPointEnd2.dx,
        controlPointEnd2.dy,
        controlPointStart2.dx,
        controlPointStart2.dy,
        sourcePos2.dx,
        sourcePos2.dy,
      )
      ..close();
  }

  /// Returns true for wires the selection model can address by a click: any
  /// `External`-destination wire — regular same-scope wires, the function pin
  /// (`pin_index == -1`), **captures** (`sourceScopeDepth ≥ 1`), and
  /// **iteration-value references** (`ZoneInput` source). All of these are
  /// stored on a destination node's `arguments`, so the Rust side can
  /// canonicalize their identity from storage and `delete_selected` can remove
  /// them. Only zone-output (body-return) wires are excluded — they live on the
  /// HOF's `zone_output_arguments`, not addressable through the selection model.
  static bool _isSelectableWire(WireView wire) =>
      wire.destinationArgumentKind == APIArgumentKind.external_;

  /// Hit-test wires across every scope. `position` is in screen coordinates and
  /// the resolver returns pin endpoints in screen coordinates, so no transform
  /// is needed. Bodies are tested before the top level (deepest first), mirroring
  /// the overlay painter's stacking — body wires render *on top of* node widgets
  /// and the (hidden-under-widgets) top-level wires, so a click inside a body
  /// must never select a top-level wire passing invisibly beneath it. Collapsed
  /// bodies expose only their boundary-crossing wires (the visible clipped
  /// portion outside the placeholder), mirroring the paint pass.
  WireHitResult? findWireAtPosition(Offset position) {
    final resolver = _makeResolver();
    if (resolver == null) return null;

    for (final node in resolver.root.nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      final innerChain = <BigInt>[node.id];
      if (resolver.isBodyCollapsed(innerChain)) {
        final hit = _hitCollapsedBodyCrossingWires(
            resolver, node, zone, const <BigInt>[], position);
        if (hit != null) return hit;
        continue;
      }
      final hit = _hitWiresInZone(resolver, zone, innerChain, position);
      if (hit != null) return hit;
    }

    // Top-level wires last.
    return _hitWiresAtScope(
        resolver, resolver.root.wires, const <BigInt>[], position);
  }

  /// Recursively hit-test a body's wires (deepest nested body first), then the
  /// body's own wires. Mirrors [_drawWiresInZone].
  WireHitResult? _hitWiresInZone(
    ScopeResolver resolver,
    ZoneView zone,
    List<BigInt> scopeChain,
    Offset position,
  ) {
    for (final node in zone.nodes.values) {
      final inner = node.zone;
      if (inner == null) continue;
      final innerChain = [...scopeChain, node.id];
      if (resolver.isBodyCollapsed(innerChain)) {
        final hit = _hitCollapsedBodyCrossingWires(
            resolver, node, inner, scopeChain, position);
        if (hit != null) return hit;
        continue;
      }
      final deep = _hitWiresInZone(resolver, inner, innerChain, position);
      if (deep != null) return deep;
    }
    return _hitWiresAtScope(resolver, zone.wires, scopeChain, position);
  }

  /// Hit-test the boundary-crossing wires of a hidden body. Only the clipped,
  /// visible portion (outside the placeholder rect) is hittable — a click
  /// inside the rect belongs to the node/placeholder. Mirrors
  /// [_drawCollapsedBodyCrossingWires].
  WireHitResult? _hitCollapsedBodyCrossingWires(
    ScopeResolver resolver,
    NodeView hof,
    ZoneView zone,
    List<BigInt> parentChain,
    Offset position,
  ) {
    final bodyChain = [...parentChain, hof.id];
    if (resolver.isBodyFunctionOverridden(bodyChain)) return null;
    final clipRect = _hiddenBodyClipRect(resolver, hof, parentChain);
    if (clipRect == null) return null;
    if (clipRect.contains(position)) return null;
    for (final c in _collectCollapsedCrossingWires(
        resolver, zone, bodyChain, bodyChain.length, clipRect)) {
      if (!_isSelectableWire(c.wire)) continue;
      final band = _getBand(c.sourcePos, c.destPos, HIT_TEST_WIRE_WIDTH);
      if (band.contains(position)) {
        return WireHitResult(
          c.wire.sourceNodeId,
          BigInt.from(c.wire.sourceOutputPinIndex),
          c.wire.destNodeId,
          c.wire.destParamIndex,
          c.scopeChain,
        );
      }
    }
    return null;
  }

  /// Hit-test the [wires] stored at [scopeChain] against [position].
  WireHitResult? _hitWiresAtScope(
    ScopeResolver resolver,
    List<WireView> wires,
    List<BigInt> scopeChain,
    Offset position,
  ) {
    for (final wire in wires) {
      if (!_isSelectableWire(wire)) continue;
      final source =
          resolver.tryPinScreenPosition(_wireSourcePin(wire, scopeChain));
      final dest =
          resolver.tryPinScreenPosition(_wireDestPin(wire, scopeChain));
      if (source == null || dest == null) continue;

      final hitTestPath = _getBand(source.$1, dest.$1, HIT_TEST_WIRE_WIDTH);
      if (hitTestPath.contains(position)) {
        return WireHitResult(
          wire.sourceNodeId,
          BigInt.from(wire.sourceOutputPinIndex),
          wire.destNodeId,
          wire.destParamIndex,
          scopeChain,
        );
      }
    }
    return null;
  }

  /// Draw a grid pattern that scales with zoom level
  void _drawGrid(Canvas canvas, Size size) {
    final scale = getZoomScale(zoomLevel);
    final Rect visibleRect = Offset.zero & size;

    // Apply clipping to prevent drawing outside the widget area
    canvas.clipRect(visibleRect);

    // At zoomed-out levels, only show major grid lines with minor color
    final bool showMinorLines = (zoomLevel == ZoomLevel.normal);

    // Create paints for grid lines
    final minorPaint = Paint()
      ..color = GRID_MINOR_COLOR
      ..strokeWidth = GRID_MINOR_LINE_WIDTH
      ..style = PaintingStyle.stroke;

    final majorPaint = Paint()
      ..color = showMinorLines ? GRID_MAJOR_COLOR : GRID_MINOR_COLOR
      ..strokeWidth =
          showMinorLines ? GRID_MAJOR_LINE_WIDTH : GRID_MINOR_LINE_WIDTH
      ..style = PaintingStyle.stroke;

    // Convert visible screen rect to logical coordinates
    final logicalTopLeft =
        screenToLogical(visibleRect.topLeft, panOffset, scale);
    final logicalBottomRight =
        screenToLogical(visibleRect.bottomRight, panOffset, scale);

    // Calculate grid line positions in logical space
    final gridSpacing =
        showMinorLines ? GRID_MINOR_SPACING : GRID_MAJOR_SPACING;
    final startX = (logicalTopLeft.dx / gridSpacing).floor() * gridSpacing;
    final startY = (logicalTopLeft.dy / gridSpacing).floor() * gridSpacing;

    // Draw vertical grid lines
    for (double logicalX = startX;
        logicalX <= logicalBottomRight.dx;
        logicalX += gridSpacing) {
      final screenX = logicalToScreen(Offset(logicalX, 0), panOffset, scale).dx;

      // Check if this is a major grid line
      final isMajor =
          (logicalX / GRID_MAJOR_SPACING).round() * GRID_MAJOR_SPACING ==
              logicalX;

      // In normal mode, draw both minor and major. In zoomed-out mode, only major
      final shouldDraw = showMinorLines || isMajor;

      if (shouldDraw) {
        canvas.drawLine(
            Offset(screenX, visibleRect.top),
            Offset(screenX, visibleRect.bottom),
            isMajor ? majorPaint : minorPaint);
      }
    }

    // Draw horizontal grid lines
    for (double logicalY = startY;
        logicalY <= logicalBottomRight.dy;
        logicalY += gridSpacing) {
      final screenY = logicalToScreen(Offset(0, logicalY), panOffset, scale).dy;

      // Check if this is a major grid line
      final isMajor =
          (logicalY / GRID_MAJOR_SPACING).round() * GRID_MAJOR_SPACING ==
              logicalY;

      // In normal mode, draw both minor and major. In zoomed-out mode, only major
      final shouldDraw = showMinorLines || isMajor;

      if (shouldDraw) {
        canvas.drawLine(
            Offset(visibleRect.left, screenY),
            Offset(visibleRect.right, screenY),
            isMajor ? majorPaint : minorPaint);
      }
    }
  }

  @override
  bool shouldRepaint(NodeNetworkPainter oldDelegate) => true;
}
