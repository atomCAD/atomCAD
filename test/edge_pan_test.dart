/// Tests for the pure arithmetic and release rule of edge auto-pan
/// (`lib/structure_designer/node_network/edge_pan.dart`,
/// `doc/design_move_wires.md` Phase 3). The ticker in `NodeNetworkState`
/// only calls these, so a sign error — the likely bug, and invisible to the
/// walkthrough at one edge — has to show up here.
library;

import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/structure_designer/node_network/edge_pan.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

const size = Size(800, 600);

PinReference pin(int nodeId,
        {PinKind kind = PinKind.externalInput,
        int index = 0,
        List<int> scope = const []}) =>
    PinReference(
      nodeId: BigInt.from(nodeId),
      scopeChain: scope.map(BigInt.from).toList(),
      pinKind: kind,
      pinIndex: index,
      dataType: 'Float',
    );

void main() {
  group('edgePanVelocity', () {
    test('is zero in the interior and at the band boundary', () {
      expect(edgePanVelocity(const Offset(400, 300), size), Offset.zero);
      expect(
          edgePanVelocity(const Offset(EDGE_PAN_BAND, 300), size), Offset.zero);
      expect(
          edgePanVelocity(
              const Offset(800 - EDGE_PAN_BAND, 600 - EDGE_PAN_BAND), size),
          Offset.zero);
    });

    test('points toward each edge, on that axis only', () {
      final left = edgePanVelocity(const Offset(5, 300), size);
      expect(left.dx, lessThan(0));
      expect(left.dy, 0);
      final right = edgePanVelocity(const Offset(795, 300), size);
      expect(right.dx, greaterThan(0));
      expect(right.dy, 0);
      final top = edgePanVelocity(const Offset(400, 5), size);
      expect(top.dy, lessThan(0));
      expect(top.dx, 0);
      final bottom = edgePanVelocity(const Offset(400, 595), size);
      expect(bottom.dy, greaterThan(0));
      expect(bottom.dx, 0);
    });

    test('a corner pans both axes at once', () {
      for (final (corner, sx, sy) in [
        (const Offset(2, 2), -1, -1),
        (const Offset(798, 2), 1, -1),
        (const Offset(2, 598), -1, 1),
        (const Offset(798, 598), 1, 1),
      ]) {
        final v = edgePanVelocity(corner, size);
        expect(v.dx.sign, sx, reason: '$corner');
        expect(v.dy.sign, sy, reason: '$corner');
        expect(v.dx.abs(), v.dy.abs(), reason: '$corner');
      }
    });

    test('grows with depth into the band', () {
      final shallow = edgePanVelocity(const Offset(20, 300), size).dx.abs();
      final deep = edgePanVelocity(const Offset(5, 300), size).dx.abs();
      final atEdge = edgePanVelocity(const Offset(0, 300), size).dx.abs();
      expect(shallow, greaterThan(0));
      expect(deep, greaterThan(shallow));
      expect(atEdge, greaterThan(deep));
      expect(atEdge, EDGE_PAN_MAX_SPEED);
    });

    test('reaches the cap at the edge and holds it past it', () {
      expect(edgePanVelocity(const Offset(-EDGE_PAN_OVERSHOOT, 300), size).dx,
          -EDGE_PAN_MAX_SPEED);
      expect(edgePanVelocity(const Offset(-500, 300), size).dx,
          -EDGE_PAN_MAX_SPEED);
      expect(edgePanVelocity(const Offset(2000, 300), size).dx,
          EDGE_PAN_MAX_SPEED);
      expect(edgePanVelocity(const Offset(400, -900), size).dy,
          -EDGE_PAN_MAX_SPEED);
    });

    test('opposite edges are symmetric', () {
      for (final d in [-40.0, -5.0, 0.0, 3.0, 12.0, 29.0]) {
        final left = edgePanVelocity(Offset(d, 300), size);
        final right = edgePanVelocity(Offset(800 - d, 300), size);
        expect(right.dx, closeTo(-left.dx, 1e-9), reason: 'depth $d');
        final top = edgePanVelocity(Offset(400, d), size);
        final bottom = edgePanVelocity(Offset(400, 600 - d), size);
        expect(bottom.dy, closeTo(-top.dy, 1e-9), reason: 'depth $d');
      }
    });
  });

  group('edgePanStep', () {
    test('at the right edge: content left, node right, rect start left', () {
      final step = edgePanStep(const Offset(600, 0), 0.1, 1.0);
      // screen = (logical + pan) * scale: a smaller pan moves content left.
      expect(step.panDelta, const Offset(-60, 0));
      expect(step.nodeDelta, const Offset(60, 0));
      expect(step.rectStartShift, const Offset(-60, 0));
    });

    test('at the top-left corner every sign flips', () {
      final step = edgePanStep(const Offset(-300, -300), 0.1, 1.0);
      expect(step.panDelta, const Offset(30, 30));
      expect(step.nodeDelta, const Offset(-30, -30));
      expect(step.rectStartShift, const Offset(30, 30));
    });

    test('the dragged node stays under a still pointer', () {
      const scale = 0.6;
      const pan = Offset(10, 20);
      const node = Offset(100, 50);
      final step = edgePanStep(const Offset(500, -200), 1 / 60, scale);
      final before = (node + pan) * scale;
      final after = (node + step.nodeDelta + pan + step.panDelta) * scale;
      expect((after - before).distance, closeTo(0, 1e-9));
    });

    test('the rectangle start moves with the content', () {
      const scale = 0.35;
      const pan = Offset(-40, 7);
      const contentPoint = Offset(200, 300);
      final step = edgePanStep(const Offset(-700, 400), 1 / 60, scale);
      final before = (contentPoint + pan) * scale;
      final after = (contentPoint + pan + step.panDelta) * scale;
      expect((after - before - step.rectStartShift).distance, closeTo(0, 1e-9));
    });

    test('the logical step is the screen step over the scale', () {
      final step = edgePanStep(const Offset(700, 0), 0.1, 0.35);
      expect(step.nodeDelta.dx, closeTo(70 / 0.35, 1e-9));
      expect(step.rectStartShift.dx, closeTo(-70, 1e-9));
    });
  });

  group('releaseDecision', () {
    final a = pin(1);
    final b = pin(2);

    test('outside the canvas the release is always ignored', () {
      for (final panned in [false, true]) {
        for (final (chosen, under) in [(null, null), (a, a), (a, null)]) {
          expect(
              releaseDecision(
                  insideCanvas: false,
                  pannedSinceMove: panned,
                  chosenPin: chosen,
                  pinUnderPointer: under),
              ReleaseDecision.ignore);
        }
      }
    });

    test('inside with no pan it proceeds, whatever the pins', () {
      for (final (chosen, under) in [
        (null, null),
        (a, b),
        (a, null),
        (null, b)
      ]) {
        expect(
            releaseDecision(
                insideCanvas: true,
                pannedSinceMove: false,
                chosenPin: chosen,
                pinUnderPointer: under),
            ReleaseDecision.proceed);
      }
    });

    test('after a pan it proceeds only when the pins agree', () {
      ReleaseDecision after(PinReference? chosen, PinReference? under) =>
          releaseDecision(
              insideCanvas: true,
              pannedSinceMove: true,
              chosenPin: chosen,
              pinUnderPointer: under);
      expect(after(a, pin(1)), ReleaseDecision.proceed);
      expect(after(null, null), ReleaseDecision.proceed);
      expect(after(a, b), ReleaseDecision.ignore);
      expect(after(a, null), ReleaseDecision.ignore);
      expect(after(null, a), ReleaseDecision.ignore);
    });

    test('pins are told apart by scope, kind and index', () {
      ReleaseDecision after(PinReference chosen, PinReference under) =>
          releaseDecision(
              insideCanvas: true,
              pannedSinceMove: true,
              chosenPin: chosen,
              pinUnderPointer: under);
      expect(after(pin(1), pin(1, scope: [7])), ReleaseDecision.ignore);
      expect(after(pin(1), pin(1, kind: PinKind.externalOutput)),
          ReleaseDecision.ignore);
      expect(after(pin(1), pin(1, index: 1)), ReleaseDecision.ignore);
    });
  });
}
