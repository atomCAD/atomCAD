/// The placement tool's 2D **annotations**: the ring round the atom the user
/// asked about, and the leader line joining it to the list that is answering.
///
/// The ghost atoms used to be here too, painted as projected circles. They are
/// decorator geometry now (`MechanosynthGhostVisuals`, tessellated with the
/// workpiece), which is what the design asked for in the first place — Phase 4
/// drew them in Flutter to keep a *hover* preview free, and a preview taken on
/// a click can afford the evaluation. Three things were wrong with the overlay
/// that no amount of tuning fixes: it is not depth-tested, so a ghost behind an
/// atom draws in front of it; it is not shaded, so it does not read as an atom;
/// and a `CustomPainter` is not bounded by its widget, so ghosts projected
/// outside the viewport painted over the node editor and the property panel.
///
/// What stays here is annotation about the *question* rather than the answer,
/// and it is 2D on purpose: constant size, never occluded, drawn even when the
/// anchor atom is hidden behind the workpiece.
///
/// This file also owns the popup's **layout** arithmetic, which is pure and
/// unit-tested (`test/mechanosynth_popup_layout_test.dart`).
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';

/// One ghost atom, already projected into viewport coordinates.
class ProjectedGhost {
  /// `"added"` / `"deleted"` / `"moved"` / `"changed"`.
  final String kind;
  final Offset position;

  /// The tail of a moved atom's arrow; equal to [position] otherwise.
  final Offset from;

  /// The circle's radius in pixels, from projecting a fixed world radius at the
  /// atom's own depth — so a ghost shrinks with distance like the atoms it sits
  /// among.
  final double radius;

  const ProjectedGhost({
    required this.kind,
    required this.position,
    required this.from,
    required this.radius,
  });
}

class MechanosynthGhostPainter extends CustomPainter {
  /// The clicked atom, ringed as the **query anchor**. Without it the popup is
  /// the only thing on screen saying which atom was asked about, and the popup
  /// is placed clear of the reaction precisely so that it is *not* on the atom
  /// — so the ring is what keeps the question visible while the answer is read.
  final Offset? anchor;

  /// Where the popup is, when it is far enough from the anchor to want a line
  /// drawn to it.
  final Offset? leaderEnd;

  const MechanosynthGhostPainter({
    this.anchor,
    this.leaderEnd,
  });

  static const Color _anchorColor = Color(0xFFFFB74D);

  @override
  void paint(Canvas canvas, Size size) {
    final a = anchor;
    if (a != null) {
      final end = leaderEnd;
      if (end != null && (end - a).distance > MS_ANCHOR_RING_RADIUS + 8) {
        _dashedLine(
            canvas,
            a,
            end,
            Paint()
              ..color = _anchorColor.withValues(alpha: 0.55)
              ..strokeWidth = 1.2);
      }
      canvas.drawCircle(
          a,
          MS_ANCHOR_RING_RADIUS,
          Paint()
            ..color = _anchorColor
            ..style = PaintingStyle.stroke
            ..strokeWidth = 2.0);
    }
  }

  /// A leader line, so it reads as an annotation rather than as a bond.
  void _dashedLine(Canvas canvas, Offset from, Offset to, Paint paint) {
    const dash = 5.0;
    const gap = 4.0;
    final total = (to - from).distance;
    if (total < 1) return;
    final step = (to - from) / total;
    var t = MS_ANCHOR_RING_RADIUS;
    while (t < total) {
      final end = math.min(t + dash, total);
      canvas.drawLine(from + step * t, from + step * end, paint);
      t = end + gap;
    }
  }

  /// Both fields are re-projected on every build, so identity says nothing and
  /// the comparison has to be by value. Both are one `Offset`.
  @override
  bool shouldRepaint(MechanosynthGhostPainter oldDelegate) =>
      anchor != oldDelegate.anchor || leaderEnd != oldDelegate.leaderEnd;
}

// ---------------------------------------------------------------------------
// Layout: where the popup goes, and what marks the atom it belongs to.
//
// Pure functions over screen rectangles, kept here rather than in the viewport
// so they are decidable without a kernel, a camera or a widget tree — the
// placement rule is the part of this tool that was actually wrong in use, and
// it is the part worth pinning in a test.
// ---------------------------------------------------------------------------

/// The radius of the ring drawn round the clicked atom.
const double MS_ANCHOR_RING_RADIUS = 16.0;

/// Gap between the reaction and the popup, and the viewport margin the popup is
/// clamped to.
const double MS_POPUP_GAP = 28.0;
const double MS_POPUP_MARGIN = 4.0;

/// What the popup must stay clear of this frame: the clicked atom and
/// everything the highlighted row would do to it.
///
/// Placing the list relative to *this* rather than to the anchor alone is the
/// difference between "beside the atom" and "on top of the reaction". A
/// `precursor_chemisorb` preview is six added atoms spread over two dimers, and
/// a list pinned 18 px off the host lands squarely in the middle of them.
Rect mechanosynthActionBox({
  required Offset? anchor,
  required List<ProjectedGhost> ghosts,
  required Offset fallbackCentre,
}) {
  Rect? box;
  if (anchor != null) {
    box = Rect.fromCircle(center: anchor, radius: MS_ANCHOR_RING_RADIUS);
  }
  for (final ghost in ghosts) {
    final r = Rect.fromCircle(center: ghost.position, radius: ghost.radius);
    box = box == null ? r : box.expandToInclude(r);
  }
  return box ?? Rect.fromCircle(center: fallbackCentre, radius: 1);
}

/// Picks the side of [box] with room for the popup, applies the user's [drag]
/// and clamps into the viewport.
///
/// Right first, because the list reads left-to-right and a right-hand list
/// leaves the workpiece the larger half of the viewport; then left, below,
/// above. The first side that ends up clear of [box] wins, and if the reaction
/// fills the viewport the least-overlapping side does — with the ring and the
/// leader line still saying what belongs to what, and the header still
/// draggable.
Offset mechanosynthPopupPosition({
  required Rect box,
  required Size size,
  required Size viewportSize,
  Offset drag = Offset.zero,
}) {
  final viewport = _viewportRect(viewportSize);

  final centredY = box.center.dy - size.height / 2;
  final centredX = box.center.dx - size.width / 2;
  final options = <Offset>[
    Offset(box.right + MS_POPUP_GAP, centredY),
    Offset(box.left - MS_POPUP_GAP - size.width, centredY),
    Offset(centredX, box.bottom + MS_POPUP_GAP),
    Offset(centredX, box.top - MS_POPUP_GAP - size.height),
  ];

  Offset? best;
  var bestOverlap = double.infinity;
  for (final option in options) {
    final placed = _clampInto(option + drag, size, viewport);
    final overlap = (placed & size).intersect(box);
    final area = overlap.isEmpty ? 0.0 : overlap.width * overlap.height;
    if (area == 0.0) return placed;
    if (area < bestOverlap) {
      bestOverlap = area;
      best = placed;
    }
  }
  return best ?? _clampInto(options.first + drag, size, viewport);
}

/// Keeps a popup of [size] wholly inside a viewport of [viewportSize], minus
/// the standard margin. Public because the header drag has to clamp against
/// the *same* rule the placement uses: a drag that stores the raw pointer
/// delta and clamps only on the way out accumulates invisible slack past the
/// edge, and the list then refuses to move until the user has dragged all of it
/// back — which looks like one axis being stuck.
Offset mechanosynthClampInto(Offset position, Size size, Size viewportSize) =>
    _clampInto(position, size, _viewportRect(viewportSize));

Rect _viewportRect(Size viewportSize) => Rect.fromLTWH(
      MS_POPUP_MARGIN,
      MS_POPUP_MARGIN,
      math.max(1.0, viewportSize.width - 2 * MS_POPUP_MARGIN),
      math.max(1.0, viewportSize.height - 2 * MS_POPUP_MARGIN),
    );

Offset _clampInto(Offset position, Size size, Rect viewport) => Offset(
      position.dx
          .clamp(viewport.left,
              math.max(viewport.left, viewport.right - size.width))
          .toDouble(),
      position.dy
          .clamp(viewport.top,
              math.max(viewport.top, viewport.bottom - size.height))
          .toDouble(),
    );

/// One frame's worth of placement-tool geometry, so the popup, the ring and the
/// leader line cannot disagree about where anything is.
class MechanosynthLayout {
  /// The clicked atom, projected. Null when it is behind the camera.
  final Offset? anchor;

  /// The **highlighted** row's ghosts — what is drawn. Not what the placement
  /// avoids: that is every row's, so the list holds still while the highlight
  /// moves.
  final List<ProjectedGhost> ghosts;
  final Rect rect;

  /// Carried so the header drag can clamp against the same viewport the
  /// placement did, without re-deriving it from stale constraints.
  final Size viewportSize;

  const MechanosynthLayout({
    required this.anchor,
    required this.ghosts,
    required this.rect,
    this.viewportSize = Size.zero,
  });

  /// Where the leader line meets the popup: the middle of the edge facing the
  /// anchor, so the line reads as pointing at the list rather than through it.
  Offset? get leaderEnd {
    final a = anchor;
    if (a == null) return null;
    if (a.dx < rect.left) return Offset(rect.left, rect.center.dy);
    if (a.dx > rect.right) return Offset(rect.right, rect.center.dy);
    if (a.dy < rect.top) return Offset(rect.center.dx, rect.top);
    if (a.dy > rect.bottom) return Offset(rect.center.dx, rect.bottom);
    return null; // the anchor is under the popup; the ring is hidden anyway
  }
}
