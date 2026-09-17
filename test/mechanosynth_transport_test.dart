/// The `mechanosynth` panel's transport row — the play driver
/// (`doc/design_mechanosynth_trajectory.md` Phase 5).
///
/// The widget takes plain numbers and callbacks rather than reaching for the
/// model, so this needs no kernel, and `tester.pump(Duration(...))` drives the
/// `Ticker` — the test's clock is the widget's clock.
///
/// What is worth pinning is everything a hand cannot check reliably: that a
/// held button advances by the wall clock rather than by a fixed amount per
/// frame; that crossing a step boundary lands on the next step at time zero,
/// which is the whole reason the row exists; that a tick which *could* cross
/// several boundaries advances exactly one step, because carrying the excess
/// would silently skip steps on a slow machine; that the undo bracket is opened
/// once and closed on every exit, including the two that are not a release (the
/// end of the script, and disposal of a panel whose node was deselected
/// mid-play); and that a wire on either pin takes the row out of service.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/structure_designer/node_data/mechanosynth_transport.dart';

/// A press, a run and a release, recorded as the panel would see them.
class _Recorder {
  final writes = <(int, double)>[];
  final speeds = <int>[];
  int starts = 0;
  int ends = 0;

  void onChanged(int step, double time) => writes.add((step, time));
  void onDragStart() => starts++;
  void onDragEnd() => ends++;
}

/// One step's dwell at the **default** speed, as a `Duration` — the unit every
/// pump below is written in. A test that sets its own speed says so.
final Duration _step = _fraction(1.0);

/// A fraction of one step's dwell at `speed`.
Duration _fraction(double f, {int speed = DEFAULT_PLAY_SPEED}) => Duration(
    microseconds:
        (playStepSeconds(speed) * f * Duration.microsecondsPerSecond).round());

Widget _host({
  required _Recorder recorder,
  int count = 6,
  int step = 0,
  double time = 0.0,
  List<int> playable = const <int>[],
  int speed = DEFAULT_PLAY_SPEED,
  bool enabled = true,
}) =>
    MaterialApp(
      home: Scaffold(
        body: SizedBox(
          width: 400,
          child: MechanosynthTransportRow(
            count: count,
            step: step,
            time: time,
            playable: playable,
            speed: speed,
            onSpeedChanged: recorder.speeds.add,
            enabled: enabled,
            onChanged: recorder.onChanged,
            onDragStart: recorder.onDragStart,
            onDragEnd: recorder.onDragEnd,
          ),
        ),
      ),
    );

final _play = find.byKey(const Key('mechanosynth_play_button'));
final _rewind = find.byKey(const Key('mechanosynth_rewind_button'));

void main() {
  // Every pump below is written in dwells at the default speed; if the base or
  // the default is ever tuned, they still are.
  assert(playStepSeconds(DEFAULT_PLAY_SPEED) == 1.0);

  group('the play driver', () {
    testWidgets('advances the time by the wall clock while held',
        (tester) async {
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.5));

      expect(rec.writes, isNotEmpty);
      final (step, time) = rec.writes.last;
      expect(step, 0);
      expect(time, closeTo(0.5, 1e-6));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('crosses a step boundary onto the next step at time zero',
        (tester) async {
      // The entire point of the control: advancing the step without resetting
      // the time is what makes a hand-scrubbed build break at every boundary.
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec, step: 2, time: 0.9));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.2));

      expect(rec.writes.last, (3, 0.0));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('advances at most one step per tick, however long the frame',
        (tester) async {
      // A slow scene plays slow. Carrying the excess would skip steps and show
      // a build that never happened.
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_step * 10);

      expect(rec.writes.last, (1, 0.0));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('writes the step and the time together, never one alone',
        (tester) async {
      // One `setMechanosynthData` per tick, so no frame can show the new step
      // at the old time.
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec, step: 1, time: 0.95));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.1));

      // Every write carries a whole position; the boundary write is (2, 0.0)
      // and there is no intermediate (2, 0.95) or (1, 0.0).
      expect(rec.writes, contains((2, 0.0)));
      for (final (step, time) in rec.writes) {
        expect(step, anyOf(1, 2));
        expect(time, inInclusiveRange(0.0, 1.0));
      }

      await gesture.up();
      await tester.pump();
    });
  });

  group('the ends of the script', () {
    testWidgets('stops itself at the last step, with the pointer still down',
        (tester) async {
      final rec = _Recorder();
      await tester
          .pumpWidget(_host(recorder: rec, count: 2, step: 2, time: 0.5));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_step);

      expect(rec.writes.last, (2, 1.0));
      expect(rec.ends, 1, reason: 'the bracket closes when the script ends');

      final written = rec.writes.length;
      await tester.pump(_step * 3);
      expect(rec.writes.length, written,
          reason: 'a stopped ticker writes nothing more');

      // The release that follows must not close the bracket a second time.
      await gesture.up();
      await tester.pump();
      expect(rec.ends, 1);
    });

    testWidgets('a press at the end starts the build over', (tester) async {
      final rec = _Recorder();
      await tester
          .pumpWidget(_host(recorder: rec, count: 4, step: 4, time: 1.0));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();

      expect(rec.writes.first, (0, 0.0));

      await gesture.up();
      await tester.pump();
    });
  });

  group('the undo bracket', () {
    testWidgets('opens once on the press and closes once on the release',
        (tester) async {
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.3));
      await tester.pump(_fraction(0.3));
      expect(rec.starts, 1);
      expect(rec.ends, 0);

      await gesture.up();
      await tester.pump();
      expect(rec.starts, 1);
      expect(rec.ends, 1);
    });

    testWidgets('closes when the panel is disposed mid-play', (tester) async {
      // A node deselected while the button is held: the release never arrives,
      // and an open coalescing session would swallow the next undo entry.
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec));

      await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.2));
      expect(rec.starts, 1);
      expect(rec.ends, 0);

      await tester
          .pumpWidget(const MaterialApp(home: Scaffold(body: SizedBox())));
      expect(rec.ends, 1);
    });
  });

  group('rewind', () {
    testWidgets('writes the base and starts no clock', (tester) async {
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec, step: 3, time: 0.4));

      await tester.tap(_rewind);
      await tester.pump();

      expect(rec.writes, [(0, 0.0)]);
      expect(rec.starts, 0, reason: 'one write is already one undo entry');
      expect(rec.ends, 0);

      await tester.pump(_step);
      expect(rec.writes, [(0, 0.0)]);
    });

    testWidgets('is disabled at the base, where it has nothing to do',
        (tester) async {
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec, step: 0, time: 0.0));

      expect(tester.widget<IconButton>(_rewind).onPressed, isNull);
    });
  });

  group('a wired pin', () {
    testWidgets('takes the row out of service', (tester) async {
      // The driver has to write both `step` and `time`; playing half the pair
      // would advance a number the node ignores.
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec, enabled: false));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_step);

      expect(rec.writes, isEmpty);
      expect(rec.starts, 0);
      expect(tester.widget<IconButton>(_rewind).onPressed, isNull);

      await gesture.up();
      await tester.pump();
      expect(rec.ends, 0);
    });

    testWidgets('and so does a node with no script', (tester) async {
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec, count: 0));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_step);

      expect(rec.writes, isEmpty);
      expect(find.text('No build script'), findsOneWidget);

      await gesture.up();
      await tester.pump();
    });
  });

  group('the readout', () {
    testWidgets('names the position the sliders below would show',
        (tester) async {
      final rec = _Recorder();
      await tester
          .pumpWidget(_host(recorder: rec, count: 171, step: 12, time: 0.43));

      expect(find.text('step 12 / 171 · time 0.43'), findsOneWidget);
    });
  });

  // ==========================================================================
  // Phase 6 — the playback stops on the steps that move
  // ==========================================================================

  group('the playable steps', () {
    testWidgets('a boundary lands on the next stop, stepping over a settle',
        (tester) async {
      // 2 is the settle between the visits at 1 and 3. Playing it would break
      // the run into a movement, a second of nothing, and another movement.
      final rec = _Recorder();
      await tester.pumpWidget(_host(
          recorder: rec,
          count: 4,
          playable: const [1, 3, 4],
          step: 1,
          time: 0.9));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.2));

      expect(rec.writes.last, (3, 0.0));
      expect(rec.writes.map((w) => w.$1), isNot(contains(2)));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('a whole block collapses onto its last step in one jump',
        (tester) async {
      // A phase of bulk exposures with their settles: one beat, landing on the
      // block's last step, which is the scene every step of the block made.
      final rec = _Recorder();
      await tester.pumpWidget(_host(
          recorder: rec,
          count: 9,
          playable: const [1, 6, 9],
          step: 1,
          time: 0.9));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.2));

      expect(rec.writes.last, (6, 0.0));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('still advances one stop per tick, however long the frame',
        (tester) async {
      // The skip is the list's business; falling behind the clock is still the
      // driver's, and it must not compound the two.
      final rec = _Recorder();
      await tester.pumpWidget(_host(
          recorder: rec, count: 9, playable: const [1, 3, 5, 9], step: 1));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_step * 10);

      expect(rec.writes.last, (3, 0.0));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('plays a step that is not a stop before moving on',
        (tester) async {
      // The user scrubbed to that settle on purpose. The transport is not
      // entitled to skip the thing they are looking at.
      final rec = _Recorder();
      await tester.pumpWidget(_host(
          recorder: rec,
          count: 6,
          playable: const [1, 4, 6],
          step: 2,
          time: 0.5));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.2));
      // A record compares structurally, so the two parts are asserted apart:
      // a matcher inside one is just a value that will never equal a double.
      expect(rec.writes.last.$1, 2);
      expect(rec.writes.last.$2, closeTo(0.7, 1e-6));

      await tester.pump(_fraction(0.4));
      expect(rec.writes.last, (4, 0.0));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('step 0 opens on the first stop, not on step 1',
        (tester) async {
      final rec = _Recorder();
      await tester.pumpWidget(
          _host(recorder: rec, count: 6, playable: const [2, 5], step: 0));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_step);

      expect(rec.writes.last, (2, 0.0));

      await gesture.up();
      await tester.pump();
    });

    testWidgets('past the last stop the run ends on the script, not on it',
        (tester) async {
      // A script finishing with settles must still finish on the settled
      // workpiece, so the end write is the script's last step whether or not
      // it is a stop.
      final rec = _Recorder();
      await tester.pumpWidget(_host(
          recorder: rec, count: 6, playable: const [1, 3], step: 3, time: 0.9));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.2));

      expect(rec.writes.last, (6, 1.0));
      expect(rec.ends, 1);

      await gesture.up();
      await tester.pump();
    });

    testWidgets('an empty list is the every-step walk', (tester) async {
      // What an unwired `ops` pin comes to: no method can be established, so
      // nothing is proved skippable and the playback walks the script.
      final rec = _Recorder();
      await tester
          .pumpWidget(_host(recorder: rec, count: 4, step: 1, time: 0.9));

      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.2));

      expect(rec.writes.last, (2, 0.0));

      await gesture.up();
      await tester.pump();
    });
  });

  // ==========================================================================
  // The speed multiplier
  // ==========================================================================

  group('the speed', () {
    test('is a divisor of the base, with the default in the middle', () {
      // A multiplier has to read like one: x2 is twice as fast as x1.
      expect(playStepSeconds(1), PLAY_BASE_STEP_SECONDS);
      expect(playStepSeconds(2), PLAY_BASE_STEP_SECONDS / 2);
      expect(playStepSeconds(4), PLAY_BASE_STEP_SECONDS / 4);

      // The default is the second stop, not the first, so there is a slower
      // gear below it as well as faster ones above.
      expect(DEFAULT_PLAY_SPEED, greaterThan(MIN_PLAY_SPEED));
      expect(DEFAULT_PLAY_SPEED, lessThan(MAX_PLAY_SPEED));

      // Out-of-range values are clamped rather than dividing by zero or
      // producing a dwell no frame could resolve.
      expect(playStepSeconds(0), playStepSeconds(MIN_PLAY_SPEED));
      expect(playStepSeconds(-3), playStepSeconds(MIN_PLAY_SPEED));
      expect(playStepSeconds(999), playStepSeconds(MAX_PLAY_SPEED));
    });

    testWidgets('x1 takes twice the wall clock that x2 does', (tester) async {
      final slow = _Recorder();
      await tester.pumpWidget(_host(recorder: slow, speed: 1));
      final slowGesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(1.0, speed: 2));
      // Half a dwell at x1 is a whole one at x2.
      expect(slow.writes.last.$2, closeTo(0.5, 1e-6));
      await slowGesture.up();
      await tester.pump();
    });

    testWidgets('a faster speed crosses more boundaries in the same time',
        (tester) async {
      final fast = _Recorder();
      await tester.pumpWidget(_host(recorder: fast, count: 9, speed: 4));
      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      // One default dwell is four steps' worth at x4 — but a tick only ever
      // advances one stop, so four *pumps* are what it takes.
      for (var i = 0; i < 4; i++) {
        await tester.pump(_fraction(1.0, speed: 4));
      }
      expect(fast.writes.last.$1, 4);
      await gesture.up();
      await tester.pump();
    });

    testWidgets('the field reports an edit and refuses one out of range',
        (tester) async {
      // The host re-hosts with whatever was reported, which is what the model
      // does: `IntSpinField` reverts a rejected entry to `widget.value`, so a
      // parent that does not follow the value would show a stale number.
      final rec = _Recorder();
      var shown = DEFAULT_PLAY_SPEED;
      Future<void> type(String text) async {
        await tester.pumpWidget(_host(recorder: rec, speed: shown));
        await tester.enterText(
            find.byKey(const Key('mechanosynth_speed_field')), text);
        await tester.testTextInput.receiveAction(TextInputAction.done);
        await tester.pump();
        if (rec.speeds.isNotEmpty) shown = rec.speeds.last;
      }

      await type('5');
      expect(rec.speeds.last, 5);

      // Out of range is **rejected**, not clamped — the shared field's rule for
      // a typed value (the — / + buttons are what clamp). What matters here is
      // that no dwell outside the row's range is ever reachable.
      await type('99');
      expect(shown, 5, reason: 'the refused entry left the value alone');
      await type('0');
      expect(shown, 5);

      expect(
        rec.speeds,
        everyElement(inInclusiveRange(MIN_PLAY_SPEED, MAX_PLAY_SPEED)),
      );
    });

    testWidgets('a change mid-press takes effect without moving the scene',
        (tester) async {
      // Read per tick, so nothing is recomputed and the elapsed time is never
      // rewritten: the next tick is simply longer or shorter.
      final rec = _Recorder();
      await tester.pumpWidget(_host(recorder: rec, speed: 1));
      final gesture = await tester.startGesture(tester.getCenter(_play));
      await tester.pump();
      await tester.pump(_fraction(0.5, speed: 1));
      final atSwitch = rec.writes.last;
      expect(atSwitch.$2, closeTo(0.5, 1e-6));

      await tester.pumpWidget(_host(recorder: rec, speed: 4));
      // The rebuild alone writes nothing: the scene does not jump.
      expect(rec.writes.last, atSwitch);

      // And the next tick advances at the new rate.
      await tester.pump(_fraction(0.25, speed: 4));
      expect(rec.writes.last.$2, closeTo(0.75, 1e-6));

      await gesture.up();
      await tester.pump();
    });
  });
}
