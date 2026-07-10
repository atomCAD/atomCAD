import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/inputs/string_input.dart';
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

class _ExportAtomsEditorState extends State<ExportAtomsEditor> {
  void _updateFileName(String fileName) {
    widget.model.setExportAtomsData(
      widget.nodeId,
      APIExportAtomsData(fileName: fileName),
    );
  }

  Future<void> _browseFile() async {
    try {
      String? outputFile = await FilePicker.platform.saveFile(
        dialogTitle: 'Save atoms file',
        fileName: 'structure.xyz',
        type: FileType.custom,
        allowedExtensions: ['xyz', 'mol'],
      );

      if (outputFile != null) {
        _updateFileName(outputFile);
      }
    } catch (e) {
      // Handle error silently or show a snackbar if needed
      debugPrint('Error browsing file: $e');
    }
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
                    'The file is written when the node is Executed; the format is chosen by the file extension (.xyz, .mol). '
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
