import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for import_cif nodes
class ImportCifEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIImportCIFData? data;
  final StructureDesignerModel model;

  const ImportCifEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ImportCifEditor> createState() => _ImportCifEditorState();
}

class _ImportCifEditorState extends State<ImportCifEditor> {
  String? _errorMessage;
  bool _isLoading = false;

  APIImportCIFData _currentData() {
    return widget.data ??
        const APIImportCIFData(
          useCifBonds: true,
          inferBonds: true,
          bondTolerance: 1.15,
        );
  }

  void _updateData(APIImportCIFData data) {
    widget.model.setImportCifData(widget.nodeId, data);
  }

  void _updateFileName(String fileName) {
    final data = _currentData();
    _updateData(APIImportCIFData(
      fileName: fileName.isEmpty ? null : fileName,
      blockName: data.blockName,
      useCifBonds: data.useCifBonds,
      inferBonds: data.inferBonds,
      bondTolerance: data.bondTolerance,
    ));
  }

  void _updateBlockName(String blockName) {
    final data = _currentData();
    _updateData(APIImportCIFData(
      fileName: data.fileName,
      blockName: blockName.isEmpty ? null : blockName,
      useCifBonds: data.useCifBonds,
      inferBonds: data.inferBonds,
      bondTolerance: data.bondTolerance,
    ));
  }

  void _updateUseCifBonds(bool value) {
    final data = _currentData();
    _updateData(APIImportCIFData(
      fileName: data.fileName,
      blockName: data.blockName,
      useCifBonds: value,
      inferBonds: data.inferBonds,
      bondTolerance: data.bondTolerance,
    ));
  }

  void _updateInferBonds(bool value) {
    final data = _currentData();
    _updateData(APIImportCIFData(
      fileName: data.fileName,
      blockName: data.blockName,
      useCifBonds: data.useCifBonds,
      inferBonds: value,
      bondTolerance: data.bondTolerance,
    ));
  }

  void _updateBondTolerance(String value) {
    final tolerance = double.tryParse(value);
    if (tolerance == null || tolerance <= 0) return;
    final data = _currentData();
    _updateData(APIImportCIFData(
      fileName: data.fileName,
      blockName: data.blockName,
      useCifBonds: data.useCifBonds,
      inferBonds: data.inferBonds,
      bondTolerance: tolerance,
    ));
  }

  Future<void> _browseFile() async {
    try {
      FilePickerResult? result = await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['cif'],
        dialogTitle: 'Select CIF file',
      );

      if (result != null && result.files.single.path != null) {
        final filePath = result.files.single.path!;
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
      final result = widget.model.importCif(widget.nodeId);

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

    final data = _currentData();

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Import CIF File',
            nodeTypeName: 'import_cif',
          ),
          const SizedBox(height: 16),

          // File path input
          SizedBox(
            width: double.infinity,
            child: StringInput(
              label: 'CIF File Path',
              value: data.fileName ?? '',
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
          const SizedBox(height: 16),

          // Block name input
          SizedBox(
            width: double.infinity,
            child: StringInput(
              label: 'Block Name (empty = first block)',
              value: data.blockName ?? '',
              onChanged: _updateBlockName,
            ),
          ),
          const SizedBox(height: 12),

          // Bond options
          CheckboxListTile(
            title: const Text('Use CIF Bonds'),
            subtitle: const Text('Prefer explicit bonds from CIF file'),
            value: data.useCifBonds,
            onChanged: (value) {
              if (value != null) _updateUseCifBonds(value);
            },
            dense: true,
            contentPadding: EdgeInsets.zero,
          ),
          CheckboxListTile(
            title: const Text('Infer Bonds'),
            subtitle: const Text('Distance-based bond inference'),
            value: data.inferBonds,
            onChanged: (value) {
              if (value != null) _updateInferBonds(value);
            },
            dense: true,
            contentPadding: EdgeInsets.zero,
          ),
          const SizedBox(height: 8),

          // Bond tolerance
          IgnorePointer(
            ignoring: !data.inferBonds,
            child: Opacity(
              opacity: data.inferBonds ? 1.0 : 0.5,
              child: SizedBox(
                width: double.infinity,
                child: StringInput(
                  label: 'Bond Tolerance',
                  value: data.bondTolerance.toString(),
                  onChanged: _updateBondTolerance,
                ),
              ),
            ),
          ),

          // Error message display
          if (_errorMessage != null)
            Padding(
              padding: const EdgeInsets.only(top: 16.0),
              child: Container(
                width: double.infinity,
                padding: const EdgeInsets.all(12.0),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.errorContainer,
                  borderRadius: BorderRadius.circular(8.0),
                  border: Border.all(
                    color: Theme.of(context).colorScheme.error,
                    width: 1.0,
                  ),
                ),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Icon(
                      Icons.error_outline,
                      color: Theme.of(context).colorScheme.error,
                      size: 20.0,
                    ),
                    const SizedBox(width: 8.0),
                    Expanded(
                      child: Text(
                        _errorMessage!,
                        style: TextStyle(
                          color: Theme.of(context).colorScheme.error,
                          fontSize: 14.0,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
        ],
      ),
    );
  }
}
