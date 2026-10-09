/// The `chemisorb` panel's search tree (`ChemisorbDebugTree`, design §6.5 as
/// revised in §18): states and next-foot steps alternate, children are
/// fetched only when an item is expanded, duplicates are hidden until asked
/// for and jump to their canonical row, a click selects, and a new tree
/// starts from the root again. No Rust library needed: the tree is a fake
/// behind the widget's callbacks.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/chemisorb_debug_tree.dart';

APIChemisorbDebugRef ref(int row, [int? foot]) =>
    APIChemisorbDebugRef(row: row, foot: foot);

APIChemisorbDebugRow item(
  APIChemisorbDebugRef id, {
  APIChemisorbDebugRowKind kind = APIChemisorbDebugRowKind.leg,
  int legs = 1,
  String? label,
  int children = 0,
  int candidates = 0,
  int mirrored = 0,
  int mirroredChildren = 0,
  int hidden = 0,
  APIChemisorbMirror? mirror,
  int? duplicateOf,
  double? strain,
}) =>
    APIChemisorbDebugRow(
      item: id,
      kind: kind,
      legs: legs,
      label: label ?? 'row${id.row}',
      path: '${id.row}',
      duplicateOf: duplicateOf,
      children: children,
      hiddenDuplicates: hidden,
      mirroredChildren: mirroredChildren,
      mirroredDuplicates: 0,
      candidates: candidates,
      mirrored: mirrored,
      undecided: 0,
      clashes: 0,
      rejectedValence: 0,
      rejectedNoAcceptor: 0,
      rejectedFilter: 0,
      nearMisses: const [],
      candidate: false,
      localParent: false,
      mirror: mirror,
      seatingClash: false,
      prunedClash: false,
      strain: strain,
      converged: true,
      budgetCut: false,
      canSeat: kind == APIChemisorbDebugRowKind.leg,
      canRelax: strain != null,
      defaultForm: kind == APIChemisorbDebugRowKind.leg
          ? APIChemisorbDebugForm.seated
          : kind == APIChemisorbDebugRowKind.nextFoot && legs > 0
              ? APIChemisorbDebugForm.seated
              : APIChemisorbDebugForm.posed,
    );

/// root → next foot O7 → legs 2 and 3 (4 a duplicate of 3) → leg 3's next
/// foot O8 → leg 5, and leg 6, mirror-pruned.
final items = {
  ref(0): item(ref(0),
      kind: APIChemisorbDebugRowKind.root, legs: 0, label: 'root', children: 1),
  ref(0, 0): item(ref(0, 0),
      kind: APIChemisorbDebugRowKind.nextFoot,
      legs: 0,
      label: 'next foot O7',
      children: 2,
      candidates: 3,
      hidden: 1),
  ref(2): item(ref(2), label: 'O7–Si1'),
  ref(3): item(ref(3), label: 'O7–Si2', strain: 12.5, children: 1),
  ref(4): item(ref(4), label: 'O7–Si3', duplicateOf: 3),
  ref(3, 1): item(ref(3, 1),
      kind: APIChemisorbDebugRowKind.nextFoot,
      label: 'next foot O8',
      children: 2,
      candidates: 1,
      mirrored: 1,
      mirroredChildren: 1),
  ref(5): item(ref(5), legs: 2, label: 'O8–Si4'),
  ref(6): item(ref(6),
      legs: 2, label: 'O8–Si9', mirror: APIChemisorbMirror.mirrored),
};

final shownChildren = {
  ref(0): [ref(0, 0)],
  ref(0, 0): [ref(2), ref(3)],
  ref(3): [ref(3, 1)],
  ref(3, 1): [ref(5)],
};
final allChildren = {
  ...shownChildren,
  ref(0, 0): [ref(2), ref(3), ref(4)],
};
final withMirrored = {
  ref(3, 1): [ref(5), ref(6)]
};

class Harness {
  final fetched = <(APIChemisorbDebugRef, bool)>[];
  final mirroredAsked = <bool>[];
  final selected = <(APIChemisorbDebugRef, APIChemisorbDebugForm?)>[];
  final ancestors = <APIChemisorbDebugRef>[];

  Widget build({BigInt? treeKey, APIChemisorbDebugRef? selectedItem}) =>
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SizedBox(
              width: 500,
              child: ChemisorbDebugTree(
                treeKey: treeKey ?? BigInt.one,
                searched: false,
                selected: selectedItem,
                selectedForm: null,
                debugShown: false,
                shapesShown: false,
                fetchRow: (r) => items[r],
                fetchChildren: (r, dups, mirrored) {
                  fetched.add((r, dups));
                  mirroredAsked.add(mirrored);
                  final ids = (mirrored ? withMirrored[r] : null) ??
                      (dups ? allChildren : shownChildren)[r] ??
                      <APIChemisorbDebugRef>[];
                  return [for (final c in ids) items[c]!];
                },
                fetchAncestors: (r) {
                  ancestors.add(r);
                  return [ref(0), ref(0, 0), r];
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

Future<void> expandFirst(WidgetTester tester) async {
  await tester.tap(find.byIcon(Icons.chevron_right).first);
  await tester.pump();
}

void main() {
  testWidgets('children are fetched only when an item is expanded',
      (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build());
    // The root is open: its next feet are listed, their legs are not.
    expect(find.text('next foot O7'), findsOneWidget);
    expect(find.textContaining('anchor: 3'), findsOneWidget);
    expect(find.text('O7–Si1'), findsNothing);
    expect(h.fetched, [(ref(0), false)]);

    await expandFirst(tester);
    expect(find.text('O7–Si1'), findsOneWidget);
    expect(find.text('O7–Si2'), findsOneWidget);
    expect(find.text('O7–Si3'), findsNothing,
        reason: 'the duplicate is hidden');
    expect(find.textContaining('1 dup. hidden'), findsOneWidget);
    expect(find.textContaining('strain 12.5'), findsOneWidget);
    expect(h.fetched, [(ref(0), false), (ref(0, 0), false)]);
  });

  testWidgets('states and steps alternate down the tree', (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build());
    await expandFirst(tester); // next foot O7
    await expandFirst(tester); // O7–Si2, the one leg with children
    expect(find.text('next foot O8'), findsOneWidget);
    expect(find.textContaining('shell: 2, 1 mirr.'), findsOneWidget,
        reason: 'a step counts every site its test accepted');
    expect(find.textContaining('near'), findsNothing,
        reason: 'near misses are not listed');
    await expandFirst(tester); // next foot O8
    expect(find.text('+ O8–Si4'), findsOneWidget,
        reason: 'a leg past the first is an addition');
    await tester.tap(find.text('next foot O8'));
    expect(h.selected.last, (ref(3, 1), null));
  });

  testWidgets('mirror-pruned legs are hidden until asked for', (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build());
    await expandFirst(tester);
    await expandFirst(tester);
    await expandFirst(tester); // next foot O8
    expect(h.mirroredAsked.every((m) => !m), isTrue, reason: 'off by default');
    expect(find.text('+ O8–Si4'), findsOneWidget);
    expect(find.text('+ O8–Si9'), findsNothing);

    await tester.tap(find.text('Show mirrored'));
    await tester.pump();
    expect(h.mirroredAsked.last, isTrue, reason: 'refetched with mirrored');
    expect(find.text('+ O8–Si9'), findsOneWidget);
  });

  testWidgets('a click selects; a duplicate, shown on request, jumps',
      (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build());
    await expandFirst(tester);
    await tester.tap(find.text('O7–Si1'));
    expect(h.selected, [(ref(2), null)]);

    await tester.tap(find.text('Show duplicates'));
    await tester.pump();
    expect(find.text('O7–Si3'), findsOneWidget);
    expect(find.textContaining('= duplicate of #3'), findsOneWidget);
    expect(h.fetched.last, (ref(0, 0), true),
        reason: 'refetched with duplicates');
    await tester.tap(find.text('O7–Si3'));
    expect(h.ancestors, [ref(3)]);
    expect(h.selected.last, (ref(3), null),
        reason: 'the canonical row is selected');
  });

  testWidgets('a new tree starts from the root again', (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build());
    await expandFirst(tester);
    expect(find.text('O7–Si1'), findsOneWidget);

    await tester.pumpWidget(h.build(treeKey: BigInt.two));
    expect(find.text('O7–Si1'), findsNothing);
    expect(find.text('next foot O7'), findsOneWidget);
  });

  testWidgets('the selected item offers its forms and a way back to the root',
      (tester) async {
    final h = Harness();
    await tester.pumpWidget(h.build(selectedItem: ref(3)));
    expect(find.textContaining('Showing #3 O7–Si2'), findsOneWidget);
    await tester.tap(find.text('Relaxed'));
    expect(h.selected.last, (ref(3), APIChemisorbDebugForm.relaxed));
    await tester.tap(find.byIcon(Icons.home_outlined));
    expect(h.selected.last, (ref(0), null));

    await tester.pumpWidget(h.build(selectedItem: ref(3, 1)));
    expect(
        find.textContaining('Showing next foot O8 under #3'), findsOneWidget);
    expect(find.text('Relaxed'), findsNothing,
        reason: 'a step has the one form its test read');
  });
}
