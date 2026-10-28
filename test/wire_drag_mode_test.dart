/// Tests for the move-wires gesture rule
/// (`lib/structure_designer/node_network/wire_drag_mode.dart`,
/// `doc/design_move_wires.md` "The rule"): every cell of the rule table.
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/structure_designer/node_network/wire_drag_mode.dart';

void main() {
  WireDragMode mode(
          {required bool isOutput,
          bool multi = false,
          required int wires,
          required bool ctrl}) =>
      wireDragMode(
          isOutput: isOutput, multi: multi, wireCount: wires, ctrl: ctrl);

  group('unconnected pin: always a new wire', () {
    for (final ctrl in [false, true]) {
      test('single input, ctrl=$ctrl', () {
        expect(
            mode(isOutput: false, wires: 0, ctrl: ctrl), WireDragMode.newWire);
      });
      test('array input, ctrl=$ctrl', () {
        expect(mode(isOutput: false, multi: true, wires: 0, ctrl: ctrl),
            WireDragMode.newWire);
      });
      test('output, ctrl=$ctrl', () {
        expect(
            mode(isOutput: true, wires: 0, ctrl: ctrl), WireDragMode.newWire);
      });
    }
  });

  group('connected single input: moves its wire either way', () {
    test('plain drag', () {
      expect(mode(isOutput: false, wires: 1, ctrl: false), WireDragMode.move);
    });
    test('ctrl+drag', () {
      expect(mode(isOutput: false, wires: 1, ctrl: true), WireDragMode.move);
    });
  });

  group('connected array input', () {
    test('plain drag adds a wire', () {
      expect(mode(isOutput: false, multi: true, wires: 3, ctrl: false),
          WireDragMode.newWire);
    });
    test('ctrl+drag moves all its wires', () {
      expect(mode(isOutput: false, multi: true, wires: 3, ctrl: true),
          WireDragMode.move);
    });
  });

  group('connected output (also function pin, zone input)', () {
    test('plain drag fans out a new wire', () {
      expect(mode(isOutput: true, wires: 2, ctrl: false), WireDragMode.newWire);
    });
    test('ctrl+drag moves all its wires', () {
      expect(mode(isOutput: true, wires: 2, ctrl: true), WireDragMode.move);
    });
  });
}
