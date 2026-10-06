/// The `chemisorb` panel's bond-inventory menu rows: the label wraps at its
/// `;`, the relaxation count is never cut, and a tooltip appears only on a
/// label too long for two lines. No Rust library needed.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/structure_designer/node_data/chemisorb_editor.dart';

const String LONG = 'formed 3× H–Si, 2× O–Si; broken 3× H–O';

Future<void> pumpRow(WidgetTester tester, String label, double width) =>
    tester.pumpWidget(MaterialApp(
      home: Scaffold(
        body: Align(
          alignment: Alignment.topLeft,
          child: SizedBox(
            width: width,
            child: ChemisorbInventoryOptionRow(
                label: label, count: BigInt.from(60)),
          ),
        ),
      ),
    ));

void main() {
  test('the label breaks after each semicolon', () {
    expect(wrapInventoryLabel(LONG), 'formed 3× H–Si, 2× O–Si;\nbroken 3× H–O');
    expect(wrapInventoryLabel('formed 3× O–Si'), 'formed 3× O–Si');
  });

  testWidgets('at panel width: two lines, the count beside, no tooltip',
      (tester) async {
    // Wide in test-font terms: every glyph of the test font is a full square.
    await pumpRow(tester, LONG, 700);
    expect(
        find.text('formed 3× H–Si, 2× O–Si;\nbroken 3× H–O'), findsOneWidget);
    expect(find.text('60'), findsOneWidget);
    expect(find.text('to relax'), findsOneWidget);
    expect(find.byType(Tooltip), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('too narrow: the count still shows and the label gets a tooltip',
      (tester) async {
    await pumpRow(tester, LONG, 140);
    expect(find.text('60'), findsOneWidget);
    final tooltip = tester.widget<Tooltip>(find.byType(Tooltip));
    expect(tooltip.message, LONG);
    expect(tester.takeException(), isNull);
  });
}
