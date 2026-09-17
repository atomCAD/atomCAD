/// The `mechanosynth` panel's **time** row and the readout lines under it:
/// where inside the selected step the scene is taken, and what the moving tool
/// is doing there.
///
/// Extracted the way [MechanosynthScrubber] was — plain numbers and callbacks,
/// no kernel — so the behaviour that is easy to get wrong can be tested without
/// a designer instance.
///
/// **This slider writes during the drag, and the step scrubber does not — but
/// it writes at most once per frame.** The two look alike and their commit
/// rules differ, deliberately. A step tick is a full replay of the build, so the
/// scrubber holds the dragged value and commits on release
/// (`lib/structure_designer/AGENTS.md`); a time tick re-evaluates one step of a
/// replay the node has already done, and *the viewport moving under the hand is
/// the feature* — a time slider that only landed on release would show nothing
/// of the visit it exists to show.
///
/// That is only affordable because the writes are **coalesced to one per
/// frame**. A write is `setMechanosynthData` → `refresh_structure_designer_auto`
/// over `frb(sync)`: an evaluation, a tessellation of the whole scene and a GPU
/// upload, all on the UI thread. `Slider` already drops a repeat of the same
/// discretised value, but a full traversal still crosses [TIME_DIVISIONS]
/// divisions, and writing each one as the pointer handler delivers it queues a
/// hundred blocking refreshes for a gesture the machine can afford perhaps ten
/// of — which is a drag that crawls seconds behind the pointer. So the pointer's
/// value is held in [_MechanosynthTimeRowState._dragValue], the slider *renders*
/// from it (the thumb tracks the hand at full frame rate whatever the kernel
/// costs), and one write is queued for the end of the frame carrying whatever
/// the latest value turned out to be. The frame paints before the thread is
/// handed over, which is `runExecuteWithPlacard`'s `endOfFrame` reasoning and
/// the guarded-post-frame idiom of `structure_designer_viewport.dart`'s
/// `renderingNeeded`.
///
/// `isosurface`'s drag bracket is still the precedent for the undo coalescing.
///
/// Design doc: `doc/design_mechanosynth_trajectory.md` §API and panel.
library;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_scrubber.dart';

/// Width of the label column, and of the numeric box beside the slider. The
/// same label width as the step row above it, so the two sliders line up.
const double TIME_LABEL_WIDTH = 48.0;
const double TIME_BOX_WIDTH = 72.0;

/// How finely the slider's travel is divided. A hundredth of a step is finer
/// than the eye follows and coarse enough that a drag does not ask for an
/// evaluation per pixel; a typed value in the box beside it is not snapped.
const int TIME_DIVISIONS = 100;

/// The division the reaction tick sits on: step time `0.5`, where the workpiece
/// switches from before to after for every kind of step. Mirrors `REACTION` in
/// `mechanosynth/trajectory/path.rs`.
const int REACTION_DIVISION = TIME_DIVISIONS ~/ 2;

/// Below this the approach is called *vertical* rather than tilted by a
/// fraction of a degree that would round to zero anyway.
const double VERTICAL_DEGREES = 0.5;

/// The contact-ratio cap the kernel reports when a scan found nothing at all.
/// Mirrors `MAX_REPORTED_CONTACT` in `nodes/mechanosynth.rs`.
const double MAX_REPORTED_CONTACT = 2.0;

/// The `time` row: a slider over `[0, 1]` with a tick at the reaction, and a
/// float box beside it.
class MechanosynthTimeRow extends StatefulWidget {
  /// The point the outputs were computed at — the wired pin's value when one is
  /// connected, else the stored property, clamped.
  final double value;

  /// `false` when a wire supplies the time, so editing it here would edit
  /// something the node ignores.
  final bool enabled;

  /// Fired at most **once per frame** while the pointer moves, always with the
  /// latest value the pointer reached, and once more on release; and directly
  /// on a value typed into the box. See the library doc for why it is neither
  /// every tick nor only the release.
  final ValueChanged<double> onChanged;

  /// Bracket the drag, so the kernel coalesces its ticks into one undo entry.
  final VoidCallback? onDragStart;
  final VoidCallback? onDragEnd;

  const MechanosynthTimeRow({
    super.key,
    required this.value,
    required this.onChanged,
    this.enabled = true,
    this.onDragStart,
    this.onDragEnd,
  });

  @override
  State<MechanosynthTimeRow> createState() => _MechanosynthTimeRowState();
}

class _MechanosynthTimeRowState extends State<MechanosynthTimeRow> {
  /// Where the pointer is, while a drag is in flight; `null` when the row is
  /// rendering the kernel's own number.
  ///
  /// The slider renders from this rather than from `widget.value`, so the thumb
  /// follows the hand at frame rate even while the kernel is several writes
  /// behind. The readout lines under the row deliberately do **not** — they
  /// describe the last evaluation, and naming a time the outputs were not
  /// computed at is the one thing they must not do.
  double? _dragValue;

  /// Whether a write is already queued for the end of this frame. The whole
  /// point: the pointer may deliver several ticks between two frames and they
  /// collapse into one write carrying the last of them.
  bool _writeQueued = false;

  /// The value the last write carried, so a release that lands on the value the
  /// frame already wrote does not spend a second refresh saying the same thing.
  /// Cleared at both ends of the drag, so it can never suppress a later write.
  double? _lastWritten;

  /// True between `onChangeStart` and `onChangeEnd`. It exists so that a drag
  /// whose end never arrives — the node deselected mid-gesture — still closes
  /// the kernel's coalescing session, exactly as the step scrubber's `_preview`
  /// does.
  bool _dragging = false;

  @override
  void dispose() {
    if (_dragging) widget.onDragEnd?.call();
    super.dispose();
  }

  /// The number the row renders: the pointer's while dragging, else the
  /// kernel's.
  double get _shown => (_dragValue ?? widget.value).clamp(0.0, 1.0);

  void _write(double value) {
    if (_lastWritten == value) return;
    _lastWritten = value;
    widget.onChanged(value);
  }

  /// Takes the value under the pointer and queues **one** write for the end of
  /// the frame.
  ///
  /// The `setState` both moves the thumb and schedules the frame the callback
  /// rides on, so there is no case where a queued write waits for a frame that
  /// never comes.
  void _onSliderChanged(double value) {
    setState(() => _dragValue = value);
    if (_writeQueued) return;
    _writeQueued = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _writeQueued = false;
      // The gesture may have ended, or the panel gone, between the queue and
      // the frame; in both cases `_dragValue` is null and the release has
      // already written the final value.
      final pending = _dragValue;
      if (!mounted || pending == null) return;
      _write(pending);
    });
  }

  void _startDrag() {
    _dragging = true;
    _lastWritten = null;
    widget.onDragStart?.call();
  }

  void _endDrag(double value) {
    _dragging = false;
    final last = _dragValue ?? value;
    // Cleared *before* the write, which notifies listeners synchronously — the
    // rebuild that follows must read the node data, not a stale drag value.
    // It also disarms whatever the queued callback would have written.
    setState(() => _dragValue = null);
    _write(last);
    _lastWritten = null;
    widget.onDragEnd?.call();
  }

  @override
  Widget build(BuildContext context) {
    final live = widget.enabled;
    final shown = _shown;
    return Row(
      children: [
        SizedBox(
          width: TIME_LABEL_WIDTH,
          child: Text('Time', style: Theme.of(context).textTheme.bodySmall),
        ),
        Expanded(
          child: SliderTheme(
            // One accent mark, at the reaction. The default shape would draw a
            // dot per division — a hundred of them — and Flutter's density gate
            // would then hide the lot; [PhaseTickMarkShape] is the same
            // reported-zero-width trick the step scrubber's phase marks use,
            // and it is reused rather than copied for exactly that reason.
            data: SliderTheme.of(context).copyWith(
              tickMarkShape: const PhaseTickMarkShape(
                boundaries: {REACTION_DIVISION},
                steps: TIME_DIVISIONS,
              ),
            ),
            child: Slider(
              key: const Key('mechanosynth_time_slider'),
              value: shown,
              min: 0.0,
              max: 1.0,
              divisions: TIME_DIVISIONS,
              label: shown.toStringAsFixed(2),
              onChanged: live ? _onSliderChanged : null,
              onChangeStart: (_) => _startDrag(),
              onChangeEnd: _endDrag,
            ),
          ),
        ),
        SizedBox(
          width: TIME_BOX_WIDTH,
          child: FloatInput(
            inputKey: const Key('mechanosynth_time_box'),
            label: '',
            value: shown,
            enabled: live,
            // A typed value is one edit, not a gesture: it goes straight
            // through, with no frame to wait for and no drag to bracket.
            onChanged: live
                ? (value) => widget.onChanged(value.clamp(0.0, 1.0))
                : (_) {},
          ),
        ),
      ],
    );
  }
}

/// The readout lines under the time row: what the tool is doing, what the sweep
/// found, and what the path scan found.
///
/// All three are facts about the **visit**, so all three are absent when every
/// tool is parked — a `bulk` step, a `spontaneous` step outside a run, tools
/// unwired. The kernel says so by leaving [APIMechanosynthInfo.leg] empty, which
/// is the one flag the panel reads rather than re-deriving from the numbers: a
/// clearance at its cap is also what a wide-open site reports.
class MechanosynthMotionLines extends StatelessWidget {
  final APIMechanosynthInfo? info;

  const MechanosynthMotionLines({super.key, required this.info});

  @override
  Widget build(BuildContext context) {
    final info = this.info;
    if (info == null || info.leg.isEmpty) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;
    final blocked = info.approachClearance <= 0.0;
    final collides = info.collision.isNotEmpty;

    return Padding(
      padding: const EdgeInsets.only(top: 6.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            info.leg,
            key: const Key('mechanosynth_leg_line'),
            style: TextStyle(fontSize: 11.5, color: scheme.onSurface),
          ),
          Text(
            approachLine(info),
            key: const Key('mechanosynth_approach_line'),
            style: TextStyle(
              fontSize: 11.5,
              color: blocked ? scheme.error : scheme.onSurfaceVariant,
            ),
          ),
          Text(
            pathLine(info),
            key: const Key('mechanosynth_path_line'),
            style: TextStyle(
              fontSize: 11.5,
              color: collides ? scheme.error : scheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}

/// `approach: vertical, clear by 1.8 Å`, `approach: tilted 23°, clear by
/// 0.5 Å`, or `approach: blocked — best is tilted 41°, 0.3 Å short`.
///
/// A blocked site is not an error anywhere: the tool visits along the
/// least-blocked direction and this line, in the warning colour, is what says
/// so (`doc/design_mechanosynth_trajectory.md` §Architecture).
String approachLine(APIMechanosynthInfo info) {
  final tilt = info.tiltDegrees < VERTICAL_DEGREES
      ? 'vertical'
      : 'tilted ${info.tiltDegrees.toStringAsFixed(0)}°';
  if (info.approachClearance <= 0.0) {
    return 'approach: blocked — best is $tilt, '
        '${(-info.approachClearance).toStringAsFixed(1)} Å short';
  }
  return 'approach: $tilt, clear by '
      '${info.approachClearance.toStringAsFixed(1)} Å';
}

/// `path clear`, `path clear (worst 1.32)`, or the kernel's own collision
/// sentence — `path collides at 41 %: O of si_tool against Si 481, ratio 0.62`.
///
/// The collision sentence names atoms and elements the panel cannot see, so the
/// kernel builds it whole; what the panel decides is only whether to use it and
/// what colour to draw it in.
String pathLine(APIMechanosynthInfo info) {
  if (info.collision.isNotEmpty) return info.collision;
  // At the cap nothing came within the scan radius at any sample, and there is
  // no number worth printing.
  if (info.contactRatio >= MAX_REPORTED_CONTACT) return 'path clear';
  return 'path clear (worst ${info.contactRatio.toStringAsFixed(2)})';
}
