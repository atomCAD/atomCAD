/// The right-click contributing-nodes menu
/// (`doc/design_output_driven_editing.md`, Flutter → Menu).
///
/// The rows arrive pre-ordered from Rust; what the widget owns is the tree
/// (custom-node instances fold and unfold their children), which row a click
/// lands on, and dismissal. The gesture that opens it and the jump it triggers
/// are checked in the manual walkthrough.
library;

import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/trace_menu.dart';

APITraceRow _row(String label, int depth,
        {String network = 'main',
        APIContributionKind kind = APIContributionKind.passed,
        String text = 'passed'}) =>
    APITraceRow(
      hostNetwork: network,
      scopePath: Uint64List.fromList(const []),
      nodeId: BigInt.from(label.hashCode.abs() % 1000),
      nodeLabel: label,
      depth: depth,
      contribution: kind,
      label: text,
    );

/// `slab1` is an instance of `slab`, which holds an instance `inner1` of
/// `inner`.
final _rows = [
  _row('slab1', 0, kind: APIContributionKind.created, text: 'filled'),
  _row('inner1', 1, network: 'slab'),
  _row('fill', 2,
      network: 'inner', kind: APIContributionKind.created, text: 'filled'),
  _row('p', 1, network: 'slab'),
  _row('sphere', 0, kind: APIContributionKind.created, text: 'shape'),
];

Future<List<APITraceRow>> _pump(WidgetTester tester,
    {List<APITraceRow>? rows,
    bool truncated = false,
    VoidCallback? onDismiss,
    void Function(APITraceRow? row)? onHover}) async {
  final selected = <APITraceRow>[];
  await tester.pumpWidget(MaterialApp(
    home: Scaffold(
      body: TraceMenu(
        position: const Offset(20, 20),
        rows: rows ?? _rows,
        truncated: truncated,
        onSelect: selected.add,
        onDismiss: onDismiss ?? () {},
        onHover: onHover,
      ),
    ),
  ));
  return selected;
}

/// Unfold `slab1` and then `inner1` (instances start collapsed).
Future<void> _expandInstances(WidgetTester tester) async {
  await tester.tap(find.byKey(const ValueKey('trace_chevron_0')));
  await tester.pump();
  await tester.tap(find.byKey(const ValueKey('trace_chevron_1')));
  await tester.pump();
}

APITraceRow _wired(String label, int depth, {String network = 'main'}) =>
    _row(label, depth,
        network: network,
        kind: APIContributionKind.unreached,
        text: 'wired upstream');

/// `move` lost the probe; `a`, `b` (with `inner` traced inside it) and `c`
/// are wired upstream of it.
final _lostRows = [
  _row('move', 0,
      kind: APIContributionKind.unknown, text: 'possibly contributed'),
  _wired('a', 0),
  _wired('b', 0),
  _wired('inner', 1, network: 'b_net'),
  _wired('c', 0),
];

/// `slab1`'s lost cone leaves it through a parameter: one run closes its
/// children (`w1`, `w2`), another follows it at the top level (`w3`).
final _splitRows = [
  _row('slab1', 0, kind: APIContributionKind.created, text: 'filled'),
  _row('fill', 1,
      network: 'slab',
      kind: APIContributionKind.unknown,
      text: 'possibly contributed'),
  _wired('w1', 1, network: 'slab'),
  _wired('w2', 1, network: 'slab'),
  _wired('w3', 0),
];

void main() {
  group('visibleTraceRows', () {
    test('everything shows when nothing is collapsed', () {
      expect(visibleTraceRows(_rows, {}), [0, 1, 2, 3, 4]);
    });

    test('a collapsed instance hides its whole subtree and nothing else', () {
      expect(visibleTraceRows(_rows, {0}), [0, 4]);
      expect(visibleTraceRows(_rows, {1}), [0, 1, 3, 4]);
    });

    test('collapsing a row without children hides nothing', () {
      expect(visibleTraceRows(_rows, {2, 4}), [0, 1, 2, 3, 4]);
    });

    test('only instance rows have children', () {
      expect([
        for (var i = 0; i < _rows.length; i++) traceRowHasChildren(_rows, i)
      ], [
        true,
        true,
        false,
        false,
        false
      ]);
    });
  });

  group('wired-upstream runs', () {
    test('a run of unreached siblings folds into one header', () {
      final lines = visibleTraceLines(_lostRows, {}, {});
      expect(
          lines.map((l) => (l.index, l.isRunHeader)), [(0, false), (1, true)]);
      // a, b, b's nested row, c.
      expect(lines[1].runSize, 4);
    });

    test('unfolded, the header stays and the rows follow', () {
      final lines = visibleTraceLines(_lostRows, {}, {1});
      expect(lines.map((l) => (l.index, l.isRunHeader)), [
        (0, false),
        (1, true),
        (1, false),
        (2, false),
        // `b`'s own unreached child is a run one level down, folded on its
        // own.
        (3, true),
        (4, false),
      ]);
      expect(visibleTraceLines(_lostRows, {}, {1, 3})[5].index, 3);
    });

    test('a run never crosses a tree level', () {
      expect(traceRowStartsRun(_splitRows, 2), isTrue);
      expect(traceRunEnd(_splitRows, 2), 4);
      expect(traceRowStartsRun(_splitRows, 4), isTrue);
      final lines = visibleTraceLines(_splitRows, {}, {});
      expect(lines.map((l) => (l.index, l.isRunHeader)), [
        (0, false),
        (1, false),
        (2, true),
        (4, true),
      ]);
      expect(lines[2].runSize, 2);
      expect(lines[3].runSize, 1);
    });

    test('a collapsed instance hides its inner run header', () {
      final lines = visibleTraceLines(_splitRows, {0}, {});
      expect(
          lines.map((l) => (l.index, l.isRunHeader)), [(0, false), (4, true)]);
    });
  });

  testWidgets('an instance starts collapsed and its chevron unfolds it',
      (tester) async {
    await _pump(tester);
    expect(find.text('inner1'), findsNothing);
    expect(find.byIcon(Icons.chevron_right), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('trace_chevron_0')));
    await tester.pump();
    expect(find.text('inner1'), findsOneWidget);
    // The nested instance starts collapsed too.
    expect(find.text('fill'), findsNothing);
  });

  testWidgets('a wired-upstream run shows one line, and unfolds in place',
      (tester) async {
    await _pump(tester, rows: _lostRows);
    expect(find.text('4 wired upstream'), findsOneWidget);
    expect(find.text('a'), findsNothing);

    await tester.tap(find.text('4 wired upstream'));
    await tester.pump();
    expect(find.text('4 wired upstream'), findsOneWidget);
    expect(
        find.descendant(
            of: find.byKey(const ValueKey('trace_run_1')),
            matching: find.byIcon(Icons.expand_more)),
        findsOneWidget);
    // Siblings the header groups, not its children: the same indent as the
    // row above the header.
    expect(tester.getTopLeft(find.text('a')).dx,
        tester.getTopLeft(find.text('move')).dx);
    expect(tester.getTopLeft(find.text('c')).dx,
        tester.getTopLeft(find.text('move')).dx);

    await tester.tap(find.text('4 wired upstream'));
    await tester.pump();
    expect(find.text('a'), findsNothing);
  });

  testWidgets('hovering a run header reports no row', (tester) async {
    final hovered = <String?>[];
    await _pump(tester,
        rows: _lostRows, onHover: (row) => hovered.add(row?.nodeLabel));
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(790, 590));
    await mouse.moveTo(tester.getCenter(find.text('4 wired upstream')));
    await tester.pump();
    expect(hovered, isEmpty);
    await mouse.removePointer();
  });

  testWidgets('a cone leaving an instance gives two runs, each its own',
      (tester) async {
    await _pump(tester, rows: _splitRows);
    // Only the outer run while the instance is folded.
    expect(find.text('1 wired upstream'), findsOneWidget);
    expect(find.text('2 wired upstream'), findsNothing);

    await tester.tap(find.byKey(const ValueKey('trace_chevron_0')));
    await tester.pump();
    expect(find.text('2 wired upstream'), findsOneWidget);
    expect(find.text('w1'), findsNothing);

    await tester.tap(find.text('2 wired upstream'));
    await tester.pump();
    expect(find.text('w1'), findsOneWidget);
    expect(find.text('w3'), findsNothing);

    await tester.tap(find.text('1 wired upstream'));
    await tester.pump();
    expect(find.text('w3'), findsOneWidget);

    // Folding the instance hides the inner run, header and all.
    await tester.tap(find.byKey(const ValueKey('trace_chevron_0')));
    await tester.pump();
    expect(find.text('2 wired upstream'), findsNothing);
    expect(find.text('w1'), findsNothing);
    expect(find.text('w3'), findsOneWidget);
  });

  testWidgets('every row shows its node and what the node did', (tester) async {
    await _pump(tester);
    await _expandInstances(tester);
    for (final row in _rows) {
      expect(find.text(row.nodeLabel), findsOneWidget);
    }
    expect(find.text('filled'), findsNWidgets(2));
    expect(find.text('shape'), findsOneWidget);
    expect(find.text('… more'), findsNothing);
  });

  testWidgets('a truncated trace ends with a "… more" line', (tester) async {
    await _pump(tester, truncated: true);
    expect(find.text('… more'), findsOneWidget);
    expect(tester.getTopLeft(find.text('… more')).dy,
        greaterThan(tester.getTopLeft(find.text(_rows.last.nodeLabel)).dy));
  });

  testWidgets('the chevron folds and unfolds an instance', (tester) async {
    await _pump(tester);
    await _expandInstances(tester);
    await tester.tap(find.byKey(const ValueKey('trace_chevron_0')));
    await tester.pump();
    expect(find.text('inner1'), findsNothing);
    expect(find.text('fill'), findsNothing);
    expect(find.text('sphere'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('trace_chevron_0')));
    await tester.pump();
    expect(find.text('fill'), findsOneWidget);
  });

  testWidgets('clicking a row selects that row, parent or child',
      (tester) async {
    final selected = await _pump(tester);
    await _expandInstances(tester);
    await tester.tap(find.text('slab1'));
    await tester.tap(find.text('fill'));
    expect(selected.map((r) => r.nodeLabel), ['slab1', 'fill']);
    expect(selected[1].hostNetwork, 'inner');
  });

  testWidgets('hovering reports each row, and null on leaving', (tester) async {
    final hovered = <String?>[];
    await _pump(tester, onHover: (row) => hovered.add(row?.nodeLabel));
    await _expandInstances(tester);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(790, 590));
    await mouse.moveTo(tester.getCenter(find.text('slab1')));
    await tester.pump();
    await mouse.moveTo(tester.getCenter(find.text('fill')));
    await tester.pump();
    await mouse.moveTo(const Offset(790, 590));
    await tester.pump();
    expect(hovered, ['slab1', 'fill', null]);
    await mouse.removePointer();
  });

  testWidgets('folding does not select', (tester) async {
    final selected = await _pump(tester);
    await _expandInstances(tester);
    await tester.tap(find.byKey(const ValueKey('trace_chevron_1')));
    await tester.pump();
    expect(selected, isEmpty);
  });

  testWidgets('Escape and clicking away dismiss', (tester) async {
    var dismissed = 0;
    await _pump(tester, onDismiss: () => dismissed++);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    expect(dismissed, 1);
    await tester.tapAt(const Offset(780, 580));
    expect(dismissed, 2);
  });
}
