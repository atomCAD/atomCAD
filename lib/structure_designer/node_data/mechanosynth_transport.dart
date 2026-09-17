/// The `mechanosynth` panel's **transport row**: rewind, play, and where in the
/// build the scene currently is.
///
/// Phase 3 made a build scrubbable, and scrubbing is not showing. Dragging the
/// step slider advances `step` and leaves `time` wherever the last drag left
/// it, so every step after the first is entered part-way through its own visit
/// and the motion breaks at every boundary. This row is the missing **driver**:
/// it advances `time` by itself and, when a step is used up, advances `step`
/// and puts `time` back to zero — in one write, so there is never a frame
/// showing the new step at the old time.
///
/// Design doc: `doc/design_mechanosynth_trajectory.md` §Playing the build.
///
/// **Hold to play.** The button plays while it is held and stops the moment it
/// is released: the scrub the user already knows, performed by a clock instead
/// of by a hand. Inheriting that shape is worth more than it looks — there is
/// no transport state to fall out of sync with the node, no stop button to hunt
/// for, no way to leave the panel playing, and letting go is always the way
/// out. It is also the gesture a presenter wants: press to advance, release to
/// talk, press again.
///
/// **It advances by the wall clock and never skips a step.** Each tick takes
/// the elapsed time since the previous one and adds `dt / playStepSeconds`.
/// When the time reaches `1.0` the write becomes the next step at time `0`, and
/// **the excess is dropped rather than carried**: on a machine where one step
/// costs more than a frame's budget, carrying the remainder would silently skip
/// steps and show a build that never happened. Falling behind the wall clock is
/// the acceptable failure; showing the wrong build is not.
///
/// **Every tick writes, so there is no preview state** — and this is where the
/// driver is *simpler* than the drag it resembles, which is worth stating
/// because the two look alike. A pointer delivers ticks faster than frames,
/// which is why `MechanosynthTimeRow` holds the dragged value and queues one
/// write for the end of the frame; a [Ticker] fires exactly once per frame, so
/// every tick is already the only write of that frame and there is nothing to
/// coalesce. The write is `frb(sync)` — an evaluation, a tessellation and a GPU
/// upload on the UI thread — so the frame's cost *is* the refresh's cost, and
/// the next tick's `dt` has already measured it. The sliders below this row go
/// on rendering the kernel's own numbers, as they do at rest.
///
/// **One press, one undo entry.** The press brackets the whole run with
/// [MechanosynthTransportRow.onDragStart] /
/// [MechanosynthTransportRow.onDragEnd] — `beginNodeDataDrag` /
/// `endNodeDataDrag` — so a three-minute play costs one Ctrl+Z rather than a
/// hundred and seventy-one. The bracket is closed on *every* exit and not just
/// on release: a pointer cancel, the end of the script, and `dispose` of a
/// panel whose node was deselected mid-play all stop the ticker and end the
/// session. `MechanosynthTimeRow`'s `_dragging` field is the precedent, and it
/// exists for exactly this.
///
/// **It does not walk every step.** A boundary lands on the next entry of
/// [MechanosynthTransportRow.playable] rather than on `current + 1`, so a
/// `spontaneous` settle — which holds its tool perfectly still while the
/// crystal relaxes — is stepped over, and a block of `bulk` exposures plays as
/// one beat on its last step. A run then reads as one movement instead of
/// being chopped up by seconds of stillness. Nothing is dropped: landing on
/// step `m` means the first `m − 1` steps are applied. The rule that builds the
/// list is the kernel's (`playable_steps`), because a step's *method* is a fact
/// about the operation the library names and the parsed script never crosses
/// the bridge. An empty list is every step.
///
/// **The speed is a multiplier, and the default is the middle of it.** The row
/// carries a small `×n` field; `×1` is [PLAY_BASE_STEP_SECONDS] a step and every
/// stop above it divides that. It defaults to [DEFAULT_PLAY_SPEED] — one second
/// a step, the only rate there was before — so that the default has a *slower*
/// gear below it as well as faster ones above: a step worth narrating while it
/// happens is worth two seconds, and a base-as-default would have had nowhere
/// to go but up. The value is read on each tick, so turning the speed up
/// mid-press takes effect on the next frame with nothing recomputed and
/// nothing jumping.
///
/// **The end of the script stops it; the next press starts over.** A build does
/// not loop — a finished workpiece flickering back to a bare slab is not
/// something anyone wants to watch twice — so reaching the last step writes
/// `(count, 1.0)`, stops the ticker and returns the button to its idle look
/// while the pointer is still down. A press made when the scene is already at
/// the end restarts from `(0, 0.0)`, the only thing such a press can mean.
library;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/int_spin_field.dart';

/// Wall-clock seconds one step's dwell takes at speed ×1.
///
/// The row's speed field divides this, so the multiplier reads the way a
/// multiplier should: ×2 plays twice as fast as ×1, ×4 four times. The
/// **default is ×2** — one second a step, which is long enough to read a
/// visit's shape (descend, react at the middle, retract) and is the only rate
/// the control had before it was a control. Making the default the *second*
/// stop rather than the first is what buys a slower gear: a step worth
/// explaining while it happens gets two seconds at ×1, and there was nowhere
/// to go but faster when the base was the default.
const double PLAY_BASE_STEP_SECONDS = 2.0;

/// The speed field's range and default. Past ×8 a step is under a quarter of a
/// second and the kernel is the limit rather than the setting — the driver
/// falls behind the clock by design, so a larger number would buy nothing but
/// a misleading readout.
const int MIN_PLAY_SPEED = 1;
const int MAX_PLAY_SPEED = 8;
const int DEFAULT_PLAY_SPEED = 2;

/// Seconds one step's dwell takes at `speed`.
double playStepSeconds(int speed) =>
    PLAY_BASE_STEP_SECONDS / speed.clamp(MIN_PLAY_SPEED, MAX_PLAY_SPEED);

/// The side of the transport row's two square buttons.
///
/// Deliberately larger than `AppSpacing.buttonHeight`: this row is pressed and
/// *held*, often while the presenter is looking at the viewport rather than at
/// the panel, which is a different job from a button that is clicked once with
/// the eye on it. Ten extra pixels of panel height buys a target that can be
/// hit without aiming. Both buttons take the same side so the row still reads
/// as one control rather than a big thing beside a small one.
const double TRANSPORT_BUTTON_SIZE = 40.0;

/// The play button's fill, idle and while held. Green because it is the
/// panel's one *go* action, and a literal green rather than the theme's
/// primary — the accent is blue-grey, which is what every other button in the
/// application already is. Held is the darker of the two, the direction a
/// pressed control moves in a light theme.
const Color TRANSPORT_PLAY_COLOR = Color(0xFF43A047); // Material green 600
const Color TRANSPORT_PLAY_HELD_COLOR = Color(0xFF2E7D32); // green 800

/// The row's own three gaps. Local because they are a property of this one
/// layout, not of the application's spacing scale.
const double TRANSPORT_BUTTON_GAP = 4.0;
const double TRANSPORT_READOUT_GAP = 10.0;
const double TRANSPORT_ROW_GAP = 8.0;

/// The speed box's text width. Two characters wide — the `×` and one digit —
/// because the range stops at [MAX_PLAY_SPEED].
const double TRANSPORT_SPEED_BOX_WIDTH = 40.0;

/// Rewind, play, and the position readout — the top row of the `mechanosynth`
/// panel.
///
/// Takes plain numbers and callbacks, no kernel, so the driver can be tested
/// with `tester.pump(Duration(...))` as its clock — the shape
/// `MechanosynthScrubber` and `MechanosynthTimeRow` already use.
class MechanosynthTransportRow extends StatefulWidget {
  /// How many steps the script has. `0` disables the row rather than parking it
  /// on a build that is not there.
  final int count;

  /// Where the scene is now: the kernel's own clamp of the stored step (so a
  /// stored `-1` arrives resolved) and the time the last evaluation used.
  final int step;
  final double time;

  /// The step numbers a playback stops on, **1-based** and ascending: the
  /// visits, plus the last step of each block of gating steps containing a
  /// `bulk` exposure. Straight from `APIMechanosynthInfo.playable` — the rule
  /// is the kernel's, because a step's method is not a thing the panel can
  /// see (`doc/design_mechanosynth_trajectory.md` §What playing skips).
  ///
  /// A settle between two visits is simply absent, and the driver steps over
  /// it: it holds its tool perfectly still, so playing it would break a run
  /// into a movement, a second of nothing, and another movement. Skipping it
  /// loses nothing — landing on step `m` means the first `m − 1` steps are
  /// applied, so the scene is the one the skipped steps produced.
  ///
  /// **Empty means every step**, which is what an unwired `ops` pin and a
  /// script of nothing but settles both come to. Never a reason to play
  /// nothing.
  final List<int> playable;

  /// The playback multiplier, [MIN_PLAY_SPEED]–[MAX_PLAY_SPEED]. Session
  /// state on the model, not node data: it changes no atom, so it belongs to
  /// the watching rather than to the design.
  final int speed;

  /// Fired when the speed field is edited. A speed change mid-press takes
  /// effect on the **next** tick and never rewrites the elapsed time, so
  /// nothing jumps.
  final ValueChanged<int>? onSpeedChanged;

  /// `false` when a wire supplies the step or the time. The driver has to write
  /// **both**, so either wire disables the row: playing half the pair would
  /// advance a number the node ignores.
  final bool enabled;

  /// One write per tick, carrying the step and the time together.
  final void Function(int step, double time) onChanged;

  /// Bracket the press, so the kernel coalesces a whole run into one undo
  /// entry.
  final VoidCallback? onDragStart;
  final VoidCallback? onDragEnd;

  const MechanosynthTransportRow({
    super.key,
    required this.count,
    required this.step,
    required this.time,
    required this.onChanged,
    this.playable = const <int>[],
    this.speed = DEFAULT_PLAY_SPEED,
    this.onSpeedChanged,
    this.enabled = true,
    this.onDragStart,
    this.onDragEnd,
  });

  @override
  State<MechanosynthTransportRow> createState() =>
      _MechanosynthTransportRowState();
}

class _MechanosynthTransportRowState extends State<MechanosynthTransportRow>
    with SingleTickerProviderStateMixin {
  Ticker? _ticker;

  /// The elapsed reading of the previous tick, so `dt` is a difference rather
  /// than a total. The ticker's own clock is used rather than a `Stopwatch`
  /// because it already excludes the time the app spent unscheduled.
  Duration _lastElapsed = Duration.zero;

  /// What the driver is advancing. Seeded at the press from the kernel's
  /// numbers and written on every tick; the kernel's clamp is not a place to
  /// accumulate a fraction, which is the only reason this is held at all.
  int _step = 0;
  double _time = 0.0;

  /// True between the press and whichever exit comes first. It exists so that a
  /// press whose release never arrives — the node deselected mid-play — still
  /// closes the kernel's coalescing session.
  bool _playing = false;

  @override
  void dispose() {
    _ticker?.dispose();
    _ticker = null;
    if (_playing) {
      _playing = false;
      widget.onDragEnd?.call();
    }
    super.dispose();
  }

  /// Whether there is anything to play: a script, and both pins free.
  bool get _live => widget.enabled && widget.count > 0;

  /// Whether the scene sits at the very end of the script, where the only thing
  /// a press can mean is "show it again".
  bool get _atEnd => widget.step >= widget.count && widget.time >= 1.0;

  /// The step a boundary at `current` lands on, or `null` when the run is over.
  ///
  /// The whole of Phase 6 in one function. With no list it is the step after
  /// this one; with one it is the first entry beyond it, which is how a settle
  /// gets stepped over and a block of exposures collapses onto its last step.
  /// The current step needs no membership of its own — a user who scrubbed to a
  /// settle and pressed play is looking at it on purpose, and the transport is
  /// not entitled to skip the thing they are looking at.
  int? _nextStop(int current) {
    if (widget.playable.isEmpty) {
      return current < widget.count ? current + 1 : null;
    }
    for (final stop in widget.playable) {
      if (stop > current) return stop;
    }
    return null;
  }

  void _startPlaying() {
    if (!_live || _playing) return;
    _playing = true;
    widget.onDragStart?.call();

    if (_atEnd) {
      // A press at the end is a request to watch it again; there is nowhere
      // else for it to go.
      _step = 0;
      _time = 0.0;
      widget.onChanged(_step, _time);
    } else {
      _step = widget.step.clamp(0, widget.count);
      _time = widget.time.clamp(0.0, 1.0);
    }

    _lastElapsed = Duration.zero;
    _ticker = createTicker(_onTick)..start();
    setState(() {});
  }

  /// Ends the run: stops the clock and closes the undo bracket. Idempotent, so
  /// the release that follows an end-of-script stop does nothing twice.
  void _stopPlaying() {
    if (!_playing) return;
    _playing = false;
    _ticker?.dispose();
    _ticker = null;
    widget.onDragEnd?.call();
    if (mounted) setState(() {});
  }

  void _onTick(Duration elapsed) {
    final dt = elapsed - _lastElapsed;
    _lastElapsed = elapsed;
    if (dt <= Duration.zero) return;

    // Read per tick, so turning the speed up mid-press takes effect on the next
    // tick with nothing recomputed and nothing jumping.
    final advance = dt.inMicroseconds /
        Duration.microsecondsPerSecond /
        playStepSeconds(widget.speed);
    var time = _time + advance;
    var step = _step;

    if (time >= 1.0) {
      final next = _nextStop(step);
      if (next == null) {
        // Nothing left to stop on. Land on the script's end whether or not it
        // is a stop — a script that finishes with settles must still finish on
        // the settled workpiece — then let go: the button returns to its idle
        // look with the pointer still down.
        _step = widget.count;
        _time = 1.0;
        widget.onChanged(_step, _time);
        _stopPlaying();
        return;
      }
      // At most one stop per tick, whatever the excess: a slow scene plays
      // slow, it does not skip what it could not keep up with.
      step = next;
      time = 0.0;
    }

    _step = step;
    _time = time;
    widget.onChanged(step, time);
  }

  /// Back to the untouched base, as one write and one undo entry. No bracket:
  /// a single write is already a single entry.
  void _rewind() {
    _stopPlaying();
    widget.onChanged(0, 0.0);
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final live = _live;
    final canRewind = live && !(widget.step == 0 && widget.time <= 0.0);

    return Padding(
      padding: const EdgeInsets.only(bottom: TRANSPORT_ROW_GAP),
      child: Row(
        children: [
          IconButton(
            key: const Key('mechanosynth_rewind_button'),
            onPressed: canRewind ? _rewind : null,
            icon: const Icon(Icons.skip_previous),
            iconSize: 24.0,
            visualDensity: VisualDensity.compact,
            constraints: const BoxConstraints.tightFor(
              width: TRANSPORT_BUTTON_SIZE,
              height: TRANSPORT_BUTTON_SIZE,
            ),
            padding: EdgeInsets.zero,
            tooltip: 'Back to step 0',
          ),
          const SizedBox(width: TRANSPORT_BUTTON_GAP),
          // A Listener rather than a button's onPressed: the gesture *is* the
          // control, and a tap callback would only report that it is over.
          Listener(
            onPointerDown: live ? (_) => _startPlaying() : null,
            onPointerUp: (_) => _stopPlaying(),
            onPointerCancel: (_) => _stopPlaying(),
            child: Tooltip(
              message: live
                  ? 'Hold to play the build'
                  : 'Nothing to play on this node',
              child: Container(
                key: const Key('mechanosynth_play_button'),
                width: TRANSPORT_BUTTON_SIZE,
                height: TRANSPORT_BUTTON_SIZE,
                decoration: BoxDecoration(
                  color: !live
                      ? scheme.surfaceContainerHighest
                      : (_playing
                          ? TRANSPORT_PLAY_HELD_COLOR
                          : TRANSPORT_PLAY_COLOR),
                  borderRadius: BorderRadius.circular(4.0),
                ),
                child: Icon(
                  Icons.play_arrow,
                  size: 28.0,
                  color: live ? Colors.white : scheme.onSurfaceVariant,
                ),
              ),
            ),
          ),
          const SizedBox(width: TRANSPORT_READOUT_GAP),
          // The speed, as a bare `×n` box rather than a labelled field: a caption
          // over it would double the row's height, which is the one thing this
          // row is not allowed to spend. `IntSpinField` brings the − / + buttons,
          // the wheel and the arrow keys with it (`lib/AGENTS.md`), so the
          // increment logic is not written a second time here.
          SizedBox(
            width: TRANSPORT_SPEED_BOX_WIDTH + AppSpacing.intSpinChromeWidth,
            child: IntSpinField(
              key: const Key('mechanosynth_speed_field'),
              value: widget.speed,
              minimumValue: MIN_PLAY_SPEED,
              maximumValue: MAX_PLAY_SPEED,
              onChanged: live ? (widget.onSpeedChanged ?? (_) {}) : (_) {},
              fieldConstraints: const BoxConstraints(
                minWidth: TRANSPORT_SPEED_BOX_WIDTH,
                maxWidth: TRANSPORT_SPEED_BOX_WIDTH,
              ),
              decoration: AppInputDecorations.standard.copyWith(
                prefixText: '×',
                prefixStyle: Theme.of(context).textTheme.bodySmall,
              ),
            ),
          ),
          const SizedBox(width: TRANSPORT_READOUT_GAP),
          // The position, duplicating what the two sliders below already say —
          // deliberately, so this row alone is enough to follow a demo and
          // everything under it can be scrolled away.
          Expanded(
            child: Text(
              widget.count == 0
                  ? 'No build script'
                  : 'step ${widget.step} / ${widget.count}'
                      ' · time ${formatNatural(widget.time, 2)}',
              key: const Key('mechanosynth_transport_readout'),
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: scheme.onSurfaceVariant,
                  ),
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ],
      ),
    );
  }
}
