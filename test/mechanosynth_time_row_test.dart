/// The `mechanosynth` panel's time row and its readout lines
/// (`doc/design_mechanosynth_trajectory.md` Phase 3).
///
/// The widget takes plain numbers and callbacks rather than reaching for the
/// model, so this needs no kernel. What is worth pinning is the handful of
/// things that make this row *not* the step scrubber beside it: it writes
/// **during** the drag where the scrubber writes on release, but at most once
/// per frame — which is the whole reason the drag is usable, since a write is a
/// blocking evaluation plus a tessellation of the scene — while the thumb still
/// tracks the pointer on every tick; the drag is bracketed exactly once at each
/// end so Ctrl+Z undoes the gesture rather than its ticks; the row goes dead
/// when the `time` pin is wired; and the tick marks the reaction at the middle
/// of the travel.
///
/// The two readout lines are pure string functions over the kernel's numbers,
/// so they are asserted directly: what matters about them is which words a
/// blocked site and a colliding flight get, and that a hover — which has no
/// landing at all — is not described as an approach.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_scrubber.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_time_row.dart';

APIMechanosynthInfo _info({
  double time = 0.3,
  String leg = 'descending',
  double tiltDegrees = 0.0,
  double approachClearance = 1.8,
  double contactRatio = MAX_REPORTED_CONTACT,
  double contactAt = 0.0,
  String collision = '',
}) =>
    APIMechanosynthInfo(
      count: 6,
      applied: 3,
      currentOp: 'habst',
      currentNote: '',
      currentMethod: 'tip',
      currentPhase: '',
      currentLayer: -1,
      currentSite: -1,
      chapters: const [],
      currentToolType: 'habst_tool',
      currentAgent: '',
      tools: const [],
      feedstocks: const [],
      time: time,
      leg: leg,
      tiltDegrees: tiltDegrees,
      approachClearance: approachClearance,
      contactRatio: contactRatio,
      contactAt: contactAt,
      collision: collision,
    );

Widget _host(Widget child) => MaterialApp(
      home: Scaffold(body: SizedBox(width: 400, child: child)),
    );

void main() {
  group('the time row', () {
    testWidgets('writes during the drag, not only on release', (tester) async {
      // The opposite of the step scrubber's rule, and deliberately so: a time
      // tick re-evaluates one step of a replay the node has already done, and
      // the viewport moving under the hand is the feature.
      final reported = <double>[];
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.0,
        onChanged: reported.add,
      )));

      final slider = find.byKey(const Key('mechanosynth_time_slider'));
      final gesture = await tester
          .startGesture(tester.getTopLeft(slider) + const Offset(24, 0));
      await tester.pump();
      await gesture.moveTo(tester.getCenter(slider));
      await tester.pump();
      await gesture.moveTo(tester.getTopRight(slider) - const Offset(24, 0));
      await tester.pump();

      expect(reported.length, greaterThan(1),
          reason: 'the viewport follows the hand, before the release');
      expect(reported.last, greaterThan(reported.first));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('collapses the ticks of one frame into a single write',
        (tester) async {
      // The reason the drag is usable at all. A write is a blocking evaluation
      // plus a tessellation of the whole scene on the UI thread, and a full
      // traversal crosses a hundred divisions; writing each one as the pointer
      // handler delivers it queues far more work than a gesture can afford.
      final reported = <double>[];
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.0,
        onChanged: reported.add,
      )));

      final slider = find.byKey(const Key('mechanosynth_time_slider'));
      final left = tester.getTopLeft(slider);
      final gesture = await tester.startGesture(left + const Offset(24, 0));
      await tester.pump();
      reported.clear();

      // Three ticks with no frame between them.
      await gesture.moveTo(left + const Offset(80, 0));
      await gesture.moveTo(left + const Offset(140, 0));
      await gesture.moveTo(left + const Offset(200, 0));
      expect(reported, isEmpty,
          reason: 'nothing is written from the pointer handler itself');

      await tester.pump();
      expect(reported.length, 1,
          reason: 'one frame, one write, carrying the latest value');

      await gesture.up();
      await tester.pump();
    });

    testWidgets('moves the thumb on every tick even so', (tester) async {
      // The coalescing must not be visible in the control: the row renders from
      // the pointer's value, so the thumb tracks the hand at frame rate however
      // far behind the kernel is.
      var writes = 0;
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.0,
        onChanged: (_) => writes++,
      )));

      final slider = find.byKey(const Key('mechanosynth_time_slider'));
      final left = tester.getTopLeft(slider);
      final gesture = await tester.startGesture(left + const Offset(24, 0));
      await tester.pump();

      await gesture.moveTo(left + const Offset(100, 0));
      await gesture.moveTo(left + const Offset(260, 0));
      await tester.pump();

      // The host never rebuilt this widget with a new `value` — the thumb is
      // where the pointer left it regardless.
      final rendered = tester.widget<Slider>(slider).value;
      expect(rendered, greaterThan(0.3));
      expect(writes, 1);

      await gesture.up();
      await tester.pump();
    });

    testWidgets('always writes where the pointer let go', (tester) async {
      // The trailing write. Without it the kernel would keep whatever the last
      // frame happened to catch, and the scene would not match the thumb.
      final reported = <double>[];
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.0,
        onChanged: reported.add,
      )));

      final slider = find.byKey(const Key('mechanosynth_time_slider'));
      final left = tester.getTopLeft(slider);
      final gesture = await tester.startGesture(left + const Offset(24, 0));
      await tester.pump();
      await gesture.moveTo(left + const Offset(180, 0));
      await tester.pump();
      final midDrag = reported.last;

      // A last move and the release inside one frame, so the queued write never
      // runs and only the release can carry the final value.
      await gesture.moveTo(tester.getTopRight(slider));
      await gesture.up();
      await tester.pump();

      expect(reported.last, 1.0,
          reason: 'the kernel ends where the pointer let go');
      expect(midDrag, lessThan(1.0),
          reason: 'and that is past what the frame before it had written');
    });

    testWidgets('does not write the release value twice', (tester) async {
      // A release that lands on the value the frame already wrote must not
      // spend a second refresh saying the same thing.
      final reported = <double>[];
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.0,
        onChanged: reported.add,
      )));

      final slider = find.byKey(const Key('mechanosynth_time_slider'));
      final left = tester.getTopLeft(slider);
      final gesture = await tester.startGesture(left + const Offset(24, 0));
      await tester.pump();
      reported.clear();

      await gesture.moveTo(left + const Offset(200, 0));
      await tester.pump();
      expect(reported.length, 1);

      // Released without moving again.
      await gesture.up();
      await tester.pump();
      expect(reported.length, 1, reason: 'the release repeated nothing');
    });

    testWidgets('brackets the drag exactly once at each end', (tester) async {
      var starts = 0;
      var ends = 0;
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.0,
        onChanged: (_) {},
        onDragStart: () => starts++,
        onDragEnd: () => ends++,
      )));

      final slider = find.byKey(const Key('mechanosynth_time_slider'));
      final gesture = await tester.startGesture(tester.getCenter(slider));
      await tester.pump();
      await gesture.moveBy(const Offset(40, 0));
      await tester.pump();
      await gesture.up();
      await tester.pump();

      expect(starts, 1);
      expect(ends, 1, reason: 'one undo entry per gesture, not per tick');
    });

    testWidgets('closes an open drag session when it is torn down',
        (tester) async {
      // A panel torn down mid-gesture — the node deselected — would otherwise
      // leave the kernel coalescing and swallow the next undo entry.
      var ends = 0;
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.0,
        onChanged: (_) {},
        onDragEnd: () => ends++,
      )));

      final slider = find.byKey(const Key('mechanosynth_time_slider'));
      final gesture = await tester.startGesture(tester.getCenter(slider));
      await tester.pump();
      await tester.pumpWidget(_host(const SizedBox.shrink()));
      expect(ends, 1);
      await gesture.up();
    });

    testWidgets('is dead when the time pin is wired', (tester) async {
      final reported = <double>[];
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.4,
        enabled: false,
        onChanged: reported.add,
      )));

      final slider = tester
          .widget<Slider>(find.byKey(const Key('mechanosynth_time_slider')));
      expect(slider.onChanged, isNull);
      expect(slider.value, 0.4, reason: 'the wired value is still shown');
      expect(reported, isEmpty);
    });

    testWidgets('marks the reaction at the middle of the travel',
        (tester) async {
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 0.3,
        onChanged: (_) {},
      )));

      final slider = tester
          .widget<Slider>(find.byKey(const Key('mechanosynth_time_slider')));
      expect(slider.min, 0.0);
      expect(slider.max, 1.0);
      expect(slider.divisions, TIME_DIVISIONS);

      final theme = tester.widget<SliderTheme>(find.ancestor(
        of: find.byKey(const Key('mechanosynth_time_slider')),
        matching: find.byType(SliderTheme),
      ));
      final shape = theme.data.tickMarkShape;
      expect(shape, isA<PhaseTickMarkShape>());
      expect((shape as PhaseTickMarkShape).boundaries, {REACTION_DIVISION});
      expect(shape.steps, TIME_DIVISIONS);
      // The tick is at step time 0.5, whatever the division count is.
      expect(REACTION_DIVISION / TIME_DIVISIONS, 0.5);
    });

    testWidgets('clamps a value from outside the travel', (tester) async {
      await tester.pumpWidget(_host(MechanosynthTimeRow(
        value: 1.7,
        onChanged: (_) {},
      )));
      final slider = tester
          .widget<Slider>(find.byKey(const Key('mechanosynth_time_slider')));
      expect(slider.value, 1.0);
    });
  });

  group('the readout lines', () {
    test('a vertical approach says so rather than naming a tilt of 0°', () {
      expect(approachLine(_info(tiltDegrees: 0.0, approachClearance: 1.84)),
          'approach: vertical, clear by 1.8 Å');
    });

    test('a tilted approach names the angle', () {
      expect(approachLine(_info(tiltDegrees: 23.2, approachClearance: 0.5)),
          'approach: tilted 23°, clear by 0.5 Å');
    });

    test('a blocked site says how far short the best direction is', () {
      // The tool visits anyway — a blocked approach is a report, never an
      // error — so the line has to say both that it is blocked and which way
      // the tool went.
      expect(approachLine(_info(tiltDegrees: 41.0, approachClearance: -0.3)),
          'approach: blocked — best is tilted 41°, 0.3 Å short');
    });

    test('a clear path with nothing near it prints no number', () {
      expect(pathLine(_info()), 'path clear');
    });

    test('a clear path that came close prints how close', () {
      expect(pathLine(_info(contactRatio: 1.32)), 'path clear (worst 1.32)');
    });

    test("a collision is the kernel's own sentence, verbatim", () {
      const sentence =
          'path collides at 41 %: O of si_tool against Si 481, ratio 0.62';
      expect(
          pathLine(_info(contactRatio: 0.62, collision: sentence)), sentence);
    });

    testWidgets('say nothing at all when every tool is parked', (tester) async {
      // An empty `leg` is the kernel's way of saying nothing visits — a `bulk`
      // step, a settle outside a run, tools unwired — and it is the one flag
      // the panel reads rather than re-deriving from the numbers, because a
      // clearance at its cap is also what a wide-open site reports.
      await tester.pumpWidget(_host(MechanosynthMotionLines(
        info: _info(leg: '', approachClearance: 10.0),
      )));
      expect(find.byKey(const Key('mechanosynth_leg_line')), findsNothing);
      expect(find.byKey(const Key('mechanosynth_approach_line')), findsNothing);
      expect(find.byKey(const Key('mechanosynth_path_line')), findsNothing);
    });

    testWidgets('are absent before the first evaluation', (tester) async {
      await tester.pumpWidget(_host(const MechanosynthMotionLines(info: null)));
      expect(find.byKey(const Key('mechanosynth_leg_line')), findsNothing);
    });

    testWidgets('draw a blocked site and a collision in the error colour',
        (tester) async {
      const sentence = 'path collides at 41 %: H of probe against Si 12';
      await tester.pumpWidget(_host(MechanosynthMotionLines(
        info: _info(
          tiltDegrees: 41.0,
          approachClearance: -0.3,
          contactRatio: 0.62,
          collision: sentence,
        ),
      )));

      final context = tester.element(find.byType(MechanosynthMotionLines));
      final error = Theme.of(context).colorScheme.error;
      final approach = tester
          .widget<Text>(find.byKey(const Key('mechanosynth_approach_line')));
      final path =
          tester.widget<Text>(find.byKey(const Key('mechanosynth_path_line')));
      expect(approach.style?.color, error);
      expect(path.style?.color, error);
    });

    testWidgets('draw a clear visit in the ordinary colour', (tester) async {
      await tester.pumpWidget(_host(MechanosynthMotionLines(info: _info())));
      final context = tester.element(find.byType(MechanosynthMotionLines));
      final scheme = Theme.of(context).colorScheme;
      final approach = tester
          .widget<Text>(find.byKey(const Key('mechanosynth_approach_line')));
      expect(approach.style?.color, scheme.onSurfaceVariant);
      expect(
        tester
            .widget<Text>(find.byKey(const Key('mechanosynth_leg_line')))
            .data,
        'descending',
      );
    });
  });
}
