import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for import_xyz nodes
class ImportXyzEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIImportXYZData? data;
  final StructureDesignerModel model;

  const ImportXyzEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ImportXyzEditor> createState() => _ImportXyzEditorState();
}

class _ImportXyzEditorState extends State<ImportXyzEditor> {
  String? _errorMessage;
  bool _isLoading = false;

  void _updateFileName(String fileName) {
    widget.model.setImportXyzData(
      widget.nodeId,
      APIImportXYZData(fileName: fileName.isEmpty ? null : fileName),
    );
  }

  Future<void> _browseFile() async {
    try {
      FilePickerResult? result = await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['xyz'],
        dialogTitle: 'Select XYZ file',
        initialDirectory:
            initialDirectoryFor(APIFileDialogPurpose.structureImport),
      );

      if (result != null && result.files.single.path != null) {
        final filePath = result.files.single.path!;
        rememberPickedFile(APIFileDialogPurpose.structureImport, filePath);
        _updateFileName(filePath);

        // Automatically try to load the file after browsing
        await _loadFileWithPath(filePath);
      }
    } catch (e) {
      setState(() {
        _errorMessage = 'Error browsing file: $e';
      });
    }
  }

  Future<void> _loadFile() async {
    if (widget.data?.fileName == null || widget.data!.fileName!.isEmpty) {
      setState(() {
        _errorMessage = 'Please specify a file path first';
      });
      return;
    }

    await _loadFileWithPath(widget.data!.fileName!);
  }

  Future<void> _loadFileWithPath(String filePath) async {
    setState(() {
      _isLoading = true;
      _errorMessage = null;
    });

    try {
      final result = widget.model.importXyz(widget.nodeId);

      setState(() {
        _isLoading = false;
        if (!result.success) {
          _errorMessage = result.errorMessage;
        } else {
          _errorMessage = null;
        }
      });
    } catch (e) {
      setState(() {
        _isLoading = false;
        _errorMessage = 'Error loading file: $e';
      });
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
            title: 'Import XYZ File',
            nodeTypeName: 'import_xyz',
          ),
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

          // Buttons row
          Row(
            children: [
              ElevatedButton.icon(
                onPressed: _isLoading ? null : _browseFile,
                icon: const Icon(Icons.folder_open),
                label: const Text('Browse'),
                style: ElevatedButton.styleFrom(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
                ),
              ),
              const SizedBox(width: 12),
              ElevatedButton.icon(
                onPressed: _isLoading ? null : _loadFile,
                icon: _isLoading
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.upload_file),
                label: Text(_isLoading ? 'Loading...' : 'Load'),
                style: ElevatedButton.styleFrom(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
                ),
              ),
            ],
          ),

          // Error message display (selectable + copyable, issue #359).
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
