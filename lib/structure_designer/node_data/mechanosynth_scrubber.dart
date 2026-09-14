/// The step scrubber and chapter list, shared by the two mechanosynthesis
/// panels: `mechanosynth`'s replay slider and `mechanosynth_edit`'s cursor.
///
/// They are the same control over the same quantity — "how many steps of a
/// build script are applied" — differing only in where the number is stored and
/// what it is called. Phase 4 of `doc/design_mechanosynth_editor.md` extracts
/// it rather than copying it, because the three things that make it non-obvious
/// (the `-1` = "follow the end" convention, the commit-on-release rule, and the
/// phase tick marks) would otherwise have to be got right twice.
///
/// **The slider commits on release.** Every write goes through
/// `refresh_structure_designer_auto` on the UI thread, and a full replay plus
/// the tessellation of a workpiece is not free at 60 Hz, so the dragged value
/// is held in the state's `_preview`, the widget renders from it, and
/// [MechanosynthScrubber.onChanged] fires once in `onChangeEnd` — the rule from
/// `lib/structure_designer/AGENTS.md`, whose reference implementation is
/// `isosurface_editor.dart`. [MechanosynthScrubber.onDragStart] /
/// [MechanosynthScrubber.onDragEnd] bracket the drag so Ctrl+Z undoes the whole
/// scrub rather than walking back through it tick by tick — the editor's cursor
/// records no undo entry at all, but the bracket costs nothing there and the
/// widget must not have to know which of its two hosts it is in.
///
/// **Neither host ever writes the stored `-1` back.** A slider that reaches the
/// end of the script cannot express "and keep following it as the script
/// grows", so `-1` is *shown* at the end of the travel (the kernel's own clamp
/// arrives as `applied`) and any edit replaces it with a concrete number.
/// Leaving the control alone keeps the auto-following default.
///
/// **A long script is scrubbed by phase, not by step.** A 450-step build has a
/// dozen or so runs of consecutive steps sharing a `(phase, layer)`, and the
/// kernel hands them over ready-made — neither panel re-derives them, because
/// the parsed script never crosses the bridge. The kernel calls these runs
/// *chapters* to keep them distinct from a step's `phase` field (a phase name
/// that recurs on several layers, or resumes after a break, is several runs);
/// the panels call them **phases**, because that is the word the user already
/// knows from the script, and the layer printed on each row disambiguates the
/// rest. They drive the phase list and the accent ticks on the slider.
library;

import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Width of the label column on the step row, and of its numeric box — a 72 px
/// digit box plus the minus / plus buttons around it.
const double _SCRUB_LABEL_WIDTH = 48.0;
const double _SCRUB_BOX_WIDTH = 72.0 + AppSpacing.intSpinChromeWidth;

class MechanosynthScrubber extends StatefulWidget {
  /// The label in front of the slider — "Step" for the replayer, "Cursor" for
  /// the editor.
  final String label;

  /// How many steps the script has. `0` disables the control rather than
  /// parking it at a meaningless stop.
  final int count;

  /// The kernel's own clamp of the stored value: how many steps are applied.
  /// A stored `-1` arrives here as [count] without the widget re-deriving the
  /// rule.
  final int applied;

  /// The script's chapters, in order. Fewer than two means no tick marks — a
  /// boundary at the end of the only phase says nothing.
  final List<APIMechanosynthChapter> chapters;

  /// `false` when a wire supplies the value, so editing it here would edit
  /// something the node ignores.
  final bool enabled;

  /// Fired once per edit, with the new step count. Never during a drag.
  final ValueChanged<int> onChanged;

  /// Bracket a slider drag, so the kernel coalesces it into one undo entry.
  final VoidCallback? onDragStart;
  final VoidCallback? onDragEnd;

  /// The dragged value on every tick, and `null` on release. Nothing is
  /// written for these — it exists so a host can *stop* showing a readout it
  /// cannot recompute mid-drag (the op name of the dragged step lives in the
  /// kernel's parsed script), rather than leave the previous step's text under
  /// a slider that has moved off it.
  final ValueChanged<int?>? onPreview;

  const MechanosynthScrubber({
    super.key,
    required this.label,
    required this.count,
    required this.applied,
    required this.chapters,
    required this.onChanged,
    this.enabled = true,
    this.onDragStart,
    this.onDragEnd,
    this.onPreview,
  });

  /// The replayer's shape. `info` is null before the first evaluation.
  factory MechanosynthScrubber.fromInfo(
    APIMechanosynthInfo? info, {
    Key? key,
    String label = 'Step',
    bool enabled = true,
    required ValueChanged<int> onChanged,
    VoidCallback? onDragStart,
    VoidCallback? onDragEnd,
    ValueChanged<int?>? onPreview,
  }) =>
      MechanosynthScrubber(
        key: key,
        label: label,
        count: info?.count ?? 0,
        applied: info?.applied ?? 0,
        chapters: info?.chapters ?? const <APIMechanosynthChapter>[],
        enabled: enabled,
        onChanged: onChanged,
        onDragStart: onDragStart,
        onDragEnd: onDragEnd,
        onPreview: onPreview,
      );

  /// The editor's shape: the cursor over the **authored block only**, so the
  /// wired prefix is not part of the travel. A prefix is not editable here and
  /// including it would make most of the slider dead.
  factory MechanosynthScrubber.fromEditData(
    APIMechanosynthEditData? data, {
    Key? key,
    String label = 'Cursor',
    bool enabled = true,
    required ValueChanged<int> onChanged,
    VoidCallback? onDragStart,
    VoidCallback? onDragEnd,
    ValueChanged<int?>? onPreview,
  }) =>
      MechanosynthScrubber(
        key: key,
        label: label,
        count: data?.authored.length ?? 0,
        applied: data?.applied ?? 0,
        chapters: data?.chapters ?? const <APIMechanosynthChapter>[],
        enabled: enabled,
        onChanged: onChanged,
        onDragStart: onDragStart,
        onDragEnd: onDragEnd,
        onPreview: onPreview,
      );

  @override
  State<MechanosynthScrubber> createState() => _MechanosynthScrubberState();
}

class _MechanosynthScrubberState extends State<MechanosynthScrubber> {
  /// The step under the pointer while a slider drag is in flight. While set,
  /// the widget renders from it and nothing is written.
  int? _preview;

  @override
  void dispose() {
    // A drag whose end never arrives — the node deselected mid-gesture — would
    // otherwise leave the kernel's coalescing session open and swallow the next
    // node-data undo entry for this node.
    if (_preview != null) widget.onDragEnd?.call();
    super.dispose();
  }

  /// The value the control renders: the dragged one while dragging, else the
  /// kernel's clamp.
  int get _shown => _preview ?? widget.applied;

  void _setPreview(int? value) {
    setState(() => _preview = value);
    widget.onPreview?.call(value);
  }

  void _endDrag() {
    final step = _preview;
    // Cleared *before* the write, which notifies listeners synchronously — the
    // rebuild that follows must read the node data, not a stale preview.
    _setPreview(null);
    if (step != null) widget.onChanged(step);
    widget.onDragEnd?.call();
  }

  /// An accent mark at the end of every phase, or no marks at all when the
  /// script has fewer than two of them.
  SliderTickMarkShape _phaseTicks() {
    if (widget.count <= 0 || widget.chapters.length < 2) {
      return SliderTickMarkShape.noTickMark;
    }
    // A phase covering steps a..b ends at slider value b — which is where the
    // next phase begins, and so where clicking its row lands.
    return PhaseTickMarkShape(
      boundaries: widget.chapters.map((c) => c.lastStep).toSet(),
      steps: widget.count,
    );
  }

  @override
  Widget build(BuildContext context) {
    final count = widget.count;
    final live = count > 0 && widget.enabled;
    return Row(
      children: [
        SizedBox(
          width: _SCRUB_LABEL_WIDTH,
          child:
              Text(widget.label, style: Theme.of(context).textTheme.bodySmall),
        ),
        Expanded(
          child: SliderTheme(
            // The default tick shape would draw one dot per step, which on a
            // 450-step script is a grey smear. The phase boundaries are the
            // marks worth having; a single-phase script has none.
            data:
                SliderTheme.of(context).copyWith(tickMarkShape: _phaseTicks()),
            child: Slider(
              key: const Key('mechanosynth_step_slider'),
              value: count > 0 ? _shown.clamp(0, count).toDouble() : 0.0,
              min: 0.0,
              max: count > 0 ? count.toDouble() : 1.0,
              divisions: count > 0 ? count : null,
              label: '$_shown',
              onChanged: live ? (value) => _setPreview(value.round()) : null,
              onChangeStart: (value) {
                widget.onDragStart?.call();
                _setPreview(value.round());
              },
              onChangeEnd: (_) => _endDrag(),
            ),
          ),
        ),
        SizedBox(
          width: _SCRUB_BOX_WIDTH,
          child: IntInput(
            label: '',
            value: _shown,
            minimumValue: count > 0 ? 0 : null,
            maximumValue: count > 0 ? count : null,
            onChanged: live ? widget.onChanged : (_) {},
          ),
        ),
      ],
    );
  }
}

/// One row per phase. Clicking a row jumps to the step just *before* the
/// phase's first, so the next `+` applies its first reaction: the row is a
/// starting point for scrubbing through the phase, not a bookmark of its
/// finished state. (That state is where the *next* row lands, and the end of
/// the last phase is the end of the slider.)
///
/// The highlight follows the same cursor: the phase whose first step is next,
/// i.e. `first - 1 <= applied <= last - 1`. So clicking a row highlights that
/// row, step 0 highlights the first phase, the last step of a phase already
/// highlights the following one, and a fully applied script highlights nothing.
/// This differs from the metadata chips on purpose — they describe the last
/// applied step, the highlight describes what comes next.
///
/// Renders nothing for a script with a single phase: a list of one is
/// navigation the slider already provides.
class MechanosynthChapterList extends StatelessWidget {
  final List<APIMechanosynthChapter> chapters;

  /// How many steps are applied — the cursor the highlight follows.
  final int applied;

  /// `false` when a wire owns the step number, so a row cannot jump.
  final bool enabled;

  /// Given the step to jump *to* (one before the phase's first).
  final ValueChanged<int> onJump;

  /// The heading above the rows.
  final String title;

  const MechanosynthChapterList({
    super.key,
    required this.chapters,
    required this.applied,
    required this.onJump,
    this.enabled = true,
    this.title = 'Phases',
  });

  @override
  Widget build(BuildContext context) {
    if (chapters.length < 2) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;

    return Padding(
      padding: const EdgeInsets.only(top: 12.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: Theme.of(context).textTheme.bodySmall),
          const SizedBox(height: 4.0),
          for (final phase in chapters)
            _ChapterRow(
              phase: phase,
              isCurrent: applied >= phase.firstStep - 1 &&
                  applied <= phase.lastStep - 1,
              enabled: enabled,
              scheme: scheme,
              onTap: () => onJump(phase.firstStep - 1),
            ),
        ],
      ),
    );
  }
}

class _ChapterRow extends StatelessWidget {
  final APIMechanosynthChapter phase;
  final bool isCurrent;
  final bool enabled;
  final ColorScheme scheme;
  final VoidCallback onTap;

  const _ChapterRow({
    required this.phase,
    required this.isCurrent,
    required this.enabled,
    required this.scheme,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final parts = <String>[
      phase.phase.isEmpty ? 'untitled' : phase.phase,
      if (phase.layer >= 0) 'layer ${phase.layer}',
      phase.firstStep == phase.lastStep
          ? 'step ${phase.firstStep}'
          : 'steps ${phase.firstStep}–${phase.lastStep}',
    ];
    final color = enabled
        ? (isCurrent ? scheme.onSurface : scheme.onSurfaceVariant)
        : scheme.onSurfaceVariant.withValues(alpha: 0.5);

    return Padding(
      padding: const EdgeInsets.only(bottom: 2.0),
      child: InkWell(
        // One step *before* the phase's first, so the next scrub tick applies
        // that first step rather than skipping past it.
        onTap: enabled ? onTap : null,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 4.0, vertical: 3.0),
          decoration: BoxDecoration(
            color: isCurrent ? scheme.primary.withValues(alpha: 0.12) : null,
            borderRadius: BorderRadius.circular(3.0),
          ),
          child: Text(
            parts.join(' · '),
            style: TextStyle(
              fontSize: 12.0,
              color: color,
              fontWeight: isCurrent ? FontWeight.bold : FontWeight.normal,
            ),
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ),
    );
  }
}

/// Paints one accent mark per phase boundary and nothing at the other steps.
///
/// Two things about `Slider`'s tick-mark protocol make this shape look odd, and
/// both are load-bearing:
///
/// **The reported width is zero, and it is a density budget rather than a
/// size.** `_RenderSlider.paint` skips tick marks altogether unless
/// `trackWidth / divisions >= 3 * reportedWidth` — so a 450-step script, which
/// is precisely the script that needs phase marks, would get none at all.
/// Reporting zero opts out of that gate, and the marks are then drawn at
/// `_MARK_WIDTH` regardless. That is honest rather than a cheat: the gate asks
/// "would *every* division fit?", and this shape draws a dozen marks whatever
/// the division count.
///
/// **Which step a mark belongs to is recovered from where Flutter puts it**,
/// not passed in: `paint` is called once per division with only a centre point.
/// Inverting Flutter's own placement formula (below) is exact, and it is what
/// keeps the marks aligned with the thumb — a strip laid out separately beneath
/// the slider would have to guess the track insets and would drift.
class PhaseTickMarkShape extends SliderTickMarkShape {
  /// A mark's drawn width, and the height it reserves on the track.
  static const double _MARK_WIDTH = 2.0;
  static const double _MARK_HEIGHT = 10.0;

  /// Slider values (= "steps applied") to mark.
  final Set<int> boundaries;

  /// The slider's division count, i.e. the script's step count.
  final int steps;

  const PhaseTickMarkShape({required this.boundaries, required this.steps});

  @override
  Size getPreferredSize({
    required SliderThemeData sliderTheme,
    required bool isEnabled,
  }) =>
      const Size(0.0, _MARK_HEIGHT);

  @override
  void paint(
    PaintingContext context,
    Offset center, {
    required RenderBox parentBox,
    required SliderThemeData sliderTheme,
    required Animation<double> enableAnimation,
    required Offset thumbCenter,
    bool? isEnabled,
    required TextDirection textDirection,
  }) {
    final enabled = isEnabled ?? false;
    final track = sliderTheme.trackShape?.getPreferredRect(
      parentBox: parentBox,
      sliderTheme: sliderTheme,
      isEnabled: enabled,
      isDiscrete: true,
    );
    if (track == null || steps <= 0) return;

    // Flutter places tick `i` at
    //   left + (i / divisions) * (width - padding) + padding / 2
    // with `padding == trackRect.height` on a discrete slider. Inverted:
    final padding = track.height;
    final span = track.width - padding;
    if (span <= 0) return;
    var fraction = (center.dx - track.left - padding / 2) / span;
    if (textDirection == TextDirection.rtl) fraction = 1.0 - fraction;
    if (!boundaries.contains((fraction * steps).round())) return;

    final color = enabled
        ? (sliderTheme.activeTickMarkColor ?? sliderTheme.activeTrackColor)
        : sliderTheme.disabledActiveTickMarkColor;
    if (color == null) return;

    context.canvas.drawRect(
      Rect.fromCenter(center: center, width: _MARK_WIDTH, height: _MARK_HEIGHT),
      Paint()..color = color,
    );
  }
}
