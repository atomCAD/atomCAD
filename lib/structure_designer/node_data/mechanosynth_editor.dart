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
