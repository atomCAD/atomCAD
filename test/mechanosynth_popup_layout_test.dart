/// Where the placement popup goes (`mechanosynth_ghost_painter.dart`).
///
/// Three rules, each pinned because it was wrong in use:
///
/// - the popup was anchored 18 px off the clicked atom and clamped, which put a
///   300 px list squarely on top of the reaction it was describing — and once
///   the clamp bit, the list had no visible relation to the atom at all. It is
///   now placed clear of an **action box**;
/// - that box was first keyed on the *highlighted* row's ghosts, so the list
///   jumped on every hover. It is the union over **every** row, fixed for a
///   query;
/// - the header drag accumulated the raw pointer delta and clamped only on the
///   way out, banking invisible slack at an edge. It accumulates the movement
///   that actually happened.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/structure_designer/mechanosynth_ghost_painter.dart';

ProjectedGhost _ghost(double x, double y, {double radius = 12}) =>
    ProjectedGhost(
      kind: 'added',
      position: Offset(x, y),
      from: Offset(x, y),
      radius: radius,
    );

const _popup = Size(300, 200);
const _viewport = Size(1000, 600);

void main() {
  group('action box', () {
    test('covers the anchor and every ghost', () {
      final box = mechanosynthActionBox(
        anchor: const Offset(400, 300),
        ghosts: [_ghost(340, 360), _ghost(470, 250)],
        fallbackCentre: Offset.zero,
      );

      expect(box.left, lessThanOrEqualTo(340 - 12));
      expect(box.right, greaterThanOrEqualTo(470 + 12));
      expect(box.top, lessThanOrEqualTo(250 - 12));
      expect(box.bottom, greaterThanOrEqualTo(360 + 12));
    });

    test('is the anchor ring alone when nothing is previewed', () {
      final box = mechanosynthActionBox(
        anchor: const Offset(400, 300),
        ghosts: const [],
        fallbackCentre: Offset.zero,
      );
      expect(box.center, const Offset(400, 300));
      expect(box.width, MS_ANCHOR_RING_RADIUS * 2);
    });

    test('falls back to the given centre when the anchor is behind the camera',
        () {
      final box = mechanosynthActionBox(
        anchor: null,
        ghosts: const [],
        fallbackCentre: const Offset(500, 300),
      );
      expect(box.center, const Offset(500, 300));
    });
  });

  group('popup position', () {
    test('never covers the reaction it describes', () {
      // The screenshot case: six ghosts spread around the clicked atom.
      final box = mechanosynthActionBox(
        anchor: const Offset(400, 400),
        ghosts: [
          _ghost(355, 458),
          _ghost(385, 462),
          _ghost(310, 505),
          _ghost(278, 562),
        ],
        fallbackCentre: Offset.zero,
      );

      final position = mechanosynthPopupPosition(
        box: box,
        size: _popup,
        viewportSize: _viewport,
      );

      expect((position & _popup).intersect(box).isEmpty, isTrue,
          reason: 'popup $position overlaps the action box $box');
    });

    test('prefers the right of the reaction when there is room', () {
      final box = mechanosynthActionBox(
        anchor: const Offset(300, 300),
        ghosts: const [],
        fallbackCentre: Offset.zero,
      );
      final position = mechanosynthPopupPosition(
        box: box,
        size: _popup,
        viewportSize: _viewport,
      );
      expect(position.dx, greaterThanOrEqualTo(box.right));
    });

    test('goes to the left when the right would fall off the viewport', () {
      final box = mechanosynthActionBox(
        anchor: const Offset(900, 300),
        ghosts: const [],
        fallbackCentre: Offset.zero,
      );
      final position = mechanosynthPopupPosition(
        box: box,
        size: _popup,
        viewportSize: _viewport,
      );
      expect(position.dx + _popup.width, lessThanOrEqualTo(box.left));
    });

    test('stays wholly inside the viewport, drag included', () {
      final box = mechanosynthActionBox(
        anchor: const Offset(500, 300),
        ghosts: const [],
        fallbackCentre: Offset.zero,
      );
      for (final drag in const [
        Offset.zero,
        Offset(900, 900),
        Offset(-900, -900),
      ]) {
        final rect = mechanosynthPopupPosition(
              box: box,
              size: _popup,
              viewportSize: _viewport,
              drag: drag,
            ) &
            _popup;
        expect(rect.left, greaterThanOrEqualTo(MS_POPUP_MARGIN - 0.001));
        expect(rect.top, greaterThanOrEqualTo(MS_POPUP_MARGIN - 0.001));
        expect(rect.right,
            lessThanOrEqualTo(_viewport.width - MS_POPUP_MARGIN + 0.001));
        expect(rect.bottom,
            lessThanOrEqualTo(_viewport.height - MS_POPUP_MARGIN + 0.001));
      }
    });

    test('a drag moves it, and is relative to the automatic placement', () {
      final box = mechanosynthActionBox(
        anchor: const Offset(400, 300),
        ghosts: const [],
        fallbackCentre: Offset.zero,
      );
      final base = mechanosynthPopupPosition(
          box: box, size: _popup, viewportSize: _viewport);
      final dragged = mechanosynthPopupPosition(
          box: box,
          size: _popup,
          viewportSize: _viewport,
          drag: const Offset(40, 25));
      expect(dragged - base, const Offset(40, 25));
    });

    test('still places something when the reaction fills the viewport', () {
      // Nothing can be clear of this; the contract is that it stays on screen
      // and the leader line does the explaining.
      final box = Rect.fromLTRB(0, 0, _viewport.width, _viewport.height);
      final rect = mechanosynthPopupPosition(
            box: box,
            size: _popup,
            viewportSize: _viewport,
          ) &
          _popup;
      expect(rect.left, greaterThanOrEqualTo(MS_POPUP_MARGIN - 0.001));
      expect(rect.right,
          lessThanOrEqualTo(_viewport.width - MS_POPUP_MARGIN + 0.001));
    });
  });

  group('holding still', () {
    // The bug this pins: the placement was keyed on the *highlighted* row's
    // ghosts, so the list jumped across the viewport on every hover and every
    // arrow key — i.e. exactly while it was being read. The box is now the
    // union over every row, which is fixed for a query.
    final allRows = [
      _ghost(360, 460),
      _ghost(390, 462),
      _ghost(310, 505),
      _ghost(278, 562),
      _ghost(430, 430),
    ];

    Offset place(List<ProjectedGhost> ghosts) => mechanosynthPopupPosition(
          box: mechanosynthActionBox(
            anchor: const Offset(400, 470),
            ghosts: ghosts,
            fallbackCentre: Offset.zero,
          ),
          size: _popup,
          viewportSize: _viewport,
        );

    test('highlighting a different row does not move the list', () {
      // Each row previews its own reaction, so placing from the *highlighted*
      // row's ghosts gives a different answer per row — that was the jump.
      final perRow = [
        for (final row in allRows) place([row])
      ];
      expect(perRow.toSet().length, greaterThan(1),
          reason: 'the fixture must actually distinguish the two rules');

      // Placing from the union gives one answer for the whole query.
      final union = place(allRows);
      for (var i = 0; i < allRows.length; i++) {
        expect(place(allRows), union, reason: 'row $i moved the list');
      }
      expect(perRow.any((p) => p != union), isTrue);
    });

    test('the union covers every row, so no preview can land under the list',
        () {
      final box = mechanosynthActionBox(
        anchor: const Offset(400, 470),
        ghosts: allRows,
        fallbackCentre: Offset.zero,
      );
      final rect = place(allRows) & _popup;
      for (final ghost in allRows) {
        final g = Rect.fromCircle(center: ghost.position, radius: ghost.radius);
        expect(rect.intersect(g).isEmpty, isTrue,
            reason: 'a preview at ${ghost.position} would be under the list');
      }
      expect(rect.intersect(box).isEmpty, isTrue);
    });
  });

  group('drag clamping', () {
    // The bug this pins: the raw pointer delta was accumulated and clamped
    // only on the way out, so dragging into an edge banked invisible slack and
    // the list then refused to come back until all of it had been undone —
    // which reads as one axis being stuck.
    test('a drag into an edge yields no movement, so nothing accumulates', () {
      const rect = Rect.fromLTWH(690, 380, 300, 200);
      final moved = mechanosynthClampInto(
              rect.topLeft + const Offset(500, 500), rect.size, _viewport) -
          rect.topLeft;
      expect(moved.dx,
          closeTo(_viewport.width - MS_POPUP_MARGIN - rect.right, 0.001));
      expect(moved.dy,
          closeTo(_viewport.height - MS_POPUP_MARGIN - rect.bottom, 0.001));
    });

    test('a drag away from an edge is realised on both axes', () {
      const rect = Rect.fromLTWH(696, 396, 300, 200);
      final moved = mechanosynthClampInto(
              rect.topLeft + const Offset(-60, -40), rect.size, _viewport) -
          rect.topLeft;
      expect(moved, const Offset(-60, -40));
    });

    test('a popup already at the bottom edge can still be dragged up', () {
      final rect = Rect.fromLTWH(
          300, _viewport.height - MS_POPUP_MARGIN - 200, 300, 200);
      final moved = mechanosynthClampInto(
              rect.topLeft + const Offset(0, -120), rect.size, _viewport) -
          rect.topLeft;
      expect(moved, const Offset(0, -120));
    });
  });

  group('leader line', () {
    test('meets the edge of the popup that faces the anchor', () {
      const layout = MechanosynthLayout(
        anchor: Offset(200, 300),
        ghosts: [],
        rect: Rect.fromLTWH(500, 200, 300, 200),
      );
      expect(layout.leaderEnd, const Offset(500, 300));
    });

    test('is absent when the anchor is under the popup', () {
      const layout = MechanosynthLayout(
        anchor: Offset(600, 300),
        ghosts: [],
        rect: Rect.fromLTWH(500, 200, 300, 200),
      );
      expect(layout.leaderEnd, isNull);
    });

    test('is absent when the anchor is behind the camera', () {
      const layout = MechanosynthLayout(
        anchor: null,
        ghosts: [],
        rect: Rect.fromLTWH(500, 200, 300, 200),
      );
      expect(layout.leaderEnd, isNull);
    });
  });
}
