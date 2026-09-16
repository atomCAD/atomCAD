/// The derivation behind the editor panel's operation-group chips
/// (`doc/design_mechanosynth_op_muting.md`).
///
/// The chips are the fast way to mute a whole method — "I am not doing
/// precursor deposits this week" — and the grouping they offer is **derived
/// from fields the library already states** rather than from a `family` key the
/// schema deliberately does not have. That derivation is the part worth
/// pinning: the widgets around it are thin (a checkbox, a `Wrap` of chips) and
/// belong to the manual walkthrough.
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_status.dart';

APIMechanosynthOp _op(
  String name, {
  String method = 'tip',
  String toolType = '',
  String agent = '',
  bool muted = false,
}) =>
    APIMechanosynthOp(
      name: name,
      note: '',
      method: method,
      toolType: toolType,
      agent: agent,
      muted: muted,
    );

/// The silicon T-centre library's shape, abbreviated: three tip instruments,
/// two bulk agents, one spontaneous family.
final _library = [
  _op('cl_abstract', toolType: 'w_probe'),
  _op('precursor_chemisorb', method: 'bulk', agent: 'C2HCl3'),
  _op('h_donate_c', toolType: 'c_tool'),
  _op('h_abstract', toolType: 'c_tool'),
  _op('si_donate_dimer', toolType: 'si_tool'),
  _op('si_pickup', toolType: 'si_tool'),
  _op('dimerize', method: 'spontaneous'),
  _op('bridge', method: 'spontaneous'),
  _op('cl_donate_core', method: 'bulk', agent: 'Cl2'),
];

void main() {
  group('instrumentOf', () {
    test('a tip operation is named by its tool, a bulk one by its agent', () {
      expect(instrumentOf(_op('x', toolType: 'si_tool')), 'si_tool');
      expect(instrumentOf(_op('x', method: 'bulk', agent: 'Cl2')), 'Cl2');
    });

    test('a spontaneous operation falls back to its method', () {
      // It has neither a tool nor an agent — nothing performs it — but the chip
      // still needs a label, and "spontaneous" is the honest one.
      expect(instrumentOf(_op('x', method: 'spontaneous')), 'spontaneous');
    });

    test('an operation the library does not define has no instrument', () {
      expect(instrumentOf(_op('stranger', method: '')), '');
    });
  });

  group('groupOperationsByInstrument', () {
    test('groups by instrument in the library\'s own order', () {
      final groups = groupOperationsByInstrument(_library);
      expect(groups.map((g) => g.key).toList(),
          ['w_probe', 'C2HCl3', 'c_tool', 'si_tool', 'spontaneous', 'Cl2']);
      expect(groups.map((g) => g.value.length).toList(), [1, 1, 2, 2, 2, 1]);
    });

    test('mechadense\'s ask is one group', () {
      // The request that started this feature — "turning off ... the precursor
      // deposit pattern(s)" — has to land on a single chip, or the affordance
      // is not the one that was asked for.
      final groups = groupOperationsByInstrument(_library);
      final precursor = groups.firstWhere((g) => g.key == 'C2HCl3');
      expect(precursor.value.single.name, 'precursor_chemisorb');
    });

    test('a name the wired library does not define joins no group', () {
      // It is kept in the mute set on purpose, and the palette lists it greyed
      // — but nothing performs it, so it has nothing to say about instruments.
      final groups = groupOperationsByInstrument([
        ..._library,
        _op('from_another_library', method: '', muted: true),
      ]);
      expect(groups.length, 6);
      expect(
        groups.expand((g) => g.value).map((op) => op.name),
        isNot(contains('from_another_library')),
      );
    });

    test('muting does not move an operation out of its group', () {
      // The chip is tri-state over a group whose membership never changes —
      // that is what lets it report "si_tool 1/2" rather than losing the row.
      final muted = [
        _op('si_donate_dimer', toolType: 'si_tool', muted: true),
        _op('si_pickup', toolType: 'si_tool'),
      ];
      final groups = groupOperationsByInstrument(muted);
      expect(groups.single.key, 'si_tool');
      expect(groups.single.value.length, 2);
      expect(groups.single.value.where((op) => op.muted).length, 1);
    });
  });
}
