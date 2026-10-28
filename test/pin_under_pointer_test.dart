/// Widget test for [pinUnderPointer]
/// (`lib/structure_designer/node_network/edge_pan.dart`,
/// `doc/design_move_wires.md` Phase 3). It depends on Flutter's hit testing,
/// so it runs against real widgets laid out as on the canvas — `MetaData`
/// hit boxes under a scaled, translated `Stack` — not re-derived geometry.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/structure_designer/node_network/edge_pan.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

const double boxW = 20;
const double boxH = 24;
const double scale = 0.6;
const Offset pan = Offset(30, 40);

PinReference pin(int nodeId) => PinReference(
      nodeId: BigInt.from(nodeId),
      scopeChain: const [],
      pinKind: PinKind.externalInput,
      pinIndex: 0,
      dataType: 'Float',
    );

/// A pin hit box as `PinWidget` builds it: a translucent `MetaData` around
/// a `DragTarget` whose child paints nothing in most of the box.
Widget hitBox(PinReference ref) => MetaData(
      metaData: ref,
      behavior: HitTestBehavior.translucent,
      child: SizedBox(
        width: boxW,
        height: boxH,
        child: DragTarget<PinReference>(
          builder: (context, _, __) =>
              const Center(child: SizedBox(width: 4, height: 4)),
        ),
      ),
    );

/// The logical (pre-scale) top-left of each built box.
const boxes = {1: Offset(100, 50), 2: Offset(300, 200)};

Offset screen(Offset logical) => (logical + pan) * scale;

Future<int> pumpCanvas(WidgetTester tester, {bool buildPin2 = true}) async {
  await tester.pumpWidget(Directionality(
    textDirection: TextDirection.ltr,
    child: Align(
      alignment: Alignment.topLeft,
      child: SizedBox(
        width: 600,
        height: 400,
        child: Stack(children: [
          for (final e in boxes.entries)
            if (e.key != 2 || buildPin2)
              Positioned(
                left: screen(e.value).dx,
                top: screen(e.value).dy,
                child: Transform.scale(
                  scale: scale,
                  alignment: Alignment.topLeft,
                  child: hitBox(pin(e.key)),
                ),
              ),
        ]),
      ),
    ),
  ));
  return tester.view.viewId;
}

void main() {
  testWidgets('a point just inside each corner finds the pin', (tester) async {
    final viewId = await pumpCanvas(tester);
    for (final e in boxes.entries) {
      final topLeft = screen(e.value);
      final w = boxW * scale, h = boxH * scale;
      for (final corner in [
        topLeft + const Offset(0.5, 0.5),
        topLeft + Offset(w - 0.5, 0.5),
        topLeft + Offset(0.5, h - 0.5),
        topLeft + Offset(w - 0.5, h - 0.5),
      ]) {
        expect(pinUnderPointer(corner, viewId)?.nodeId, BigInt.from(e.key),
            reason: 'pin ${e.key} at $corner');
      }
    }
  });

  testWidgets('a point just outside finds none', (tester) async {
    final viewId = await pumpCanvas(tester);
    final topLeft = screen(boxes[1]!);
    final w = boxW * scale, h = boxH * scale;
    for (final p in [
      topLeft + const Offset(-0.5, 5),
      topLeft + Offset(w + 0.5, 5),
      topLeft + const Offset(5, -0.5),
      topLeft + Offset(5, h + 0.5),
    ]) {
      expect(pinUnderPointer(p, viewId), isNull, reason: '$p');
    }
  });

  testWidgets('a box that is not built is never found', (tester) async {
    final viewId = await pumpCanvas(tester, buildPin2: false);
    final centre = screen(boxes[2]!) + const Offset(boxW, boxH) * scale / 2;
    expect(pinUnderPointer(centre, viewId), isNull);
  });

  testWidgets('the DragTarget inside does not confuse it', (tester) async {
    final viewId = await pumpCanvas(tester);
    final centre = screen(boxes[1]!) + const Offset(boxW, boxH) * scale / 2;
    final found = pinUnderPointer(centre, viewId);
    expect(found, isA<PinReference>());
    expect(found!.nodeId, BigInt.one);
  });
}
