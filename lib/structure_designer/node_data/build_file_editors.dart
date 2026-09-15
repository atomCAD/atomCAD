import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_status.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// The panels of the three mechanosynthesis file nodes: `ops_library`,
/// `build_script` and `export_build_script`.
///
/// They share a file row and a Browse dialog, so they share a file — three
/// near-identical one-field editors in three files would be three places for
/// the dialog purpose or the "the wired pin supplies this" rule to drift.
/// See `doc/design_mechanosynth_editor.md`.
///
/// **A Reload button is not the same write as the path field.** The field's
/// `onChanged` fires on every focus loss, so a write that does not change the
/// name deliberately keeps the parsed payload
/// (`project_import_node_payload_wipe`); Reload is the explicit "the file
/// changed underneath me" gesture and passes `reload: true` to force the
/// re-read.

/// The shared path row: a text field, a Browse button and, when the file-name
/// pin is wired, one line saying the wire wins.
class _FileRow extends StatelessWidget {
  final String label;
  final String? value;
  final bool connected;
  final String dialogTitle;
  final ValueChanged<String?> onChanged;
  final VoidCallback onReload;
  final ValueChanged<String> onError;

  const _FileRow({
    required this.label,
    required this.value,
    required this.connected,
    required this.dialogTitle,
    required this.onChanged,
    required this.onReload,
    required this.onError,
  });

  Future<void> _browse() async {
    try {
      final result = await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['json'],
        dialogTitle: dialogTitle,
        initialDirectory:
            initialDirectoryFor(APIFileDialogPurpose.structureImport),
      );
      if (result == null || result.files.single.path == null) return;
      final filePath = result.files.single.path!;
      rememberPickedFile(APIFileDialogPurpose.structureImport, filePath);
      onChanged(filePath);
    } catch (e) {
      onError('Error browsing file: $e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final color = Theme.of(context).colorScheme.onSurfaceVariant;
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
                onChanged: (text) => onChanged(text.isEmpty ? null : text),
              ),
            ),
            const SizedBox(width: 8),
            IconButton(
              onPressed: _browse,
              icon: const Icon(Icons.folder_open),
              tooltip: 'Browse',
            ),
            IconButton(
              onPressed: onReload,
              icon: const Icon(Icons.refresh),
              tooltip: 'Reload from disk',
            ),
          ],
        ),
        if (connected)
          Padding(
            padding: const EdgeInsets.only(top: 4.0),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(Icons.link, color: color, size: 16.0),
                const SizedBox(width: 6.0),
                Expanded(
                  child: Text('The wired pin supplies this file.',
                      style: TextStyle(color: color, fontSize: 13.0)),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

/// Editor for `ops_library` — the path, and a listing of what the file holds.
///
/// The listing is the only view of a parsed library from inside the
/// application: the parsed value is payload and never crosses the bridge, so
/// [APIOpsLibraryData] carries a summary built kernel-side. It is also how a
/// user learns an operation's name, which is what the placement tool will ask
/// for.
///
/// **The tool types come above the operations** because they are the half a
/// design has to act on. A `tip` operation names the instrument it needs, and
/// the engine finds that instrument by **atom tags** — the type's own name on
/// the molecule and one frame tag on each of four atoms. Those tags are listed
/// here, and nowhere else in the application, so this section is the
/// instruction for making a molecule usable as a tool.
class OpsLibraryEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIOpsLibraryData? data;

  /// Whether the `file` input pin is wired; a wire overrides the property.
  final bool fileConnected;
  final StructureDesignerModel model;

  const OpsLibraryEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.fileConnected,
    required this.model,
  });

  @override
  State<OpsLibraryEditor> createState() => _OpsLibraryEditorState();
}

class _OpsLibraryEditorState extends State<OpsLibraryEditor> {
  String? _errorMessage;

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final captionStyle = Theme.of(context).textTheme.bodySmall;
    final color = Theme.of(context).colorScheme.onSurfaceVariant;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Operation Library',
            nodeTypeName: 'ops_library',
          ),
          const SizedBox(height: 16),
          _FileRow(
            label: 'File',
            value: data.file,
            connected: widget.fileConnected,
            dialogTitle: 'Select operation library',
            onChanged: (name) {
              setState(() => _errorMessage = null);
              widget.model.setOpsLibraryFile(widget.nodeId, name);
            },
            onReload: () {
              setState(() => _errorMessage = null);
              widget.model
                  .setOpsLibraryFile(widget.nodeId, data.file, reload: true);
            },
            onError: (message) => setState(() => _errorMessage = message),
          ),
          const SizedBox(height: 12),
          if (data.ops.isEmpty)
            Text('No library loaded.',
                style: captionStyle?.copyWith(color: color))
          else ...[
            Text(
              '${data.ops.length} operations · tolerance '
              '${formatNatural(data.tolerance, 4)} Å'
              '${data.toleranceStated ? '' : ' (default)'}',
              style: captionStyle?.copyWith(color: color),
            ),
            const SizedBox(height: 6),
            // The tool types come **above** the operations, because they are
            // what a design has to act on: the frame tags listed here are the
            // tags a tool molecule must carry before any `tip` operation in
            // this library can be performed at all.
            if (data.tools.isNotEmpty) ...[
              Text('Tool types',
                  key: const Key('ops_library_tools_heading'),
                  style: captionStyle?.copyWith(color: color)),
              const SizedBox(height: 3),
              for (final tool in data.tools) _buildToolTypeRow(context, tool),
              const SizedBox(height: 8),
              Text('Operations', style: captionStyle?.copyWith(color: color)),
              const SizedBox(height: 3),
            ],
            for (final op in data.ops) _buildOpRow(context, op),
          ],
          for (final warning in data.warnings)
            Padding(
              padding: const EdgeInsets.only(top: 8.0),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.warning_amber_outlined,
                      size: 16.0,
                      color: Theme.of(context).colorScheme.tertiary),
                  const SizedBox(width: 6.0),
                  Expanded(
                    child: SelectableText(warning,
                        style: TextStyle(fontSize: 12.0, color: color)),
                  ),
                ],
              ),
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

  /// One tool type: its name, its state vocabulary, and the four atom tags a
  /// design must apply. The note is on a second line when the file states one —
  /// it is the only place a user reads what the instrument *is*.
  ///
  /// The first state is the one every bound tool starts in, which is why the
  /// list is printed in file order rather than sorted.
  Widget _buildToolTypeRow(BuildContext context, APIOpsLibraryToolType tool) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      key: Key('ops_library_tool_${tool.name}'),
      padding: const EdgeInsets.only(bottom: 4.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: SelectableText(
                  tool.name,
                  style: TextStyle(fontSize: 12.0, color: scheme.onSurface),
                  maxLines: 1,
                ),
              ),
              if (tool.states.isNotEmpty)
                Text(
                  tool.states.join(' / '),
                  style:
                      TextStyle(fontSize: 11.0, color: scheme.onSurfaceVariant),
                ),
            ],
          ),
          if (tool.note.isNotEmpty)
            SelectableText(
              tool.note,
              style: TextStyle(
                fontSize: 11.0,
                color: scheme.onSurfaceVariant,
                fontStyle: FontStyle.italic,
              ),
            ),
          if (tool.frameTags.isNotEmpty)
            SelectableText(
              'tags: ${tool.name}, ${tool.frameTags.join(', ')}',
              style: TextStyle(fontSize: 11.0, color: scheme.onSurfaceVariant),
            ),
        ],
      ),
    );
  }

  Widget _buildOpRow(BuildContext context, APIOpsLibraryEntry op) {
    final scheme = Theme.of(context).colorScheme;
    // The method is the operation's own fact, so it is stated on the row rather
    // than inferred from the tool section; the detail beside it is the
    // instrument for `tip` and the agent for `bulk`.
    final detail = methodDetail(toolType: op.toolType, agent: op.agent);
    return Padding(
      padding: const EdgeInsets.only(bottom: 2.0),
      child: Row(
        children: [
          Expanded(
            child: SelectableText(
              op.name,
              style: TextStyle(fontSize: 12.0, color: scheme.onSurface),
              maxLines: 1,
            ),
          ),
          if (op.method.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(right: 6.0),
              child: MechanosynthMethodBadge(method: op.method, detail: detail),
            ),
          if (op.chiral)
            Padding(
              padding: const EdgeInsets.only(right: 6.0),
              child: Text('chiral',
                  style: TextStyle(
                      fontSize: 11.0, color: scheme.onSurfaceVariant)),
            ),
          Text(
            '${op.beforeAtoms} → ${op.afterAtoms}',
            style: TextStyle(fontSize: 11.0, color: scheme.onSurfaceVariant),
          ),
        ],
      ),
    );
  }
}

/// Editor for `build_script` — the path, and how many steps the file holds.
class BuildScriptEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIBuildScriptData? data;
  final bool fileConnected;
  final StructureDesignerModel model;

  const BuildScriptEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.fileConnected,
    required this.model,
  });

  @override
  State<BuildScriptEditor> createState() => _BuildScriptEditorState();
}

class _BuildScriptEditorState extends State<BuildScriptEditor> {
  String? _errorMessage;

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final captionStyle = Theme.of(context).textTheme.bodySmall;
    final color = Theme.of(context).colorScheme.onSurfaceVariant;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Build Script',
            nodeTypeName: 'build_script',
          ),
          const SizedBox(height: 16),
          _FileRow(
            label: 'File',
            value: data.file,
            connected: widget.fileConnected,
            dialogTitle: 'Select build script',
            onChanged: (name) {
              setState(() => _errorMessage = null);
              widget.model.setBuildScriptFile(widget.nodeId, name);
            },
            onReload: () {
              setState(() => _errorMessage = null);
              widget.model
                  .setBuildScriptFile(widget.nodeId, data.file, reload: true);
            },
            onError: (message) => setState(() => _errorMessage = message),
          ),
          const SizedBox(height: 12),
          Text(
            data.stepCount == 0
                ? 'No build script loaded.'
                : '${data.stepCount} steps',
            style: captionStyle?.copyWith(color: color),
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

/// Editor for `export_build_script` — one path field.
///
/// Like `export_atoms` the node writes only under **Execute**, and the panel
/// says so: without that line a user has every reason to expect the file to
/// appear when the path is typed.
class ExportBuildScriptEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIExportBuildScriptData? data;
  final bool fileNameConnected;
  final StructureDesignerModel model;

  const ExportBuildScriptEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.fileNameConnected,
    required this.model,
  });

  @override
  State<ExportBuildScriptEditor> createState() =>
      _ExportBuildScriptEditorState();
}

class _ExportBuildScriptEditorState extends State<ExportBuildScriptEditor> {
  String? _errorMessage;

  Future<void> _browse() async {
    try {
      String? outputFile = await FilePicker.platform.saveFile(
        dialogTitle: 'Save build script',
        fileName: 'build',
        type: FileType.custom,
        allowedExtensions: ['json'],
        initialDirectory:
            initialDirectoryFor(APIFileDialogPurpose.structureExport),
      );
      if (outputFile == null) return;
      // Windows historically doesn't append the extension; do it manually.
      if (!outputFile.toLowerCase().endsWith('.json')) {
        outputFile = '$outputFile.json';
      }
      rememberPickedFile(APIFileDialogPurpose.structureExport, outputFile);
      setState(() => _errorMessage = null);
      widget.model.setExportBuildScriptFileName(widget.nodeId, outputFile);
    } catch (e) {
      setState(() => _errorMessage = 'Error browsing file: $e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final captionStyle = Theme.of(context).textTheme.bodySmall;
    final color = Theme.of(context).colorScheme.onSurfaceVariant;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Export Build Script',
            nodeTypeName: 'export_build_script',
          ),
          const SizedBox(height: 16),
          Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Expanded(
                child: StringInput(
                  label: 'File Path',
                  value: data.fileName,
                  onChanged: (text) => widget.model
                      .setExportBuildScriptFileName(widget.nodeId, text),
                ),
              ),
              const SizedBox(width: 8),
              IconButton(
                onPressed: _browse,
                icon: const Icon(Icons.save_as),
                tooltip: 'Browse',
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text(
            widget.fileNameConnected
                ? 'The wired pin supplies the file name; the file is written '
                    'when the node is Executed.'
                : 'The file is written when the node is Executed.',
            style: captionStyle?.copyWith(color: color),
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
