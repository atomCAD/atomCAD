import 'package:flutter/material.dart';
import '../common/draggable_dialog.dart';
import '../src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import '../src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'structure_designer_model.dart';

/// Dialog for factoring the current node selection into a reusable subnetwork.
///
/// This dialog allows users to:
/// - Name the new subnetwork (custom node type)
/// - Edit the parameter names for external inputs
/// - See validation errors before submitting
class FactorIntoSubnetworkDialog extends StatefulWidget {
  final StructureDesignerModel model;
  final FactorSelectionInfo info;

  const FactorIntoSubnetworkDialog({
    super.key,
    required this.model,
    required this.info,
  });

  @override
  State<FactorIntoSubnetworkDialog> createState() =>
      _FactorIntoSubnetworkDialogState();
}

class _FactorIntoSubnetworkDialogState
    extends State<FactorIntoSubnetworkDialog> {
  late TextEditingController _nameController;
  late TextEditingController _paramsController;
  String? _error;
  bool _isSubmitting = false;

  @override
  void initState() {
    super.initState();
    _nameController = TextEditingController(text: widget.info.suggestedName);
    _paramsController = TextEditingController(
      text: widget.info.suggestedParamNames.join('\n'),
    );
  }

  @override
  void dispose() {
    _nameController.dispose();
    _paramsController.dispose();
    super.dispose();
  }

  /// Validates the subnetwork name
  String? _validateName(String name) {
    if (name.isEmpty) {
      return 'Name cannot be empty';
    }

    // Check for valid identifier (alphanumeric + underscore, not starting with digit)
    final validIdentifier = RegExp(r'^[a-zA-Z_][a-zA-Z0-9_]*$');
    if (!validIdentifier.hasMatch(name)) {
      return 'Name must be a valid identifier (letters, numbers, underscore; cannot start with digit)';
    }

    return null;
  }

  /// Validates the parameter names
  String? _validateParams(List<String> paramNames) {
    final expectedCount = widget.info.suggestedParamNames.length;

    if (paramNames.length != expectedCount) {
      return 'Expected $expectedCount parameter names, got ${paramNames.length}';
    }

    // Check each parameter name
    final validIdentifier = RegExp(r'^[a-zA-Z_][a-zA-Z0-9_]*$');
    for (int i = 0; i < paramNames.length; i++) {
      final name = paramNames[i];
      if (name.isEmpty) {
        return 'Parameter ${i + 1} name cannot be empty';
      }
      if (!validIdentifier.hasMatch(name)) {
        return 'Parameter "$name" is not a valid identifier';
      }
    }

    // Check for duplicates
    final uniqueNames = paramNames.toSet();
    if (uniqueNames.length != paramNames.length) {
      return 'Parameter names must be unique';
    }

    return null;
  }

  void _submit() async {
    final name = _nameController.text.trim();
    final paramNames = _paramsController.text
        .split('\n')
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList();

    // Validate name
    final nameError = _validateName(name);
    if (nameError != null) {
      setState(() => _error = nameError);
      return;
    }

    // Validate params
    final paramsError = _validateParams(paramNames);
    if (paramsError != null) {
      setState(() => _error = paramsError);
      return;
    }

    // Clear previous error and show loading
    setState(() {
      _error = null;
      _isSubmitting = true;
    });

    try {
      // Call the API
      final result = structure_designer_api.factorSelectionIntoSubnetwork(
        request: FactorSelectionRequest(
          subnetworkName: name,
          paramNames: paramNames,
        ),
      );

      if (result.success) {
        // Refresh the model to update the UI
        widget.model.refreshFromKernel();

        if (mounted) {
          Navigator.of(context).pop(true);
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text('Created subnetwork "$name"'),
              backgroundColor: Colors.green,
            ),
          );
        }
      } else {
        // Show error in dialog
        setState(() {
          _error = result.error ?? 'Unknown error';
          _isSubmitting = false;
        });
      }
    } catch (e) {
      setState(() {
        _error = 'Failed to create subnetwork: $e';
        _isSubmitting = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final hasParams = widget.info.suggestedParamNames.isNotEmpty;

    return DraggableDialog(
      width: 450,
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Title
            Text(
              'Factor out to Subnetwork',
              style: Theme.of(context).textTheme.headlineSmall,
            ),
            const SizedBox(height: 24),

            // Subnetwork name field
            TextField(
              controller: _nameController,
              decoration: const InputDecoration(
                labelText: 'Subnetwork name',
                border: OutlineInputBorder(),
                helperText: 'Name for the new custom node type',
              ),
              autofocus: true,
              enabled: !_isSubmitting,
              onSubmitted: (_) {
                if (!hasParams) {
                  _submit();
                }
              },
            ),
            const SizedBox(height: 20),

            // Parameter names field (only if there are parameters)
            if (hasParams) ...[
              Text(
                'Parameter names (one per line):',
                style: Theme.of(context).textTheme.titleSmall,
              ),
              const SizedBox(height: 8),
              Container(
                height: 120,
                decoration: BoxDecoration(
                  border: Border.all(color: Colors.grey[400]!),
                  borderRadius: BorderRadius.circular(4),
                ),
                child: TextField(
                  controller: _paramsController,
                  maxLines: null,
                  expands: true,
                  decoration: const InputDecoration(
                    border: InputBorder.none,
                    contentPadding: EdgeInsets.all(12),
                  ),
                  style: const TextStyle(fontFamily: 'monospace'),
                  enabled: !_isSubmitting,
                ),
              ),
              const SizedBox(height: 8),
              Text(
                '${widget.info.suggestedParamNames.length} parameter(s) required',
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                      color: Colors.grey[600],
                    ),
              ),
              const SizedBox(height: 16),
            ],

            // Error message
            if (_error != null) ...[
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
                        _error!,
                        style: TextStyle(color: Colors.red[700]),
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 16),
            ],

            // Action buttons
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed:
                      _isSubmitting ? null : () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 12),
                ElevatedButton(
                  onPressed: _isSubmitting ? null : _submit,
                  child: _isSubmitting
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Text('Create'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// Shows the factor into subnetwork dialog if the current selection is valid.
///
/// Returns true if the selection was successfully factored, false otherwise.
Future<bool> showFactorIntoSubnetworkDialog(
  BuildContext context,
  StructureDesignerModel model,
) async {
  // Get selection info
  final info = structure_designer_api.getFactorSelectionInfo();

  if (!info.canFactor) {
    // Show a snackbar with the reason
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(info.invalidReason ?? 'Cannot factor selection'),
        backgroundColor: Colors.orange,
      ),
    );
    return false;
  }

  // Show the dialog
  final result = await showDialog<bool>(
    context: context,
    builder: (context) => FactorIntoSubnetworkDialog(
      model: model,
      info: info,
    ),
  );

  return result ?? false;
}
