/// Snapping guides while a node is dragged
/// (`doc/design_deliberate_node_placement.md` B1).
///
/// The **grabbed** node snaps per axis to what its neighbours offer, and a
/// guide shows why: a straight wire, a shared edge (column / row), the
/// `normal` gap, or the gap of the neighbouring pair it is joining (equal
/// spacing). Everything here is pure geometry in the scope's own logical
/// coordinates — the node positions' frame, body-local inside a body — so
/// `test/snap_guides_test.dart` exercises it with no widget and no Rust.
///
/// The cost model is the drag fast path's (`node_network/AGENTS.md`): a
/// [SnapSession] is built **once per drag** from the nodes on screen, with
/// every target pre-reduced to "a value for the grabbed node's left (or top)
/// edge" and sorted, so a drag tick is a binary search per axis. The model
/// owns the session and the unsnapped pointer position; the overlay painter
/// draws [SnapResult.guides].
library;

import 'dart:ui';

import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// How close, in **screen** px, the grabbed node must come to a target for
/// it to snap. Screen px, so the feel is the same at every zoom level.
const double SNAP_THRESHOLD_SCREEN_PX = 6.0;

/// The `normal` gaps of the design's D10, in logical px: across (x) and down
/// (y). Mirrors Rust `node_layout::DEFAULT_HORIZONTAL_GAP` and
/// `layout::common::VERTICAL_GAP`, the same way the node-size constants are
/// mirrored; the bracket labels print them.
const double SNAP_GAP_ACROSS = 50.0;
const double SNAP_GAP_DOWN = 30.0;

/// What a snap aligned the grabbed node with, in priority order: when
/// several are in range on one axis, the lowest kind wins, then the nearest.
enum SnapKind { straightWire, edge, gap, equalSpacing }

/// A guide to draw for a snap, in scope-local logical coordinates.
sealed class SnapGuide {
  const SnapGuide();
}

/// A dashed alignment line (a shared left / right / top / bottom edge),
/// running through both boxes.
class SnapLineGuide extends SnapGuide {
  final Offset from;
  final Offset to;
  const SnapLineGuide(this.from, this.to);
}

/// A bracket measuring a gap, from one box's edge to the other's, labelled
/// with the gap (`50`) or, for equal spacing, `= 42`.
class SnapGapGuide extends SnapGuide {
  final Offset from;
  final Offset to;
  final String label;
  const SnapGapGuide(this.from, this.to, this.label);
}

/// A wire made straight by the snap, from pin to pin: drawn highlighted.
class SnapWireGuide extends SnapGuide {
  final Offset from;
  final Offset to;
  const SnapWireGuide(this.from, this.to);
}

/// A wire between the grabbed node and a node that stays put, for the
/// straight-wire snap. Both pins are known at drag start; the grabbed node's
/// is relative to its top-left corner, so it moves with the node.
class SnapWire {
  /// The other end's pin, scope-local.
  final Offset otherPin;

  /// The grabbed node's pin, relative to the grabbed node's top-left.
  final Offset grabbedPinOffset;

  const SnapWire({required this.otherPin, required this.grabbedPinOffset});
}

/// Where the grabbed node goes, and the guides that explain it. An axis that
/// did not snap keeps the raw (pointer-following) coordinate and its kind is
/// null.
class SnapResult {
  final Offset position;
  final SnapKind? xKind;
  final SnapKind? yKind;
  final List<SnapGuide> guides;
  const SnapResult(this.position, this.xKind, this.yKind, this.guides);
}

/// One snap target on one axis: [value] is the grabbed node's left (x) or
/// top (y) edge that the target asks for.
class _Target {
  final double value;
  final SnapKind kind;

  /// When set, the target only applies while the grabbed box overlaps this
  /// interval on the *other* axis — a gap or an equal spacing is measured to
  /// a neighbour, not to any box anywhere on screen.
  final double? crossLo;
  final double? crossHi;

  /// The guides, given the grabbed node's final box.
  final List<SnapGuide> Function(Rect grabbed) guides;

  const _Target(this.value, this.kind, this.guides,
      {this.crossLo, this.crossHi});
}

/// The snap targets of one drag. Built once, at the drag's first tick, from
/// the candidate boxes (scope siblings on screen, outside the selection) and
/// the grabbed node's wires; [resolve] is then a binary search per axis.
class SnapSession {
  final Size grabbedSize;

  /// Logical px corresponding to [SNAP_THRESHOLD_SCREEN_PX] at the zoom the
  /// session was built at.
  final double threshold;

  final List<_Target> _x;
  final List<_Target> _y;

  SnapSession._(this.grabbedSize, this.threshold, this._x, this._y);

  /// [candidates] are the boxes the grabbed node may snap to; [wires] its
  /// wires to nodes that stay put; [scale] the canvas zoom scale (screen px
  /// per logical px), which turns the screen threshold into a logical one.
  factory SnapSession({
    required Size grabbedSize,
    required List<Rect> candidates,
    List<SnapWire> wires = const [],
    required double scale,
  }) {
    final w = grabbedSize.width;
    final h = grabbedSize.height;
    final x = <_Target>[];
    final y = <_Target>[];

    for (final wire in wires) {
      y.add(_Target(
        wire.otherPin.dy - wire.grabbedPinOffset.dy,
        SnapKind.straightWire,
        (g) =>
            [SnapWireGuide(wire.otherPin, g.topLeft + wire.grabbedPinOffset)],
      ));
    }

    for (final c in candidates) {
      // Columns: left = left, right = right.
      x.add(_Target(c.left, SnapKind.edge, (g) => [_vLine(c.left, c, g)]));
      x.add(
          _Target(c.right - w, SnapKind.edge, (g) => [_vLine(c.right, c, g)]));
      // Rows: top = top, bottom = bottom.
      y.add(_Target(c.top, SnapKind.edge, (g) => [_hLine(c.top, c, g)]));
      y.add(_Target(
          c.bottom - h, SnapKind.edge, (g) => [_hLine(c.bottom, c, g)]));
      // The `normal` gap right of / below a neighbour.
      x.add(_Target(
        c.right + SNAP_GAP_ACROSS,
        SnapKind.gap,
        (g) => [_hBracket(c, g, _gapLabel(SNAP_GAP_ACROSS))],
        crossLo: c.top,
        crossHi: c.bottom,
      ));
      y.add(_Target(
        c.bottom + SNAP_GAP_DOWN,
        SnapKind.gap,
        (g) => [_vBracket(c, g, _gapLabel(SNAP_GAP_DOWN))],
        crossLo: c.left,
        crossHi: c.right,
      ));
    }

    // Equal spacing: each candidate and its nearest neighbour to the right
    // (below) among the candidates sharing its row (column) form a pair whose
    // gap the grabbed node can repeat on either end.
    for (final a in candidates) {
      final right = _nearestNeighbour(a, candidates, horizontal: true);
      if (right != null) {
        final b = right;
        final gap = b.left - a.right;
        final label = '= ${gap.round()}';
        x.add(_Target(
          b.right + gap,
          SnapKind.equalSpacing,
          (g) => [_hBracket(a, b, label), _hBracket(b, g, label)],
          crossLo: b.top,
          crossHi: b.bottom,
        ));
        x.add(_Target(
          a.left - gap - w,
          SnapKind.equalSpacing,
          (g) => [_hBracket(g, a, label), _hBracket(a, b, label)],
          crossLo: a.top,
          crossHi: a.bottom,
        ));
      }
      final below = _nearestNeighbour(a, candidates, horizontal: false);
      if (below != null) {
        final b = below;
        final gap = b.top - a.bottom;
        final label = '= ${gap.round()}';
        y.add(_Target(
          b.bottom + gap,
          SnapKind.equalSpacing,
          (g) => [_vBracket(a, b, label), _vBracket(b, g, label)],
          crossLo: b.left,
          crossHi: b.right,
        ));
        y.add(_Target(
          a.top - gap - h,
          SnapKind.equalSpacing,
          (g) => [_vBracket(g, a, label), _vBracket(a, b, label)],
          crossLo: a.left,
          crossHi: a.right,
        ));
      }
    }

    x.sort((p, q) => p.value.compareTo(q.value));
    y.sort((p, q) => p.value.compareTo(q.value));
    return SnapSession._(grabbedSize, SNAP_THRESHOLD_SCREEN_PX / scale, x, y);
  }

  /// Snap the grabbed node whose unsnapped top-left is [raw].
  SnapResult resolve(Offset raw) {
    final rawBox = raw & grabbedSize;
    final bx = _best(_x, raw.dx, rawBox.top, rawBox.bottom);
    final by = _best(_y, raw.dy, rawBox.left, rawBox.right);
    final position = Offset(bx?.value ?? raw.dx, by?.value ?? raw.dy);
    final box = position & grabbedSize;
    return SnapResult(position, bx?.kind, by?.kind, [
      if (bx != null) ...bx.guides(box),
      if (by != null) ...by.guides(box),
    ]);
  }

  /// The winning target within [threshold] of [raw]: lowest kind first, then
  /// nearest. [crossLo]..[crossHi] is the grabbed box on the other axis.
  _Target? _best(
      List<_Target> targets, double raw, double crossLo, double crossHi) {
    _Target? best;
    var bestDistance = double.infinity;
    for (var i = _lowerBound(targets, raw - threshold);
        i < targets.length && targets[i].value <= raw + threshold;
        i++) {
      final t = targets[i];
      if (t.crossLo != null &&
          !_overlaps(crossLo, crossHi, t.crossLo!, t.crossHi!)) {
        continue;
      }
      final distance = (t.value - raw).abs();
      if (best == null ||
          t.kind.index < best.kind.index ||
          (t.kind == best.kind && distance < bestDistance)) {
        best = t;
        bestDistance = distance;
      }
    }
    return best;
  }

  static int _lowerBound(List<_Target> targets, double value) {
    var lo = 0;
    var hi = targets.length;
    while (lo < hi) {
      final mid = (lo + hi) >> 1;
      if (targets[mid].value < value) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    return lo;
  }
}

/// The candidate nearest [a] to its right ([horizontal]) or below it, among
/// those sharing its row (column), with a positive gap between them.
Rect? _nearestNeighbour(Rect a, List<Rect> candidates,
    {required bool horizontal}) {
  Rect? nearest;
  for (final b in candidates) {
    if (identical(a, b)) continue;
    final double gap;
    if (horizontal) {
      if (!_overlaps(a.top, a.bottom, b.top, b.bottom)) continue;
      gap = b.left - a.right;
    } else {
      if (!_overlaps(a.left, a.right, b.left, b.right)) continue;
      gap = b.top - a.bottom;
    }
    if (gap <= 0) continue;
    final nearestGap = nearest == null
        ? double.infinity
        : horizontal
            ? nearest.left - a.right
            : nearest.top - a.bottom;
    if (gap < nearestGap) nearest = b;
  }
  return nearest;
}

bool _overlaps(double lo1, double hi1, double lo2, double hi2) =>
    lo1 < hi2 && lo2 < hi1;

/// The middle of two intervals' overlap, or of their centres when they do
/// not overlap: where a bracket between two boxes is drawn.
double _crossMid(double lo1, double hi1, double lo2, double hi2) {
  final lo = lo1 > lo2 ? lo1 : lo2;
  final hi = hi1 < hi2 ? hi1 : hi2;
  return lo < hi ? (lo + hi) / 2 : (lo1 + hi1 + lo2 + hi2) / 4;
}

String _gapLabel(double gap) => '${gap.round()}';

/// A vertical line at [x] spanning both boxes.
SnapLineGuide _vLine(double x, Rect a, Rect b) => SnapLineGuide(
    Offset(x, a.top < b.top ? a.top : b.top),
    Offset(x, a.bottom > b.bottom ? a.bottom : b.bottom));

/// A horizontal line at [y] spanning both boxes.
SnapLineGuide _hLine(double y, Rect a, Rect b) => SnapLineGuide(
    Offset(a.left < b.left ? a.left : b.left, y),
    Offset(a.right > b.right ? a.right : b.right, y));

/// A bracket across the horizontal gap between [left] and the box right of
/// it, [right].
SnapGapGuide _hBracket(Rect left, Rect right, String label) {
  final y = _crossMid(left.top, left.bottom, right.top, right.bottom);
  return SnapGapGuide(Offset(left.right, y), Offset(right.left, y), label);
}

/// A bracket across the vertical gap between [upper] and the box below it,
/// [lower].
SnapGapGuide _vBracket(Rect upper, Rect lower, String label) {
  final x = _crossMid(upper.left, upper.right, lower.left, lower.right);
  return SnapGapGuide(Offset(x, upper.bottom), Offset(x, lower.top), label);
}

/// Builds the session for a drag of [grabbedId] in [scopeChain] (empty = top
/// level) from the live view, or null when the node does not resolve.
///
/// Candidates are the scope's own nodes — comments included, body nodes
/// snapping to body siblings only — outside [movingIds] and with a box that
/// intersects [viewport] (screen px, canvas-local). The straight-wire targets
/// are the grabbed node's same-scope wires to such a candidate.
SnapSession? buildSnapSession({
  required ScopeResolver resolver,
  required List<BigInt> scopeChain,
  required BigInt grabbedId,
  required Set<BigInt> movingIds,
  required Rect viewport,
}) {
  final nodes = scopeChain.isEmpty
      ? resolver.root.nodes
      : resolver
          .findNodeInScope(
              scopeChain.sublist(0, scopeChain.length - 1), scopeChain.last)
          ?.zone
          ?.nodes;
  final grabbed = nodes?[grabbedId];
  if (nodes == null || grabbed == null) return null;
  final grabbedTopLeft = apiVec2ToOffset(grabbed.position);

  final candidates = <Rect>[];
  final candidateIds = <BigInt>{};
  for (final node in nodes.values) {
    if (movingIds.contains(node.id) || node.id == grabbedId) continue;
    final topLeft = apiVec2ToOffset(node.position);
    final box = topLeft & resolver.effectiveNodeSizeLogical(node, scopeChain);
    final screen = resolver.scopedToScreen(scopeChain, topLeft) &
        (box.size * resolver.scale);
    if (!screen.overlaps(viewport)) continue;
    candidates.add(box);
    candidateIds.add(node.id);
  }

  final wires = <SnapWire>[];
  final scopeWires = scopeChain.isEmpty
      ? resolver.root.wires
      : resolver
              .findNodeInScope(
                  scopeChain.sublist(0, scopeChain.length - 1), scopeChain.last)
              ?.zone
              ?.wires ??
          const <WireView>[];
  for (final wire in scopeWires) {
    // Same-scope wires between ordinary pins only: a capture, an iteration
    // value or a body return has its other end on the enclosing HOF.
    if (wire.sourceScopeDepth != 0 ||
        wire.destinationArgumentKind != APIArgumentKind.external_ ||
        wire.sourcePin is APISourcePin_ZoneInput) {
      continue;
    }
    final source = PinReference(
      nodeId: wire.sourceNodeId,
      scopeChain: scopeChain,
      pinKind: wire.sourceOutputPinIndex == -1
          ? PinKind.functionPin
          : PinKind.externalOutput,
      pinIndex: wire.sourceOutputPinIndex,
      dataType: '',
    );
    final dest = PinReference(
      nodeId: wire.destNodeId,
      scopeChain: scopeChain,
      pinKind: PinKind.externalInput,
      pinIndex: wire.destParamIndex.toInt(),
      dataType: '',
    );
    final PinReference mine;
    final PinReference other;
    if (wire.destNodeId == grabbedId &&
        candidateIds.contains(wire.sourceNodeId)) {
      mine = dest;
      other = source;
    } else if (wire.sourceNodeId == grabbedId &&
        candidateIds.contains(wire.destNodeId)) {
      mine = source;
      other = dest;
    } else {
      continue;
    }
    final minePos = resolver.tryPinScreenPosition(mine)?.$1;
    final otherPos = resolver.tryPinScreenPosition(other)?.$1;
    if (minePos == null || otherPos == null) continue;
    wires.add(SnapWire(
      otherPin: resolver.screenToScopedLocal(scopeChain, otherPos),
      grabbedPinOffset:
          resolver.screenToScopedLocal(scopeChain, minePos) - grabbedTopLeft,
    ));
  }

  return SnapSession(
    grabbedSize: resolver.effectiveNodeSizeLogical(grabbed, scopeChain),
    candidates: candidates,
    wires: wires,
    scale: resolver.scale,
  );
}
