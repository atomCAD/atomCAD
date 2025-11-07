import 'package:flutter/material.dart';
import '../src/rust/api/structure_designer/import_api.dart';
import 'structure_designer_model.dart';

/// Dialog for importing node networks from a .cnnd library file
class ImportCnndLibraryDialog extends StatefulWidget {
  final String libraryFilePath;
  final StructureDesignerModel model;

  const ImportCnndLibraryDialog({
    super.key,
    required this.libraryFilePath,
    required this.model,
  });

  @override
  State<ImportCnndLibraryDialog> createState() => _ImportCnndLibraryDialogState();
}

class _ImportCnndLibraryDialogState extends State<ImportCnndLibraryDialog> {
  List<String> _availableNetworks = [];
  Set<String> _selectedNetworks = {};
  String _namePrefix = '';
  bool _isLoading = true;
  String? _errorMessage;

  @override
  void initState() {
    super.initState();
    _loadLibrary();
  }

  void _loadLibrary() {
    setState(() {
      _isLoading = true;
      _errorMessage = null;
    });

    try {
      // Load the library file
      final result = loadImportLibrary(filePath: widget.libraryFilePath);
      
      if (result.success) {
        // Get available networks
        final networks = getImportableNetworkNames();
        setState(() {
          _availableNetworks = networks;
          _isLoading = false;
        });
      } else {
        setState(() {
          _errorMessage = result.errorMessage;
          _isLoading = false;
        });
      }
    } catch (e) {
      setState(() {
        _errorMessage = 'Failed to load library: $e';
        _isLoading = false;
      });
    }
  }

  void _toggleNetworkSelection(String networkName) {
    setState(() {
      if (_selectedNetworks.contains(networkName)) {
        _selectedNetworks.remove(networkName);
      } else {
        _selectedNetworks.add(networkName);
      }
    });
  }

  void _selectAll() {
    setState(() {
      _selectedNetworks = Set.from(_availableNetworks);
    });
  }

  void _selectNone() {
    setState(() {
      _selectedNetworks.clear();
    });
  }

  void _onImport() {
    setState(() {
      _isLoading = true;
      _errorMessage = null;
    });

    try {
      // Perform the import using the model
      final result = widget.model.importFromCnndLibrary(
        widget.libraryFilePath,
        _selectedNetworks.toList(),
        _namePrefix.isEmpty ? null : _namePrefix,
      );

      if (result.success) {
        // Success - close dialog and show success message
        if (mounted) {
          Navigator.of(context).pop();
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text(
                'Successfully imported ${_selectedNetworks.length} node network(s)'
                '${_namePrefix.isNotEmpty ? ' with prefix "$_namePrefix"' : ''}',
              ),
              backgroundColor: Colors.green,
            ),
          );
        }
      } else {
        // Error - keep dialog open and show error
        setState(() {
          _errorMessage = result.errorMessage;
          _isLoading = false;
        });
      }
    } catch (e) {
      // Unexpected error - keep dialog open and show error
      setState(() {
        _errorMessage = 'Import failed: $e';
        _isLoading = false;
      });
    }
  }

  @override
  void dispose() {
    // Clear the library when dialog is closed
    clearImportLibrary();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      child: Container(
        width: 500,
        height: 600,
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Title
            Text(
              'Import from .cnnd Library',
              style: Theme.of(context).textTheme.headlineSmall,
            ),
            const SizedBox(height: 8),
            
            // Library file path
            Text(
              'Library: ${widget.libraryFilePath}',
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: Colors.grey[600],
              ),
            ),
            const SizedBox(height: 24),

            // Name prefix input
            TextField(
              decoration: const InputDecoration(
                labelText: 'Name Prefix (optional)',
                hintText: 'e.g., "physics::" or "lib_"',
                border: OutlineInputBorder(),
                helperText: 'Prefix to add to imported network names to avoid conflicts',
              ),
              onChanged: (value) {
                setState(() {
                  _namePrefix = value;
                });
              },
            ),
            const SizedBox(height: 24),

            // Networks list header
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  'Available Node Networks:',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                if (!_isLoading && _availableNetworks.isNotEmpty) ...[
                  Row(
                    children: [
                      TextButton(
                        onPressed: _selectAll,
                        child: const Text('Select All'),
                      ),
                      TextButton(
                        onPressed: _selectNone,
                        child: const Text('Select None'),
                      ),
                    ],
                  ),
                ],
              ],
            ),
            const SizedBox(height: 12),

            // Networks list
            Expanded(
              child: Container(
                decoration: BoxDecoration(
                  border: Border.all(color: Colors.grey[300]!),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: _buildNetworksList(),
              ),
            ),
            const SizedBox(height: 16),

            // Error message display
            if (_errorMessage != null && !_isLoading) ...[
              Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: Colors.red[50],
                  border: Border.all(color: Colors.red[300]!),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Row(
                  children: [
                    Icon(Icons.error_outline, color: Colors.red[600], size: 20),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        _errorMessage!,
                        style: TextStyle(color: Colors.red[700]),
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 16),
            ] else ...[
              const SizedBox(height: 8),
            ],

            // Action buttons
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 12),
                ElevatedButton(
                  onPressed: _selectedNetworks.isEmpty || _isLoading ? null : _onImport,
                  child: _isLoading 
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Text('Import'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildNetworksList() {
    if (_isLoading) {
      return const Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            CircularProgressIndicator(),
            SizedBox(height: 16),
            Text('Loading library...'),
          ],
        ),
      );
    }

    if (_errorMessage != null) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              Icons.error_outline,
              size: 48,
              color: Colors.red[400],
            ),
            const SizedBox(height: 16),
            Text(
              'Error loading library:',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            Text(
              _errorMessage!,
              textAlign: TextAlign.center,
              style: TextStyle(color: Colors.red[600]),
            ),
            const SizedBox(height: 16),
            ElevatedButton(
              onPressed: _loadLibrary,
              child: const Text('Retry'),
            ),
          ],
        ),
      );
    }

    if (_availableNetworks.isEmpty) {
      return const Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              Icons.inbox_outlined,
              size: 48,
              color: Colors.grey,
            ),
            SizedBox(height: 16),
            Text('No node networks found in this library.'),
          ],
        ),
      );
    }

    return ListView.builder(
      padding: const EdgeInsets.all(8),
      itemCount: _availableNetworks.length,
      itemBuilder: (context, index) {
        final networkName = _availableNetworks[index];
        final isSelected = _selectedNetworks.contains(networkName);
        
        return CheckboxListTile(
          title: Text(networkName),
          subtitle: _namePrefix.isNotEmpty 
              ? Text('Will be imported as: $_namePrefix$networkName')
              : null,
          value: isSelected,
          onChanged: (bool? value) {
            _toggleNetworkSelection(networkName);
          },
          controlAffinity: ListTileControlAffinity.leading,
        );
      },
    );
  }
}
