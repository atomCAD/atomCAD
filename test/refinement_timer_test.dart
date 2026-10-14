/// Unit tests for `RefinementTimer` (`lib/common/refinement_timer.dart`),
/// which asks the viewport for its refined frame once the view has rested
/// (`doc/design_viewport_antialiasing.md` D5). If it breaks, every Rust test
/// still passes and the refined frame simply never comes.
///
/// Mutation: arm a new timer without cancelling the previous one; the
/// re-arm test then sees two callbacks.
library;

import 'package:flutter_cad/common/refinement_timer.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('a frame owing a refinement fires once after its delay',
      (tester) async {
    var fired = 0;
    final timer = RefinementTimer(() => fired++);

    timer.onFrame(0.15);
    await tester.pump(const Duration(milliseconds: 149));
    expect(fired, 0);
    await tester.pump(const Duration(milliseconds: 1));
    expect(fired, 1);
    await tester.pump(const Duration(seconds: 1));
    expect(fired, 1);
    timer.dispose();
  });

  testWidgets('a new frame re-arms the timer from its own delay',
      (tester) async {
    var fired = 0;
    final timer = RefinementTimer(() => fired++);

    timer.onFrame(0.15);
    await tester.pump(const Duration(milliseconds: 100));
    timer.onFrame(0.15);
    // 0.15 s after the first frame: the re-armed timer has not fired.
    await tester.pump(const Duration(milliseconds: 50));
    expect(fired, 0);
    // 0.25 s after the first frame, 0.15 s after the second: it has.
    await tester.pump(const Duration(milliseconds: 100));
    expect(fired, 1);
    await tester.pump(const Duration(seconds: 1));
    expect(fired, 1);
    timer.dispose();
  });

  testWidgets('a frame owing nothing cancels a pending timer', (tester) async {
    var fired = 0;
    final timer = RefinementTimer(() => fired++);

    timer.onFrame(0.15);
    await tester.pump(const Duration(milliseconds: 100));
    timer.onFrame(null);
    await tester.pump(const Duration(seconds: 1));
    expect(fired, 0);
    timer.dispose();
  });

  testWidgets('dispose cancels a pending timer', (tester) async {
    var fired = 0;
    final timer = RefinementTimer(() => fired++);

    timer.onFrame(0.15);
    timer.dispose();
    await tester.pump(const Duration(seconds: 1));
    expect(fired, 0);
  });
}
