import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for motif nodes
class MotifEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIMotifData? data;
  final StructureDesignerModel model;

  const MotifEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<MotifEditor> createState() => _MotifEditorState();
}

class _MotifEditorState extends State<MotifEditor> {
  late TextEditingController _definitionController;
  late TextEditingController _nameController;
  late FocusNode _definitionFocusNode;
  late FocusNode _nameFocusNode;

  @override
  void initState() {
    super.initState();
    _definitionController = TextEditingController(text: widget.data?.definition ?? '');
    _nameController = TextEditingController(text: widget.data?.name ?? '');
    _definitionFocusNode = FocusNode();
    _nameFocusNode = FocusNode();
  }

  @override
  void didUpdateWidget(MotifEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data?.definition != widget.data?.definition) {
      _definitionController.text = widget.data?.definition ?? '';
    }
    if (oldWidget.data?.name != widget.data?.name) {
      _nameController.text = widget.data?.name ?? '';
    }
  }

  @override
  void dispose() {
    _definitionController.dispose();
    _nameController.dispose();
    _definitionFocusNode.dispose();
    _nameFocusNode.dispose();
    super.dispose();
  }

  void _applyChanges() {
    final name = _nameController.text.trim();
    widget.model.setMotifData(
      widget.nodeId,
      APIMotifData(
        definition: _definitionController.text,
        name: name.isEmpty ? null : name,
        error: null, // This will be set by the backend after parsing
      ),
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
          Text('Motif Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          
          // Name input
          StringInput(
            label: 'Name (optional)',
            value: _nameController.text,
            onChanged: (value) {
              _nameController.text = value;
            },
          ),
          const SizedBox(height: 8),
          
          // Definition text area
          TextFormField(
            controller: _definitionController,
            focusNode: _definitionFocusNode,
            decoration: const InputDecoration(
              labelText: 'Motif Definition',
              border: OutlineInputBorder(),
              contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              hintText: 'Enter motif definition...',
            ),
            maxLines: 10,
            minLines: 5,
            keyboardType: TextInputType.multiline,
            textInputAction: TextInputAction.newline,
          ),
          
          const SizedBox(height: 8),
          
          // Apply button
          SizedBox(
            width: double.infinity,
            child: ElevatedButton(
              onPressed: _applyChanges,
              child: const Text('Apply'),
            ),
          ),
          
          // Error message display
          if (widget.data?.error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8.0),
              child: Container(
                width: double.infinity,
                padding: const EdgeInsets.all(8.0),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.errorContainer,
                  borderRadius: BorderRadius.circular(4.0),
                  border: Border.all(
                    color: Theme.of(context).colorScheme.error,
                    width: 1.0,
                  ),
                ),
                child: Text(
                  widget.data!.error!,
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.onErrorContainer,
                    fontSize: 12.0,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
