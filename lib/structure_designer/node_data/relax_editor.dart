import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/relax_api.dart';

/// Editor widget for relax nodes - displays energy minimization results
class RelaxEditor extends StatefulWidget {
  final BigInt nodeId;
  final StructureDesignerModel model;

  const RelaxEditor({
    super.key,
    required this.nodeId,
    required this.model,
  });

  @override
  State<RelaxEditor> createState() => _RelaxEditorState();
}

class _RelaxEditorState extends State<RelaxEditor> {
  String _relaxMessage = '';

  @override
  void initState() {
    super.initState();
    _updateRelaxMessage();

    // Listen to model changes to update the message
    widget.model.addListener(_updateRelaxMessage);
  }

  @override
  void dispose() {
    widget.model.removeListener(_updateRelaxMessage);
    super.dispose();
  }

  void _updateRelaxMessage() {
    final message = getRelaxMessage();
    if (mounted) {
      setState(() {
        _relaxMessage = message;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Energy Minimization',
            nodeTypeName: 'relax',
          ),
          const SizedBox(height: 16),
          Card(
            elevation: 1,
            child: Padding(
              padding: const EdgeInsets.all(12.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('Minimization Result',
                      style: Theme.of(context).textTheme.titleSmall),
                  const SizedBox(height: 8),
                  Container(
                    width: double.infinity,
                    padding: const EdgeInsets.all(8.0),
                    decoration: BoxDecoration(
                      color: Theme.of(context).colorScheme.surfaceContainerHighest,
                      borderRadius: BorderRadius.circular(4.0),
                    ),
                    child: Text(
                      _relaxMessage.isEmpty
                          ? 'No energy minimization result available'
                          : _relaxMessage,
                      style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                            fontFamily: 'monospace',
                          ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}
