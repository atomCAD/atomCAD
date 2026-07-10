import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/common/export_format_dialog.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for export_atoms nodes
class ExportAtomsEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIExportAtomsData? data;
  final StructureDesignerModel model;

  const ExportAtomsEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ExportAtomsEditor> createState() => _ExportAtomsEditorState();
}

/// Input pin index of the `file_name` pin on the export_atoms node
/// (molecule = 0, file_name = 1, metadata = 2). Used to detect when the file
/// name is supplied by a wire rather than the stored property.
const int _FILE_NAME_PIN_INDEX = 1;

class _ExportAtomsEditorState extends State<ExportAtomsEditor> {
  /// The supported export formats, fetched once from the Rust single source of
  /// truth (`AtomExportFormat::ALL`). Everything the panel says about formats —
  /// the indicator, the info card, the Browse chooser — is derived from this,
  /// so a new format added in Rust surfaces here with no edits.
  late final List<APIAtomExportFormat> _formats;

  @override
  void initState() {
    super.initState();
    _formats = structure_designer_api.getAtomExportFormats();
  }

  /// Human-readable, comma-separated list of supported extensions (".xyz, .mol").
  String get _supportedExtensionsDisplay =>
      _formats.map((f) => '.${f.extension_}').join(', ');

  void _updateFileName(String fileName) {
    widget.model.setExportAtomsData(
      widget.nodeId,
      APIExportAtomsData(fileName: fileName),
    );
  }

  /// True when the `file_name` input pin is wired — the extension (and hence
  /// the format) is then decided at Execute time from the wired value, so the
  /// stored file name and the reactive indicator no longer apply.
  bool _isFileNamePinConnected() {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId &&
          wire.destParamIndex == BigInt.from(_FILE_NAME_PIN_INDEX)) {
        return true;
      }
    }
    return false;
  }

  /// The format whose extension the given file name ends with (case-insensitive),
  /// or `null` for a missing/unrecognized extension. Mirrors
  /// `AtomExportFormat::from_path`.
  APIAtomExportFormat? _formatForFileName(String fileName) {
    final lower = fileName.toLowerCase();
    for (final format in _formats) {
      if (lower.endsWith('.${format.extension_}')) {
        return format;
      }
    }
    return null;
  }

  Future<void> _browseFile() async {
    try {
      // Same format chooser the File → Export visible menu uses. The OS save
      // dialog can't carry the choice (its filter collapses extensions), so we
      // pick the format first, then save with a single extension.
      final String? extension = await showAtomExportFormatDialog(context);
      if (extension == null) return;

      // Extension-less default name (macOS appends the allowed extension; a
      // default already carrying it misbehaves — see commit b63c8a32).
      String? outputFile = await FilePicker.platform.saveFile(
        dialogTitle: 'Save atoms file',
        fileName: 'structure',
        type: FileType.custom,
        allowedExtensions: [extension],
      );

      if (outputFile != null) {
        // Windows historically doesn't append the extension; do it manually.
        // Use endsWith (not contains('.')) so a directory with a dot in its
        // name doesn't defeat the check.
        if (!outputFile.toLowerCase().endsWith('.$extension')) {
          outputFile = '$outputFile.$extension';
        }
        _updateFileName(outputFile);
      }
    } catch (e) {
      // Handle error silently or show a snackbar if needed
      debugPrint('Error browsing file: $e');
    }
  }

  /// A row under the path field that reports the format the current file name
  /// resolves to, turning the Execute-deferred format check into while-typing
  /// feedback. Four states: wired pin (neutral), empty (neutral hint),
  /// recognized extension (format label), unrecognized extension (error).
  Widget _buildFormatIndicator(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;

    IconData icon;
    Color color;
    String text;

    if (_isFileNamePinConnected()) {
      icon = Icons.link;
      color = colorScheme.onSurfaceVariant;
      text = "Format is chosen by the wired file name's extension at Execute.";
    } else {
      final fileName = widget.data?.fileName ?? '';
      if (fileName.isEmpty) {
        icon = Icons.help_outline;
        color = colorScheme.onSurfaceVariant;
        text = 'Choose a file name (supported: $_supportedExtensionsDisplay).';
      } else {
        final format = _formatForFileName(fileName);
        if (format != null) {
          icon = Icons.check_circle_outline;
          color = colorScheme.onSurface;
          text = 'Format: ${format.label}';
        } else {
          icon = Icons.error_outline;
          color = colorScheme.error;
          text = 'Unrecognized extension — supported: '
              '$_supportedExtensionsDisplay';
        }
      }
    }

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(icon, color: color, size: 18.0),
        const SizedBox(width: 6.0),
        Expanded(
          child: Text(
            text,
            style: TextStyle(color: color, fontSize: 13.0),
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Export Atoms',
            nodeTypeName: 'export_atoms',
          ),
          const SizedBox(height: 16),

          // File path input
          SizedBox(
            width: double.infinity,
            child: StringInput(
              label: 'File Path',
              value: widget.data?.fileName ?? '',
              onChanged: _updateFileName,
            ),
          ),
          const SizedBox(height: 8),

          // Reactive format indicator, derived from the file name's extension.
          _buildFormatIndicator(context),
          const SizedBox(height: 12),

          // Browse button
          ElevatedButton.icon(
            onPressed: _browseFile,
            icon: const Icon(Icons.save_as),
            label: const Text('Browse'),
            style: ElevatedButton.styleFrom(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
            ),
          ),

          const SizedBox(height: 16),

          // Info text
          Container(
            width: double.infinity,
            padding: const EdgeInsets.all(12.0),
            decoration: BoxDecoration(
              color: Theme.of(context).colorScheme.surfaceContainerHighest,
              borderRadius: BorderRadius.circular(8.0),
              border: Border.all(
                color: Theme.of(context).colorScheme.outline,
                width: 1.0,
              ),
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  Icons.info_outline,
                  color: Theme.of(context).colorScheme.primary,
                  size: 20.0,
                ),
                const SizedBox(width: 8.0),
                Expanded(
                  child: Text(
                    'The file is written when the node is Executed; the format is chosen by the file extension ($_supportedExtensionsDisplay). '
                    'Wire a record into the optional "metadata" pin to also write a "<file>.params.json" sidecar containing those generation parameters plus a BLAKE3 hash of the exported file.',
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.onSurface,
                      fontSize: 14.0,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
