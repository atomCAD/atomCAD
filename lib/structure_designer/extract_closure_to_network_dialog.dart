import 'package:flutter/material.dart';
import '../common/draggable_dialog.dart';
import 'identifier_validation.dart';
import 'structure_designer_model.dart';

/// Dialog for extracting a `closure` node into a new named custom network
/// (*Closure → Network*). Only the network name is collected — parameter and
/// capture parameter-node names are auto-derived by the Rust side. Modeled on
/// [FactorIntoSubnetworkDialog]'s name field (name only, no param rows). See
/// `doc/design_closure_network_conversion.md`.
class ExtractClosureToNetworkDialog extends StatefulWidget {
  final StructureDesignerModel model;
  final BigInt nodeId;
  final List<BigInt> scopeChain;

  const ExtractClosureToNetworkDialog({
    super.key,
    required this.model,
    required this.nodeId,
    required this.scopeChain,
  });

  @override
  State<ExtractClosureToNetworkDialog> createState() =>
      _ExtractClosureToNetworkDialogState();
}

class _ExtractClosureToNetworkDialogState
    extends State<ExtractClosureToNetworkDialog> {
  final TextEditingController _nameController = TextEditingController();
  String? _error;
  bool _isSubmitting = false;

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  void _submit() {
    final name = _nameController.text.trim();

    // Network names follow the relaxed user-name rules (see
    // `doc/design_relaxed_node_names.md`).
    final nameError = validateUserName(name);
    if (nameError != null) {
      setState(() => _error = nameError);
      return;
    }

    setState(() {
      _error = null;
      _isSubmitting = true;
    });

    final result = widget.model.extractClosureToNetwork(
      widget.nodeId,
      name,
      scopeChain: widget.scopeChain,
    );

    if (result.success) {
      if (mounted) {
        Navigator.of(context).pop(true);
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Created network "$name"'),
            backgroundColor: Colors.green,
          ),
        );
      }
    } else {
      setState(() {
        _error = result.error ?? 'Unknown error';
        _isSubmitting = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return DraggableDialog(
      width: 450,
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Extract to Network',
              style: Theme.of(context).textTheme.headlineSmall,
            ),
            const SizedBox(height: 24),
            TextField(
              controller: _nameController,
              decoration: const InputDecoration(
                labelText: 'Network name',
                border: OutlineInputBorder(),
                helperText: 'Name for the new custom node type',
              ),
              autofocus: true,
              enabled: !_isSubmitting,
              onSubmitted: (_) => _submit(),
            ),
            const SizedBox(height: 20),
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

/// Shows the extract-closure-to-network dialog for the `closure` node
/// [nodeId] in scope [scopeChain].
Future<void> showExtractClosureToNetworkDialog(
  BuildContext context,
  StructureDesignerModel model,
  BigInt nodeId,
  List<BigInt> scopeChain,
) async {
  await showDialog<bool>(
    context: context,
    barrierDismissible: false,
    builder: (context) => ExtractClosureToNetworkDialog(
      model: model,
      nodeId: nodeId,
      scopeChain: scopeChain,
    ),
  );
}
