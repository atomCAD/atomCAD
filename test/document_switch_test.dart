/// `withDocumentSwitch` ordering (`doc/design_multiple_documents.md` D11,
/// Phase 3).
///
/// A value typed into a field and not yet committed must be written into the
/// document that was active when it was typed — i.e. the field's commit must
/// run **before** the switch's API call. And two switches requested back to
/// back must run one after the other, never interleaved.
///
/// [DocumentSwitcher] takes its action as a plain callback, so the action here
/// is a fake that records when it ran; no Rust library is needed.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/document_switch.dart';

void main() {
  testWidgets('a pending field edit commits before the switch runs',
      (tester) async {
    final log = <String>[];
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(
        body: FloatInput(
          label: 'radius',
          value: 1.0,
          onChanged: (v) => log.add('commit $v'),
        ),
      ),
    ));

    // Typed, not submitted: the field still has focus.
    await tester.enterText(find.byType(TextField), '7.5');
    expect(log, isEmpty);

    final switcher = DocumentSwitcher();
    final done = switcher.run(() {
      log.add('switch');
      return 1;
    });
    expect(switcher.isSwitching, isTrue);

    // Let the frame the switcher waits for happen.
    await tester.pump();
    await tester.pump();
    expect(await done, 1);
    expect(log, ['commit 7.5', 'switch']);
    expect(switcher.isSwitching, isFalse);
  });

  test('back-to-back switches run one after the other', () async {
    final frames = <Completer<void>>[];
    final log = <String>[];
    final switcher = DocumentSwitcher(
      unfocus: () => log.add('unfocus'),
      waitForFrame: () {
        final frame = Completer<void>();
        frames.add(frame);
        return frame.future;
      },
    );

    final first = switcher.run(() => log.add('first'));
    final second = switcher.run(() => log.add('second'));
    await Future<void>.delayed(Duration.zero);

    // Only the first is waiting for a frame; the second has not even started
    // (it will unfocus and wait for its own frame after the first ran).
    expect(frames, hasLength(1));
    expect(log, ['unfocus']);

    frames[0].complete();
    await first;
    await Future<void>.delayed(Duration.zero);
    expect(log, ['unfocus', 'first', 'unfocus']);
    expect(frames, hasLength(2));

    frames[1].complete();
    await second;
    expect(log, ['unfocus', 'first', 'unfocus', 'second']);
  });

  test('a failing switch does not block the queue', () async {
    final log = <String>[];
    final switcher = DocumentSwitcher(
      unfocus: () {},
      waitForFrame: () => Future<void>.value(),
    );
    final failing = switcher.run<void>(() => throw StateError('refused'));
    final next = switcher.run(() => log.add('next'));
    await expectLater(failing, throwsStateError);
    await next;
    expect(log, ['next']);
    expect(switcher.isSwitching, isFalse);
  });
}
