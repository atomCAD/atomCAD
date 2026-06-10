import 'package:flutter/material.dart';
import '../common/draggable_dialog.dart';
import 'identifier_validation.dart';
import 'namespace_utils.dart';
import 'structure_designer_model.dart';

/// Suggests a unique, namespace-qualified name for a closure being converted
/// into a network when the closure has no display label to derive one from.
///
/// Mirrors the factor-out-to-subnetwork suggestion: the leaf is `closureN`
/// (smallest `N ≥ 1` not already taken) and it is prefixed with the namespace
/// (folder path) of the active network, so a closure converted inside
/// `Foo.Bar.Baz` suggests `Foo.Bar.closure1`.
String suggestClosureNetworkName(StructureDesignerModel model) {
  final namespace = getNamespace(model.nodeNetworkView?.name ?? '');

  // Names already taken across networks and record defs (one namespace).
  final taken = <String>{
    ...model.nodeNetworkNames.map((n) => n.name),
    ...model.recordTypeDefNames,
  };

  var counter = 1;
  while (true) {
    final candidate = combineQualifiedName(namespace, 'closure$counter');
    if (!taken.contains(candidate)) {
      return candidate;
    }
    counter++;
  }
}

/// Dialog for extracting a `closure` node into a new named custom network
/// (*Closure → Network*). Only the network name is collected — parameter and
/// capture parameter-node names are auto-derived by the Rust side. Modeled on
/// [FactorIntoSubnetworkDialog]'s name field (name only, no param rows). See
/// `doc/design_closure_network_conversion.md`.
class ExtractClosureToNetworkDialog extends StatefulWidget {
  final StructureDesignerModel model;
  final BigInt nodeId;
  final List<BigInt> scopeChain;

  /// Name to pre-fill the network-name field with. The caller derives this
  /// from the closure's display label, re-qualified with the local namespace
  /// (so a closure that came from a `Foo.Bar` network suggests `Foo.Bar`
  /// again). Empty when the closure has no label.
  final String initialName;

  const ExtractClosureToNetworkDialog({
    super.key,
    required this.model,
    required this.nodeId,
    required this.scopeChain,
    this.initialName = '',
  });

  @override
  State<ExtractClosureToNetworkDialog> createState() =>
      _ExtractClosureToNetworkDialogState();
}

class _ExtractClosureToNetworkDialogState
    extends State<ExtractClosureToNetworkDialog> {
  late final TextEditingController _nameController =
      TextEditingController(text: widget.initialName)
        // Select the whole pre-filled name so the user can immediately type
        // over it or keep it with a single keystroke.
        ..selection = TextSelection(
          baseOffset: 0,
          extentOffset: widget.initialName.length,
        );
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
              'Convert Closure to Network',
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
  List<BigInt> scopeChain, {
  String initialName = '',
}) async {
  await showDialog<bool>(
    context: context,
    barrierDismissible: false,
    builder: (context) => ExtractClosureToNetworkDialog(
      model: model,
      nodeId: nodeId,
      scopeChain: scopeChain,
      initialName: initialName,
    ),
  );
}
