/// Tests for the pure helpers of the move-wires drag
/// (`lib/structure_designer/node_network/wire_move.dart`,
/// `doc/design_move_wires.md` Phase 2), over hand-built API data classes —
/// no Rust library. What the painters draw comes from [wireMoveSegments]
/// alone, so only how it looks is left to the walkthrough.
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_painter.dart';
import 'package:flutter_cad/structure_designer/node_network/wire_move.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

BigInt b(int v) => BigInt.from(v);

PinReference pin(int nodeId, PinKind kind, int index,
        {List<int> scope = const [], String dataType = ''}) =>
    PinReference(
      nodeId: b(nodeId),
      scopeChain: scope.map(b).toList(),
      pinKind: kind,
      pinIndex: index,
      dataType: dataType,
    );

WireView wireView(int source, int dest, int slot,
        {APIArgumentKind kind = APIArgumentKind.external_,
        int sourceScopeDepth = 0}) =>
    WireView(
      sourceNodeId: b(source),
      sourceOutputPinIndex: 0,
      destNodeId: b(dest),
      destParamIndex: b(slot),
      selected: false,
      destinationArgumentKind: kind,
      sourcePin: const APISourcePin.nodeOutput(pinIndex: 0),
      sourceScopeDepth: sourceScopeDepth,
    );

APIWireEnd wireEnd(
        {required List<int> storageScope,
        required int source,
        required int dest,
        required int slot,
        APIArgumentKind kind = APIArgumentKind.external_,
        required PinReference fixedEnd}) =>
    APIWireEnd(
      storageScope: scopeChainToBytes(storageScope.map(b).toList()),
      anchor: APIWireAnchor(
        destinationNodeId: b(dest),
        destinationArgumentKind: kind,
        destinationArgumentIndex: b(slot),
        destinationParamId: b(77),
        sourceNodeId: b(source),
      ),
      fixedEnd: pinAddressOf(fixedEnd),
    );

void main() {
  group('pinKey', () {
    final base = pin(5, PinKind.externalInput, 1, scope: [3]);

    test('equal for the same pin, whatever its data type', () {
      expect(
          pinKey(pin(5, PinKind.externalInput, 1, scope: [3], dataType: 'Int')),
          pinKey(base));
    });
    test('differs by scope chain alone', () {
      expect(pinKey(pin(5, PinKind.externalInput, 1)), isNot(pinKey(base)));
      expect(pinKey(pin(5, PinKind.externalInput, 1, scope: [4])),
          isNot(pinKey(base)));
    });
    test('differs by pin kind alone', () {
      expect(pinKey(pin(5, PinKind.zoneOutput, 1, scope: [3])),
          isNot(pinKey(base)));
      expect(pinKey(pin(5, PinKind.externalOutput, 1, scope: [3])),
          isNot(pinKey(base)));
    });
    test('differs by index alone', () {
      expect(pinKey(pin(5, PinKind.externalInput, 2, scope: [3])),
          isNot(pinKey(base)));
    });
    test('scope [1, 23] and [12, 3] do not collide', () {
      expect(pinKey(pin(5, PinKind.externalInput, 1, scope: [1, 23])),
          isNot(pinKey(pin(5, PinKind.externalInput, 1, scope: [12, 3]))));
    });
  });

  group('heldWireKey matches WireHitResult.keyOf', () {
    test('top-level wire', () {
      final end = wireEnd(
          storageScope: [],
          source: 1,
          dest: 2,
          slot: 0,
          fixedEnd: pin(1, PinKind.externalOutput, 0));
      expect(heldWireKey(end), WireHitResult.keyOf([], wireView(1, 2, 0)));
    });

    test('body wire', () {
      final end = wireEnd(
          storageScope: [9],
          source: 1,
          dest: 2,
          slot: 3,
          fixedEnd: pin(1, PinKind.externalOutput, 0, scope: [9]));
      expect(heldWireKey(end), WireHitResult.keyOf([b(9)], wireView(1, 2, 3)));
      // The same ids at the top level are another wire.
      expect(
          heldWireKey(end), isNot(WireHitResult.keyOf([], wireView(1, 2, 3))));
    });

    test('body-return wire: listed under the body, stored on the HOF', () {
      // Body node 1 of HOF 9 returns into 9's zone-output pin 0. The painter
      // lists the wire under the body [9]; Rust stores it in 9's own scope.
      final end = wireEnd(
          storageScope: [],
          source: 1,
          dest: 9,
          slot: 0,
          kind: APIArgumentKind.zoneOutput,
          fixedEnd: pin(1, PinKind.externalOutput, 0, scope: [9]));
      expect(
          heldWireKey(end),
          WireHitResult.keyOf(
              [b(9)], wireView(1, 9, 0, kind: APIArgumentKind.zoneOutput)));
    });
  });

  group('pinAddressOf / pinReferenceOf', () {
    for (final p in [
      pin(4, PinKind.externalInput, 2, scope: [1, 2]),
      pin(4, PinKind.zoneOutput, 0, scope: [1]),
      pin(4, PinKind.externalOutput, 1),
      pin(4, PinKind.functionPin, -1, scope: [3]),
      pin(4, PinKind.zoneInput, 1),
    ]) {
      test('round trip of ${p.pinKind.name}', () {
        expect(pinKey(pinReferenceOf(pinAddressOf(p))), pinKey(p));
      });
    }
  });

  group('wireMoveSegments', () {
    // Pin positions: each pin sits at (nodeId * 100, pinIndex * 10), and its
    // data type is named after its node.
    (Offset, String)? positions(PinReference p) =>
        (Offset(p.nodeId.toInt() * 100.0, p.pinIndex * 10.0), 'T${p.nodeId}');
    const cursor = Offset(5, 5);

    test('input-side pick-up: fixed source end to the cursor', () {
      // Array input 2 of node 7 holds wires from nodes 1 and 3.
      final from = pin(7, PinKind.externalInput, 2, dataType: '[Geometry]');
      final move = WireMove(from: from, cursor: cursor, ends: [
        wireEnd(
            storageScope: [],
            source: 1,
            dest: 7,
            slot: 2,
            fixedEnd: pin(1, PinKind.externalOutput, 0)),
        wireEnd(
            storageScope: [],
            source: 3,
            dest: 7,
            slot: 2,
            fixedEnd: pin(3, PinKind.externalOutput, 1)),
      ]);
      expect(wireMoveSegments(move, positions), [
        const WireMoveSegment(Offset(100, 0), cursor, 'T1'),
        const WireMoveSegment(Offset(300, 10), cursor, 'T3'),
      ]);
    });

    test('output-side pick-up: cursor to each consumer, in the source colour',
        () {
      final from = pin(1, PinKind.externalOutput, 0, dataType: 'Geometry');
      final move = WireMove(from: from, cursor: cursor, ends: [
        wireEnd(
            storageScope: [],
            source: 1,
            dest: 7,
            slot: 2,
            fixedEnd: pin(7, PinKind.externalInput, 2)),
        wireEnd(
            storageScope: [9],
            source: 1,
            dest: 8,
            slot: 0,
            fixedEnd: pin(8, PinKind.externalInput, 0, scope: [9])),
      ]);
      expect(wireMoveSegments(move, positions), [
        const WireMoveSegment(cursor, Offset(700, 20), 'Geometry'),
        const WireMoveSegment(cursor, Offset(800, 0), 'Geometry'),
      ]);
    });

    test('hovering an accepting output (D16): that output to the start pin',
        () {
      final from = pin(7, PinKind.externalInput, 2);
      final move = WireMove(from: from, cursor: cursor, ends: [
        wireEnd(
            storageScope: [],
            source: 1,
            dest: 7,
            slot: 2,
            fixedEnd: pin(1, PinKind.externalOutput, 0)),
      ])
        ..hoveredTarget = pin(4, PinKind.externalOutput, 1);
      expect(wireMoveSegments(move, positions), [
        const WireMoveSegment(Offset(400, 10), Offset(700, 20), 'T4'),
      ]);
    });

    test('a cancelled move draws nothing', () {
      final move = WireMove(
          from: pin(7, PinKind.externalInput, 2),
          cursor: cursor,
          ends: [
            wireEnd(
                storageScope: [],
                source: 1,
                dest: 7,
                slot: 2,
                fixedEnd: pin(1, PinKind.externalOutput, 0)),
          ])
        ..cancelled = true;
      expect(wireMoveSegments(move, positions), isEmpty);
    });

    test('a fixed end off the canvas is skipped', () {
      final move = WireMove(
          from: pin(7, PinKind.externalInput, 2),
          cursor: cursor,
          ends: [
            wireEnd(
                storageScope: [],
                source: 1,
                dest: 7,
                slot: 2,
                fixedEnd: pin(1, PinKind.externalOutput, 0)),
          ]);
      expect(wireMoveSegments(move, (_) => null), isEmpty);
    });
  });
}
