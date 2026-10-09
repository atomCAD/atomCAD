/// The `chemisorb` panel's search tree (`ChemisorbDebugTree`, design §6.5):
/// children are fetched only when a row is expanded, duplicates are hidden
/// until asked for and jump to their canonical row, a click selects, and a
/// new tree starts from the root again. No Rust library needed: the tree is
/// a fake behind the widget's callbacks.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/chemisorb_debug_tree.dart';

APIChemisorbDebugRow row(
  int id, {
  int? parent,
  APIChemisorbDebugRowKind kind = APIChemisorbDebugRowKind.leg,
  int legs = 1,
  String? label,
  int children = 0,
  int hidden = 0,
  int? duplicateOf,
  double? strain,
}) =>
    APIChemisorbDebugRow(
      row: id,
      parent: parent,
      kind: kind,
      legs: legs,
      label: label ?? 'row$id',
      path: '$id',
      duplicateOf: duplicateOf,
      children: children,
      hiddenDuplicates: hidden,
      candidates: children,
      mirrored: 0,
      undecided: 0,
      clashes: 0,
      rejectedValence: 0,
      rejectedNoAcceptor: 0,
      rejectedFilter: 0,
      nearMisses: const [],
      candidate: false,
      localParent: false,
      seatingClash: false,
      prunedClash: false,
      strain: strain,
      converged: true,
      budgetCut: false,
      canSeat: kind == APIChemisorbDebugRowKind.leg,
      canRelax: strain != null,
      defaultForm: kind == APIChemisorbDebugRowKind.leg
          ? APIChemisorbDebugForm.seated
          : APIChemisorbDebugForm.posed,
    );

/// root(0) → foot(1) → legs 2 and 3; row 4 under foot 1 is a duplicate of 3.
final rows = {
  0: row(0,
      kind: APIChemisorbDebugRowKind.root, legs: 0, label: 'root', children: 1),
  1: row(1,
      parent: 0,
      kind: APIChemisorbDebugRowKind.foot,
      legs: 0,
      label: 'foot O7',
      children: 2,
      hidden: 1),
  2: row(2, parent: 1, label: 'O7–Si1'),
  3: row(3, parent: 1, label: 'O7–Si2', strain: 12.5),
  4: row(4, parent: 1, label: 'O7–Si3', duplicateOf: 3),
};

final shownChildren = {
  0: [1],
  1: [2, 3],
};
final allChildren = {
  0: [1],
  1: [2, 3, 4],
};

class Harness {
  final fetched = <(int, bool)>[];
  final selected = <(int, APIChemisorbDebugForm?)>[];
  final ancestors = <int>[];

  Widget build({BigInt? treeKey, int? selectedRow}) => MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SizedBox(
              width: 500,
              child: ChemisorbDebugTree(
                treeKey: treeKey ?? BigInt.one,
                searched: false,
                selectedRow: selectedRow,
                selectedForm: null,
                debugShown: false,
                shapesShown: false,
                fetchRow: (r) => rows[r],
                fetchChildren: (r, dups) {
                  fetched.add((r, dups));
                  return [
                    for (final c
                        in (dups ? allChildren : shownChildren)[r] ?? <int>[])
                      rows[c]!
                  ];
                },
                fetchAncestors: (r) {
                  ancestors.add(r);
                  return [0, 1, r];
                },
                onSelect: (r, f) => selected.add((r, f)),
                onToggleDebug: () {},
                onToggleShapes: () {},
              ),
            ),
          ),
        ),
      );
}

void main() {
  testWidgets('children are fetched only when a row is expanded',
      (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build());
    // The root is open: its children (the feet) are listed, the feet's not.
    expect(find.text('foot O7'), findsOneWidget);
    expect(find.text('+ O7–Si1'), findsNothing);
    expect(find.text('O7–Si1'), findsNothing);
    expect(h.fetched, [(0, false)]);

    await tester.tap(find.byIcon(Icons.chevron_right));
    await tester.pump();
    expect(find.text('O7–Si1'), findsOneWidget);
    expect(find.text('O7–Si2'), findsOneWidget);
    expect(find.text('O7–Si3'), findsNothing,
        reason: 'the duplicate is hidden');
    expect(find.textContaining('1 dup. hidden'), findsOneWidget);
    expect(find.textContaining('strain 12.5'), findsOneWidget);
    expect(h.fetched, [(0, false), (1, false)]);
  });

  testWidgets('a click selects; a duplicate, shown on request, jumps',
      (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build());
    await tester.tap(find.byIcon(Icons.chevron_right));
    await tester.pump();
    await tester.tap(find.text('O7–Si1'));
    expect(h.selected, [(2, null)]);

    await tester.tap(find.text('Show duplicates'));
    await tester.pump();
    expect(find.text('O7–Si3'), findsOneWidget);
    expect(find.textContaining('= duplicate of #3'), findsOneWidget);
    expect(h.fetched.last, (1, true), reason: 'refetched with duplicates');
    await tester.tap(find.text('O7–Si3'));
    expect(h.ancestors, [3]);
    expect(h.selected.last, (3, null), reason: 'the canonical row is selected');
  });

  testWidgets('a new tree starts from the root again', (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build());
    await tester.tap(find.byIcon(Icons.chevron_right));
    await tester.pump();
    expect(find.text('O7–Si1'), findsOneWidget);

    await tester.pumpWidget(h.build(treeKey: BigInt.two));
    expect(find.text('O7–Si1'), findsNothing);
    expect(find.text('foot O7'), findsOneWidget);
  });

  testWidgets('the selected row offers its forms and a way back to the root',
      (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build(selectedRow: 3));
    expect(find.textContaining('Showing #3 O7–Si2'), findsOneWidget);
    await tester.tap(find.text('Relaxed'));
    expect(h.selected.last, (3, APIChemisorbDebugForm.relaxed));
    await tester.tap(find.byIcon(Icons.home_outlined));
    expect(h.selected.last, (0, null));
  });
}
