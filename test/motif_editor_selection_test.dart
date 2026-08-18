/// Regression test for issue #422 — "fix copy paste for the Motif Definition
/// window".
///
/// Dragging a selection over the motif definition and scrolling mid-drag used
/// to leave the selection clipped to whatever happened to be on screen: the
/// field was wrapped in an extra vertical `SingleChildScrollView`, and Flutter's
/// drag-selection anchor compensation (`TextSelectionGestureDetectorBuilder.
/// onDragSelectionUpdate`) only corrects for the editable's *own* viewport
/// offset and for the *nearest ancestor* `Scrollable`. That extra scroll view
/// was neither, so every scrolled pixel dragged the anchor along with the
/// viewport.
///
/// This is a pure widget test — `MotifEditor` only touches the Rust model when
/// Apply is pressed, which this test never does.
library;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:code_text_field/code_text_field.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/motif_editor.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Long enough that only a fraction of it fits the 200 px tall field.
final String _definition = List<String>.generate(
  60,
  (i) => 'atom_${i.toString().padLeft(2, '0')} = C 0.0 0.0 $i.0',
).join('\n');

Future<void> _pumpEditor(WidgetTester tester) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SizedBox(
          width: 500,
          height: 600,
          child: MotifEditor(
            nodeId: BigInt.one,
            data: APIMotifData(definition: _definition, name: 'test'),
            model: StructureDesignerModel(),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

/// The definition field's controller (the only [CodeController] on screen).
CodeController _definitionController(WidgetTester tester) {
  final CodeField field = tester.widget<CodeField>(find.byType(CodeField));
  return field.controller;
}

/// The definition [EditableText] — matched by controller, since the editor also
/// hosts the name field and CodeField's line-number gutter.
Finder _definitionEditable(WidgetTester tester) {
  final CodeController controller = _definitionController(tester);
  return find.byWidgetPredicate(
    (Widget w) => w is EditableText && identical(w.controller, controller),
  );
}

void main() {
  testWidgets(
      'definition field scrolls itself rather than an outer scroll view',
      (tester) async {
    await _pumpEditor(tester);

    // The field is exactly as tall as its box: it does not grow to fit all 60
    // lines inside an enclosing scroll view.
    final Size fieldSize = tester.getSize(find.byType(CodeField));
    expect(fieldSize.height, lessThanOrEqualTo(200.0));

    // And it really is the field itself that scrolls: a wheel event over it
    // moves the *editable's* own viewport offset, which is the offset Flutter
    // compensates the drag anchor for.
    final EditableTextState state = tester.state(_definitionEditable(tester));
    expect(state.renderEditable.offset.pixels, 0.0);

    final TestPointer wheel = TestPointer(2, PointerDeviceKind.mouse);
    wheel.hover(tester.getCenter(find.byType(CodeField)));
    await tester.sendEventToBinding(wheel.scroll(const Offset(0.0, 120.0)));
    await tester.pumpAndSettle();

    expect(state.renderEditable.offset.pixels, greaterThan(0.0));
  });

  testWidgets('drag-selection survives scrolling mid-drag (issue #422)',
      (tester) async {
    await _pumpEditor(tester);

    final CodeController controller = _definitionController(tester);
    // Gesture geometry comes from the *visible box* (the 200 px tall slot the
    // field sits in), not from the editable — before the fix the editable was
    // 1276 px tall and mostly off screen, so points derived from it would land
    // outside the window.
    final Rect fieldRect = tester.getRect(find.byType(CodeTheme));
    expect(fieldRect.height, 200.0);
    // Just inside the left edge of the text itself, so the anchor lands on the
    // very first character.
    final double x = tester.getRect(_definitionEditable(tester)).left + 1.0;

    // Press near the very top of the first visible line...
    final TestGesture drag = await tester.startGesture(
      Offset(x, fieldRect.top + 4),
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    await drag.moveTo(Offset(x, fieldRect.top + 40));
    await tester.pump();

    // ...scroll the field a long way down while still holding the button...
    final TestPointer wheel = TestPointer(2, PointerDeviceKind.mouse);
    wheel.hover(fieldRect.center);
    for (int i = 0; i < 10; i++) {
      await tester.sendEventToBinding(wheel.scroll(const Offset(0.0, 120.0)));
      await tester.pump();
    }

    // ...then drag to the bottom of the (now scrolled) field and release.
    await drag.moveTo(Offset(x, fieldRect.bottom - 4));
    await tester.pump();
    await drag.up();
    await tester.pumpAndSettle();

    final TextSelection selection = controller.selection;
    expect(selection.baseOffset, 0,
        reason: 'the anchor must stay on the first line, not slide with the '
            'viewport');
    // The extent must reach text that was off-screen when the drag started.
    final int line40 = _definition.indexOf('atom_40');
    expect(line40, greaterThan(0));
    expect(selection.extentOffset, greaterThan(line40),
        reason: 'the selection must cover the lines scrolled into view');
  });
}
