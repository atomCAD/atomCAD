/// Widget tests for `IntSpinField`, the single integer box behind `IntInput`
/// and the integer vector inputs (`lib/inputs/int_spin_field.dart`).
///
/// The `−` / `+` buttons exist because mouse-wheel stepping is silently dead
/// on some hardware, so the three stepping routes — buttons, arrow keys and
/// the wheel — are each exercised here against the same clamping rules. This
/// is a pure widget test: the field never touches the Rust model.
library;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/int_spin_field.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_test/flutter_test.dart';

/// Pumps an `IntInput` whose committed value follows what it emits, the way a
/// property panel re-renders from the model after every write.
Future<List<int>> _pumpIntInput(
  WidgetTester tester, {
  required int value,
  int? min,
  int? max,
}) async {
  final received = <int>[];
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: Center(
          child: SizedBox(
            width: 200,
            child: StatefulBuilder(
              builder: (context, setState) => IntInput(
                label: 'n',
                value: value,
                minimumValue: min,
                maximumValue: max,
                onChanged: (v) {
                  received.add(v);
                  setState(() => value = v);
                },
              ),
            ),
          ),
        ),
      ),
    ),
  );
  return received;
}

Finder get _plus => find.byIcon(Icons.add);
Finder get _minus => find.byIcon(Icons.remove);

void main() {
  testWidgets('the + and − buttons step by one', (tester) async {
    final received = await _pumpIntInput(tester, value: 5);

    await tester.tap(_plus);
    await tester.pump();
    expect(received, [6]);
    expect(find.text('6'), findsOneWidget);

    await tester.tap(_minus);
    await tester.tap(_minus);
    await tester.pump();
    expect(received, [6, 5, 4]);
    expect(find.text('4'), findsOneWidget);
  });

  testWidgets('stepping clamps to the range and is silent at the bound',
      (tester) async {
    final received = await _pumpIntInput(tester, value: 6, min: 0, max: 7);

    await tester.tap(_plus);
    await tester.pump();
    expect(received, [7]);

    // Already at the maximum: no write, not even a repeated `7`.
    await tester.tap(_plus);
    await tester.pump();
    expect(received, [7]);
  });

  testWidgets('a held button auto-repeats until release', (tester) async {
    final received = await _pumpIntInput(tester, value: 0);

    final gesture = await tester.startGesture(tester.getCenter(_plus));
    await tester.pump();
    expect(received, [1], reason: 'fires once on press');

    // Nothing more before the initial delay has elapsed.
    await tester.pump(INT_SPIN_REPEAT_DELAY - const Duration(milliseconds: 50));
    expect(received, [1]);

    await tester.pump(const Duration(milliseconds: 50));
    await tester.pump(INT_SPIN_REPEAT_INTERVAL);
    await tester.pump(INT_SPIN_REPEAT_INTERVAL);
    await tester.pump(INT_SPIN_REPEAT_INTERVAL);
    expect(received.length, greaterThanOrEqualTo(3),
        reason: 'repeats while held');

    await gesture.up();
    await tester.pump();
    final atRelease = received.length;
    await tester.pump(INT_SPIN_REPEAT_INTERVAL * 5);
    expect(received.length, atRelease, reason: 'stops on release');
  });

  testWidgets('SHIFT multiplies a button step by ten', (tester) async {
    final received = await _pumpIntInput(tester, value: 5, min: 0, max: 100);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.tap(_plus);
    await tester.pump();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    expect(received, [5 + INT_SPIN_SHIFT_MULTIPLIER]);

    // A shifted step that would overshoot lands on the bound.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.tap(_minus);
    await tester.tap(_minus);
    await tester.pump();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    expect(received, [15, 5, 0]);
  });

  testWidgets('arrow keys step the focused field', (tester) async {
    final received = await _pumpIntInput(tester, value: 5);

    await tester.tap(find.byType(TextField));
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(received, [6, 7, 6]);
  });

  testWidgets('the mouse wheel still steps the field', (tester) async {
    final received = await _pumpIntInput(tester, value: 5);

    final pointer = TestPointer(1, PointerDeviceKind.mouse);
    final center = tester.getCenter(find.byType(TextField));
    await tester.sendEventToBinding(pointer.hover(center));
    // Wheel up increments, wheel down decrements.
    await tester.sendEventToBinding(pointer.scroll(const Offset(0, -20)));
    await tester.pump();
    await tester.sendEventToBinding(pointer.scroll(const Offset(0, 20)));
    await tester.sendEventToBinding(pointer.scroll(const Offset(0, 20)));
    await tester.pump();
    expect(received, [6, 5, 4]);
  });

  testWidgets('a step is based on unsubmitted typed text', (tester) async {
    final received = await _pumpIntInput(tester, value: 5);

    await tester.enterText(find.byType(TextField), '40');
    await tester.tap(_plus);
    await tester.pump();
    expect(received, [41]);
    expect(find.text('41'), findsOneWidget);
  });

  testWidgets('typed text commits on Enter and invalid text snaps back',
      (tester) async {
    final received = await _pumpIntInput(tester, value: 5, min: 0, max: 9);

    await tester.enterText(find.byType(TextField), '7');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    expect(received, [7]);

    await tester.enterText(find.byType(TextField), '42');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    expect(received, [7], reason: 'out of range is rejected');
    expect(find.text('7'), findsOneWidget, reason: 'and the box snaps back');
  });

  testWidgets('the vector input has no buttons but steps per axis',
      (tester) async {
    final received = <APIIVec3>[];
    await _pumpVectorInput(tester, received);

    expect(_plus, findsNothing);
    expect(_minus, findsNothing);

    final yField = find.byKey(const Key('y'));
    await tester.tap(yField);
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(received.length, 1);
    expect(
        (received.single.x, received.single.y, received.single.z), (1, 3, 3));
  });
}

Future<void> _pumpVectorInput(
    WidgetTester tester, List<APIIVec3> received) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: Center(
          child: SizedBox(
            width: 280,
            child: IVec3Input(
              label: 'v',
              value: const APIIVec3(x: 1, y: 2, z: 3),
              yInputKey: const Key('y'),
              onChanged: received.add,
            ),
          ),
        ),
      ),
    ),
  );
}
