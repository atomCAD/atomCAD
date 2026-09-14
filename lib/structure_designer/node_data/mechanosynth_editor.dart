import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_scrubber.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for `mechanosynth` nodes — a step scrubber, the phase list,
/// and the deprecated file properties where a legacy project still has them.
///
/// **The library and the steps arrive on wires now** (`ops`, `steps`), so the
/// panel has nothing to offer for them: `ops_library` and `build_script` are
/// ordinary nodes with their own panels. The two file fields appear only on a
/// node that still carries the deprecated properties, and a field disappears
/// as soon as its pin is wired, because the wire wins at evaluation and a
/// field that edits something the node ignores is a trap. The **Convert to
/// nodes** button is the one-press migration; nothing converts automatically.
/// See `doc/design_mechanosynth_editor.md`.
///
/// **The scrubber and the phase list are not this panel's**, since Phase 4:
/// they are [MechanosynthScrubber] / [MechanosynthChapterList], shared with the
/// `mechanosynth_edit` panel, and everything about their behaviour — the
/// commit-on-release rule, the `-1` convention, the phase tick marks — is
/// documented there.
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

  /// Whether the `ops` / `steps` / `step` input pins are wired. A wire
  /// overrides the matching stored property, so the property's field is hidden
  /// rather than left editing something the node ignores.
  final bool opsConnected;
  final bool stepsConnected;
  final bool stepConnected;

  final StructureDesignerModel model;

  const MechanosynthEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.info,
    required this.opsConnected,
    required this.stepsConnected,
    required this.stepConnected,
    required this.model,
  });

  @override
  State<MechanosynthEditor> createState() => _MechanosynthEditorState();
}

class _MechanosynthEditorState extends State<MechanosynthEditor> {
  /// A failure of one of the two file dialogs, or of **Convert to nodes**;
  /// nothing else lands here.
  String? _errorMessage;

  /// The step under the pointer while the scrubber is being dragged. The
  /// readout and the chips are suppressed while it is set: naming the step the
  /// slider has just left would be worse than naming none, and the dragged
  /// step's op name lives in the kernel's parsed script, which the panel cannot
  /// read mid-drag.
  int? _previewStep;

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
        // Read-only on the kernel side; the setter ignores what is sent.
        hasLegacyFiles: current.hasLegacyFiles,
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

  /// A deprecated path field and its Browse button. Only reached for a
  /// property that is actually set on a node whose matching pin is unwired —
  /// see [_buildLegacyGroup].
  Widget _buildFileRow({
    required String label,
    required String? value,
    required bool ops,
  }) {
    return Row(
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
    );
  }

  /// The deprecated file properties, shown only on a node that still has one.
  ///
  /// A property whose pin is wired gets no field: the wire supplies the value
  /// and the property is dead weight the button is about to clear. A brand-new
  /// node has neither property and sees none of this — the way to give it a
  /// library and steps is to wire `ops_library` and `build_script`.
  Widget _buildLegacyGroup(BuildContext context, APIMechanosynthData data) {
    if (!data.hasLegacyFiles) return const SizedBox.shrink();

    final showOps = data.opsFile != null && !widget.opsConnected;
    final showBuild = data.buildFile != null && !widget.stepsConnected;
    final captionStyle = Theme.of(context).textTheme.bodySmall;
    final color = Theme.of(context).colorScheme.onSurfaceVariant;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('File properties (deprecated)',
            style: captionStyle?.copyWith(color: color)),
        const SizedBox(height: 6),
        if (showOps) ...[
          _buildFileRow(
            label: 'Operation Library',
            value: data.opsFile,
            ops: true,
          ),
          const SizedBox(height: 8),
        ],
        if (showBuild) ...[
          _buildFileRow(
            label: 'Build Script',
            value: data.buildFile,
            ops: false,
          ),
          const SizedBox(height: 8),
        ],
        if (!showOps && !showBuild)
          _buildWiredHint('The wired pins supply the library and the steps.'),
        Align(
          alignment: Alignment.centerLeft,
          child: OutlinedButton.icon(
            key: const Key('mechanosynth_convert_to_nodes'),
            onPressed: _convertToNodes,
            icon: const Icon(Icons.account_tree_outlined, size: 18),
            label: const Text('Convert to nodes'),
          ),
        ),
        const SizedBox(height: 12),
      ],
    );
  }

  /// One press, one undo entry: the kernel builds the `ops_library` /
  /// `build_script` nodes, wires them in and clears the properties.
  void _convertToNodes() {
    final error = widget.model.convertMechanosynthFilesToNodes(widget.nodeId);
    setState(() => _errorMessage = error);
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
    final info = widget.info;

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
          _buildLegacyGroup(context, data),
          MechanosynthScrubber.fromInfo(
            info,
            enabled: !widget.stepConnected,
            onChanged: (step) => _update(step: step),
            onDragStart: () => widget.model.beginNodeDataDrag(widget.nodeId),
            onDragEnd: widget.model.endNodeDataDrag,
            onPreview: (step) => setState(() => _previewStep = step),
          ),
          if (widget.stepConnected)
            _buildWiredHint('The wired pin supplies the step number.'),
          const SizedBox(height: 4),
          _buildCurrentStep(context),
          _buildMetadataChips(context),
          MechanosynthChapterList(
            chapters: info?.chapters ?? const <APIMechanosynthChapter>[],
            applied: _previewStep ?? info?.applied ?? 0,
            enabled: !widget.stepConnected,
            onJump: (step) => _update(step: step),
          ),
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
