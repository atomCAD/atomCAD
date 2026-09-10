import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for `mechanosynth` nodes — two file pickers and a step
/// scrubber.
///
/// **The step slider commits on release.** Every write goes through
/// `refresh_structure_designer_auto` on the UI thread, and a full replay plus
/// the tessellation of a workpiece is not free at 60 Hz, so the dragged value
/// is held in [_previewStep], the panel renders from it, and the kernel is
/// written once in `onChangeEnd` — the rule from
/// `lib/structure_designer/AGENTS.md`, whose reference implementation is
/// `isosurface_editor.dart`. The drag is bracketed in
/// `beginNodeDataDrag` / `endNodeDataDrag` so Ctrl+Z undoes the whole scrub
/// rather than walking back through it tick by tick.
///
/// The panel never writes the stored `-1` ("every step") back: a slider that
/// reaches the end of the script cannot express "and keep following it as the
/// script grows", so `-1` is *shown* at the end of the travel and any edit
/// replaces it with a concrete number. Leaving the control alone keeps the
/// auto-following default.
///
/// **A long script is scrubbed by chapter, not by step.** A 450-step build has
/// a dozen or so runs of consecutive steps sharing a `(phase, layer)`, and the
/// kernel hands them over ready-made in [APIMechanosynthInfo.chapters] — the
/// panel never re-derives them, because the parsed script never crosses the
/// bridge. They drive two things: the chapter list under the scrubber, and the
/// accent tick marks on the slider ([_ChapterTickMarkShape]).
///
/// Load failures are deliberately not repeated here. A bad library or script
/// surfaces on the result pin and reaches the user through the unified error
/// list; the [ErrorBanner] below is for failures of the panel's own file
/// dialogs, which have no other surface.
class MechanosynthEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIMechanosynthData? data;

  /// The script's length and the current step's readout — the panel cannot
  /// compute either, because the parsed script never crosses the bridge.
  /// Fetched by the router on every rebuild, so loading a file or rewiring the
  /// `build_file` pin cannot leave a stale count on screen.
  final APIMechanosynthInfo? info;

  /// Whether the `ops_file` / `build_file` / `step` input pins are wired. A
  /// wire overrides the stored property, so the field below it still edits
  /// something real but no longer describes what the node evaluates.
  final bool opsFileConnected;
  final bool buildFileConnected;
  final bool stepConnected;

  final StructureDesignerModel model;

  const MechanosynthEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.info,
    required this.opsFileConnected,
    required this.buildFileConnected,
    required this.stepConnected,
    required this.model,
  });

  @override
  State<MechanosynthEditor> createState() => _MechanosynthEditorState();
}

class _MechanosynthEditorState extends State<MechanosynthEditor> {
  /// Width of the label column on the step row, and of its numeric box —
  /// a 72 px digit box plus the `−` / `+` buttons around it.
  static const double _STEP_LABEL_WIDTH = 40.0;
  static const double _STEP_BOX_WIDTH = 72.0 + AppSpacing.intSpinChromeWidth;

  /// The step under the pointer while a slider drag is in flight. While set,
  /// the panel renders from it and nothing is written to the kernel.
  int? _previewStep;

  /// A failure of one of the two file dialogs; nothing else lands here.
  String? _errorMessage;

  @override
  void dispose() {
    // A drag whose end never arrives — the node deselected mid-gesture —
    // would otherwise leave the kernel's coalescing session open and swallow
    // the next node-data undo entry for this node.
    if (_previewStep != null) widget.model.endNodeDataDrag();
    super.dispose();
  }

  void _update({
    Object? opsFile = _unset,
    Object? buildFile = _unset,
    int? step,
  }) {
    final current = widget.data;
    if (current == null) return;
    widget.model.setMechanosynthData(
      widget.nodeId,
      APIMechanosynthData(
        opsFile:
            identical(opsFile, _unset) ? current.opsFile : opsFile as String?,
        buildFile: identical(buildFile, _unset)
            ? current.buildFile
            : buildFile as String?,
        step: step ?? current.step,
      ),
    );
  }

  Future<void> _browse({required bool ops}) async {
    try {
      final result = await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['json'],
        dialogTitle: ops ? 'Select operation library' : 'Select build script',
        initialDirectory:
            initialDirectoryFor(APIFileDialogPurpose.structureImport),
      );
      if (result == null || result.files.single.path == null) return;

      final filePath = result.files.single.path!;
      rememberPickedFile(APIFileDialogPurpose.structureImport, filePath);
      setState(() => _errorMessage = null);
      if (ops) {
        _update(opsFile: filePath);
      } else {
        _update(buildFile: filePath);
      }
    } catch (e) {
      setState(() => _errorMessage = 'Error browsing file: $e');
    }
  }

  /// A path field, its Browse button and — when the matching pin is wired —
  /// the one line saying the wire wins.
  Widget _buildFileRow({
    required String label,
    required String? value,
    required bool connected,
    required bool ops,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Expanded(
              child: StringInput(
                label: label,
                value: value ?? '',
                onChanged: (text) {
                  final name = text.isEmpty ? null : text;
                  if (ops) {
                    _update(opsFile: name);
                  } else {
                    _update(buildFile: name);
                  }
                },
              ),
            ),
            const SizedBox(width: 8),
            IconButton(
              onPressed: () => _browse(ops: ops),
              icon: const Icon(Icons.folder_open),
              tooltip: 'Browse',
            ),
          ],
        ),
        if (connected) _buildWiredHint('The wired pin supplies this file.'),
      ],
    );
  }

  Widget _buildWiredHint(String text) {
    final color = Theme.of(context).colorScheme.onSurfaceVariant;
    return Padding(
      padding: const EdgeInsets.only(top: 4.0),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.link, color: color, size: 16.0),
          const SizedBox(width: 6.0),
          Expanded(
            child: Text(text, style: TextStyle(color: color, fontSize: 13.0)),
          ),
        ],
      ),
    );
  }

  /// The step scrubber. The slider spans `0..count` and is disabled — rather
  /// than parked at a meaningless stop — while no script is loaded.
  Widget _buildStepGroup(BuildContext context, APIMechanosynthData data) {
    final info = widget.info;
    final count = info?.count ?? 0;
    // `applied` is the kernel's own clamp of the stored `step`, so a stored
    // `-1` reads as `count` here without the panel re-deriving the rule.
    final shown =
        _previewStep ?? (count > 0 ? (info?.applied ?? 0) : data.step);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            SizedBox(
              width: _STEP_LABEL_WIDTH,
              child: Text('Step', style: Theme.of(context).textTheme.bodySmall),
            ),
            Expanded(
              child: SliderTheme(
                // The default tick shape would draw one dot per step, which on
                // a 450-step script is a grey smear. The chapter boundaries are
                // the marks worth having; a single-chapter script has none.
                data: SliderTheme.of(context).copyWith(
                  tickMarkShape: _chapterTicks(count),
                ),
                child: Slider(
                  key: const Key('mechanosynth_step_slider'),
                  value: count > 0 ? shown.clamp(0, count).toDouble() : 0.0,
                  min: 0.0,
                  max: count > 0 ? count.toDouble() : 1.0,
                  divisions: count > 0 ? count : null,
                  label: '$shown',
                  onChanged: count > 0
                      ? (value) => setState(() => _previewStep = value.round())
                      : null,
                  onChangeStart: (value) {
                    widget.model.beginNodeDataDrag(widget.nodeId);
                    setState(() => _previewStep = value.round());
                  },
                  onChangeEnd: (_) => _endDrag(),
                ),
              ),
            ),
            SizedBox(
              width: _STEP_BOX_WIDTH,
              child: IntInput(
                label: '',
                value: shown,
                minimumValue: count > 0 ? 0 : null,
                maximumValue: count > 0 ? count : null,
                onChanged: (value) => _update(step: value),
              ),
            ),
          ],
        ),
        if (widget.stepConnected)
          _buildWiredHint('The wired pin supplies the step number.'),
      ],
    );
  }

  /// The slider's tick shape: an accent mark at the end of every chapter, or
  /// no marks at all when the script has fewer than two of them (a boundary at
  /// the end of the only chapter says nothing).
  SliderTickMarkShape _chapterTicks(int count) {
    final chapters = widget.info?.chapters ?? const <APIMechanosynthChapter>[];
    if (count <= 0 || chapters.length < 2) {
      return SliderTickMarkShape.noTickMark;
    }
    // A chapter covering steps a..b ends at slider value b — which is both the
    // state the chapter list jumps to and where the next chapter begins.
    return _ChapterTickMarkShape(
      boundaries: chapters.map((c) => c.lastStep).toSet(),
      steps: count,
    );
  }

  /// The current step's metadata as chips. Each is omitted when the script says
  /// nothing about it, so an unannotated build shows no row at all rather than
  /// a line of placeholders.
  Widget _buildMetadataChips(BuildContext context) {
    final info = widget.info;
    if (info == null || info.count == 0 || _previewStep != null) {
      return const SizedBox.shrink();
    }
    final labels = <String>[
      if (info.currentMethod.isNotEmpty) info.currentMethod,
      if (info.currentPhase.isNotEmpty) info.currentPhase,
      if (info.currentLayer >= 0) 'layer ${info.currentLayer}',
      if (info.currentSite >= 0) 'site ${info.currentSite}',
    ];
    if (labels.isEmpty) return const SizedBox.shrink();

    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.only(top: 6.0),
      child: Wrap(
        spacing: 4.0,
        runSpacing: 4.0,
        children: [
          for (final label in labels)
            Container(
              padding:
                  const EdgeInsets.symmetric(horizontal: 6.0, vertical: 2.0),
              decoration: BoxDecoration(
                color: scheme.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(4.0),
              ),
              child: Text(
                label,
                style: TextStyle(fontSize: 11.0, color: scheme.onSurface),
              ),
            ),
        ],
      ),
    );
  }

  /// One row per chapter. The body jumps to the chapter's **last** step — its
  /// finished state, which is what one wants to look at — and the leading
  /// button jumps to the step just before its first, for scrubbing through it.
  ///
  /// Hidden for a script with a single chapter: a list of one is navigation
  /// the slider already provides.
  Widget _buildChapterList(BuildContext context) {
    final info = widget.info;
    final chapters = info?.chapters ?? const <APIMechanosynthChapter>[];
    if (info == null || chapters.length < 2) return const SizedBox.shrink();

    final scheme = Theme.of(context).colorScheme;
    final captionStyle = Theme.of(context).textTheme.bodySmall;
    // "Current" follows the same rule as the readout: the chapter holding the
    // last applied step. At step 0 nothing is current.
    final applied = _previewStep ?? info.applied;

    return Padding(
      padding: const EdgeInsets.only(top: 12.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Chapters', style: captionStyle),
          const SizedBox(height: 4.0),
          for (final chapter in chapters)
            _buildChapterRow(
              context,
              chapter,
              isCurrent:
                  applied >= chapter.firstStep && applied <= chapter.lastStep,
              enabled: !widget.stepConnected,
              scheme: scheme,
            ),
        ],
      ),
    );
  }

  Widget _buildChapterRow(BuildContext context, APIMechanosynthChapter chapter,
      {required bool isCurrent,
      required bool enabled,
      required ColorScheme scheme}) {
    final parts = <String>[
      chapter.phase.isEmpty ? 'untitled' : chapter.phase,
      if (chapter.layer >= 0) 'layer ${chapter.layer}',
      chapter.firstStep == chapter.lastStep
          ? 'step ${chapter.firstStep}'
          : 'steps ${chapter.firstStep}–${chapter.lastStep}',
    ];
    final color = enabled
        ? (isCurrent ? scheme.onSurface : scheme.onSurfaceVariant)
        : scheme.onSurfaceVariant.withValues(alpha: 0.5);

    return Padding(
      padding: const EdgeInsets.only(bottom: 2.0),
      child: Row(
        children: [
          SizedBox(
            width: 24.0,
            height: 24.0,
            child: IconButton(
              padding: EdgeInsets.zero,
              iconSize: 14.0,
              icon: const Icon(Icons.first_page),
              tooltip: 'Jump to the start of this chapter',
              // One step *before* the chapter's first, so the next scrub tick
              // applies that first step rather than skipping past it.
              onPressed:
                  enabled ? () => _update(step: chapter.firstStep - 1) : null,
            ),
          ),
          const SizedBox(width: 2.0),
          Expanded(
            child: InkWell(
              onTap: enabled ? () => _update(step: chapter.lastStep) : null,
              child: Container(
                padding:
                    const EdgeInsets.symmetric(horizontal: 4.0, vertical: 3.0),
                decoration: BoxDecoration(
                  color:
                      isCurrent ? scheme.primary.withValues(alpha: 0.12) : null,
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
          ),
        ],
      ),
    );
  }

  /// Commit the scrub: one write, one evaluation, one undo entry. The preview
  /// is cleared *before* the write, which notifies listeners synchronously —
  /// the rebuild that follows must read the node data, not a stale preview.
  void _endDrag() {
    final step = _previewStep;
    setState(() => _previewStep = null);
    if (step != null) _update(step: step);
    widget.model.endNodeDataDrag();
  }

  /// The read-only line naming what the current step did. "Current" is the
  /// last step applied, so at step 0 there is nothing to name.
  Widget _buildCurrentStep(BuildContext context) {
    final info = widget.info;
    final captionStyle = Theme.of(context).textTheme.bodySmall;
    final color = Theme.of(context).colorScheme.onSurfaceVariant;

    if (info == null || info.count == 0) {
      return Text(
        'No build script loaded.',
        style: captionStyle?.copyWith(color: color),
      );
    }
    // During a drag the readout would need the *dragged* step's op name, which
    // only the kernel knows; saying nothing beats naming the wrong step.
    if (_previewStep != null) {
      return Text('Step $_previewStep / ${info.count}',
          style: captionStyle?.copyWith(color: color));
    }
    if (info.applied == 0) {
      return Text(
        'Step 0 of ${info.count} — the untouched base.',
        style: captionStyle?.copyWith(color: color),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Step ${info.applied} of ${info.count}: ${info.currentOp}',
          style: captionStyle?.copyWith(color: color),
        ),
        if (info.currentNote.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(top: 2.0),
            child: Text(
              info.currentNote,
              style: captionStyle?.copyWith(
                color: color,
                fontStyle: FontStyle.italic,
              ),
            ),
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Mechanosynth',
            nodeTypeName: 'mechanosynth',
          ),
          const SizedBox(height: 16),
          _buildFileRow(
            label: 'Operation Library',
            value: data.opsFile,
            connected: widget.opsFileConnected,
            ops: true,
          ),
          const SizedBox(height: 12),
          _buildFileRow(
            label: 'Build Script',
            value: data.buildFile,
            connected: widget.buildFileConnected,
            ops: false,
          ),
          const SizedBox(height: 12),
          _buildStepGroup(context, data),
          const SizedBox(height: 4),
          _buildCurrentStep(context),
          _buildMetadataChips(context),
          _buildChapterList(context),
          if (_errorMessage != null)
            Padding(
              padding: const EdgeInsets.only(top: 16.0),
              child: ErrorBanner(message: _errorMessage!),
            ),
        ],
      ),
    );
  }
}

/// Sentinel for [_MechanosynthEditorState._update]'s nullable string
/// parameters: `null` is a legal value there (it clears the file name), so a
/// plain `null` default could not mean "leave this one alone".
const Object _unset = Object();

/// Paints one accent mark per chapter boundary and nothing at the other steps.
///
/// Two things about `Slider`'s tick-mark protocol make this shape look odd, and
/// both are load-bearing:
///
/// **The reported width is zero, and it is a density budget rather than a
/// size.** `_RenderSlider.paint` skips tick marks altogether unless
/// `trackWidth / divisions >= 3 * reportedWidth` — so a 450-step script, which
/// is precisely the script that needs chapter marks, would get none at all.
/// Reporting zero opts out of that gate, and the marks are then drawn at
/// [_MARK_WIDTH] regardless. That is honest rather than a cheat: the gate asks
/// "would *every* division fit?", and this shape draws a dozen marks whatever
/// the division count.
///
/// **Which step a mark belongs to is recovered from where Flutter puts it**,
/// not passed in: `paint` is called once per division with only a centre point.
/// Inverting Flutter's own placement formula (below) is exact, and it is what
/// keeps the marks aligned with the thumb — a strip laid out separately beneath
/// the slider would have to guess the track insets and would drift.
class _ChapterTickMarkShape extends SliderTickMarkShape {
  /// A mark's drawn width, and the height it reserves on the track.
  static const double _MARK_WIDTH = 2.0;
  static const double _MARK_HEIGHT = 10.0;

  /// Slider values (= "steps applied") to mark.
  final Set<int> boundaries;

  /// The slider's division count, i.e. the script's step count.
  final int steps;

  const _ChapterTickMarkShape({required this.boundaries, required this.steps});

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
