// Library linking, Phase 4 (`doc/design_library_linking.md` §11): the Dart
// twins of the Rust name rules. Pure functions — no FFI.

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/library_link_actions.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:flutter_test/flutter_test.dart';

APIRefreshReport _report({
  List<String> refreshedMounts = const [],
  List<APIReportedWire> droppedWires = const [],
  List<APIReportedNode> frozenNodes = const [],
  List<APIReportedNode> reconciledNodes = const [],
  List<APIMountStatusChange> statusChanges = const [],
  List<String> held = const [],
  List<String> changedSinceSaved = const [],
  List<String> errors = const [],
  required bool isClean,
}) =>
    APIRefreshReport(
      refreshedMounts: refreshedMounts,
      refreshedDataFiles: const [],
      reconciledNodes: reconciledNodes,
      droppedWires: droppedWires,
      flaggedWires: const [],
      outputPinWarnings: const [],
      removedNamesInUse: const [],
      frozenNodes: frozenNodes,
      statusChanges: statusChanges,
      held: held,
      changedSinceSaved: changedSinceSaved,
      errors: errors,
      isClean: isClean,
    );

APIReportedWire _wire() => APIReportedWire(
      network: 'Main',
      scopePath: Uint64List(0),
      nodeId: BigInt.one,
      nodeLabel: 'f',
      pinName: 'x',
      sourceLabel: 'i1',
      reason: 'parameter removed',
    );

APIReportedNode _node() => APIReportedNode(
      network: 'Main',
      scopePath: Uint64List(0),
      nodeId: BigInt.two,
      nodeLabel: 'g',
      name: 'demolib.gone',
    );

void main() {
  group('refresh report wording (D10)', () {
    test('a clean refresh names each library with its file', () {
      final r = _report(
          refreshedMounts: ['demolib (libs/demolib_v3.cnnd)'], isClean: true);
      expect(
          refreshReportHeadline(r), 'Refreshed demolib (libs/demolib_v3.cnnd)');
      expect(refreshReportIsSilent(r), isFalse);
    });
    test('a refresh that disconnected wires says how many', () {
      final r = _report(
        refreshedMounts: ['demolib (libs/demolib_v3.cnnd)'],
        droppedWires: [_wire(), _wire(), _wire()],
        isClean: false,
      );
      expect(
          refreshReportHeadline(r), 'Refreshed demolib — 3 wires disconnected');
    });
    test('frozen nodes and a missing library are problems', () {
      final r = _report(
        frozenNodes: [_node()],
        statusChanges: [
          APIMountStatusChange(
              mountPath: 'demolib',
              status: APIMountStatus.missing,
              message: 'file not found'),
        ],
        isClean: false,
      );
      expect(refreshReportProblems(r),
          '1 node refers to a missing definition, demolib: file not found');
    });
    test('an open words it as an open', () {
      expect(
          refreshReportHeadline(
              _report(changedSinceSaved: ['demolib'], isClean: true),
              opened: true),
          'demolib changed since this file was saved');
      expect(
          refreshReportHeadline(
              _report(reconciledNodes: [_node(), _node()], isClean: true),
              opened: true),
          'Rewired 2 nodes to changed libraries');
    });
    test('a status-only return to Loaded is silent', () {
      final r = _report(statusChanges: [
        APIMountStatusChange(
            mountPath: 'demolib',
            status: APIMountStatus.loaded,
            message: 'loaded'),
      ], isClean: true);
      expect(refreshReportIsSilent(r), isTrue);
    });
    test('a refused operation is a failure, not a report', () {
      expect(
          refreshReportIsFailure(
              _report(errors: ['no linked library'], isClean: false)),
          isTrue);
      expect(
          refreshReportIsFailure(_report(
              refreshedMounts: ['a (a.cnnd)'], errors: ['x'], isClean: false)),
          isFalse);
    });
  });

  group('mountFor', () {
    // Mirrors `MOUNT_FOR_CASES` in
    // rust/crates/atomcad-structure-designer/tests/structure_designer/
    // library_links_interaction_test.rs case for case: change one, change
    // both.
    const cases = <(List<String>, String, String?)>[
      ([], 'demolib.foo', null),
      (['demolib'], 'demolib', 'demolib'),
      (['demolib'], 'demolib.foo', 'demolib'),
      (['demolib'], 'demolib.shapes.slab', 'demolib'),
      (['demolib'], 'demolibx', null),
      (['demolib'], 'demolib_extra.foo', null),
      (['demolib'], 'Main', null),
      (['demolib'], 'demo', null),
      (['demolib', 'demolib.common'], 'demolib.common.slab', 'demolib.common'),
      (['demolib', 'demolib.common'], 'demolib.commonx', 'demolib'),
      (['demolib', 'demolib.common'], 'demolib.common', 'demolib.common'),
      (['libs.demolib', 'libs.other'], 'libs.demolib.foo', 'libs.demolib'),
      (['libs.demolib', 'libs.other'], 'libs.other', 'libs.other'),
      (['libs.demolib', 'libs.other'], 'libs', null),
      (['libs.demolib', 'libs.other'], 'libs.local', null),
    ];
    for (final (mounts, name, expected) in cases) {
      test('$mounts / $name → $expected', () {
        expect(mountFor(name, mounts), expected);
        // Order of the mount list does not matter.
        expect(mountFor(name, mounts.reversed), expected);
      });
    }
  });

  group('validateLibraryAlias', () {
    test('accepts identifiers and dotted identifiers', () {
      for (final alias in ['demolib', '_x', 'lib2', 'libs.demolib', 'a.b.c']) {
        expect(validateLibraryAlias(alias), isNull, reason: alias);
      }
    });
    test('refuses empty and non-identifier segments', () {
      for (final alias in [
        '',
        'libs..x',
        '1lib',
        '.x',
        'x.',
        'demo lib',
        'demo-lib',
        'a.1b',
        'ä',
      ]) {
        expect(validateLibraryAlias(alias), isNotNull, reason: alias);
      }
    });
  });

  group('suggestLibraryAlias', () {
    test('drops the extension, the folder and a version suffix', () {
      expect(suggestLibraryAlias('demolib_v3.cnnd'), 'demolib');
      expect(suggestLibraryAlias(r'C:\libs\demolib_v3.cnnd'), 'demolib');
      expect(suggestLibraryAlias('../libs/common.cnnd'), 'common');
      expect(suggestLibraryAlias('shapes-v12.cnnd'), 'shapes');
      expect(suggestLibraryAlias('tools.cnnd'), 'tools');
    });
    test('always yields a valid alias', () {
      for (final path in [
        'my lib.cnnd',
        '3d-shapes.cnnd',
        'v2.cnnd',
        '.cnnd',
        'a.b.cnnd',
      ]) {
        final alias = suggestLibraryAlias(path);
        expect(validateLibraryAlias(alias), isNull, reason: '$path → $alias');
      }
      expect(suggestLibraryAlias('my lib.cnnd'), 'my_lib');
      expect(suggestLibraryAlias('3d-shapes.cnnd'), '_3d_shapes');
    });
  });

  test('mountFolderLabel puts the file after the alias segment', () {
    expect(mountFolderLabel('demolib', 'demolib_v3.cnnd'),
        'demolib — demolib_v3.cnnd');
  });
}
