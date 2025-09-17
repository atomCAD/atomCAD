import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for export_xyz nodes
class ExportXyzEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIExportXYZData? data;
  final StructureDesignerModel model;

  const ExportXyzEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ExportXyzEditor> createState() => _ExportXyzEditorState();
}

class _ExportXyzEditorState extends State<ExportXyzEditor> {
  void _updateFileName(String fileName) {
    widget.model.setExportXyzData(
      widget.nodeId,
      APIExportXYZData(fileName: fileName),
    );
  }

  Future<void> _browseFile() async {
    try {
      String? outputFile = await FilePicker.platform.saveFile(
        dialogTitle: 'Save XYZ file',
        fileName: 'structure.xyz',
        type: FileType.custom,
        allowedExtensions: ['xyz'],
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
          Text('Export XYZ File',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 16),
          
          // File path input
          SizedBox(
            width: double.infinity,
            child: StringInput(
              label: 'XYZ File Path',
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
                    'The XYZ file will be exported when the node network is evaluated. Specify a file path above to set the export destination.',
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
