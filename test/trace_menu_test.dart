/// The right-click contributing-nodes menu
/// (`doc/design_output_driven_editing.md`, Flutter → Menu).
///
/// The rows arrive pre-ordered from Rust; what the widget owns is the tree
/// (custom-node instances fold and unfold their children), which row a click
/// lands on, and dismissal. The gesture that opens it and the jump it triggers
/// are checked in the manual walkthrough.
library;

import 'package:flutter/material.dart';
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
    VoidCallback? onDismiss}) async {
  final selected = <APITraceRow>[];
  await tester.pumpWidget(MaterialApp(
    home: Scaffold(
      body: TraceMenu(
        position: const Offset(20, 20),
        rows: rows ?? _rows,
        truncated: truncated,
        onSelect: selected.add,
        onDismiss: onDismiss ?? () {},
      ),
    ),
  ));
  return selected;
}

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

  testWidgets('every row shows its node and what the node did', (tester) async {
    await _pump(tester);
    for (final row in _rows) {
      expect(find.text(row.nodeLabel), findsOneWidget);
    }
    expect(find.text('filled'), findsNWidgets(2));
    expect(find.text('shape'), findsOneWidget);
    expect(find.text('… more'), findsNothing);
  });

  testWidgets('a truncated trace starts with a "… more" line', (tester) async {
    await _pump(tester, truncated: true);
    expect(find.text('… more'), findsOneWidget);
  });

  testWidgets('the chevron folds and unfolds an instance', (tester) async {
    await _pump(tester);
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
    await tester.tap(find.text('slab1'));
    await tester.tap(find.text('fill'));
    expect(selected.map((r) => r.nodeLabel), ['slab1', 'fill']);
    expect(selected[1].hostNetwork, 'inner');
  });

  testWidgets('folding does not select', (tester) async {
    final selected = await _pump(tester);
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
