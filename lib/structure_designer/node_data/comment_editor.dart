import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

class CommentEditor extends StatefulWidget {
  final BigInt nodeId;
  final APICommentData? data;
  final StructureDesignerModel model;

  const CommentEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<CommentEditor> createState() => _CommentEditorState();
}

class _CommentEditorState extends State<CommentEditor> {
  late TextEditingController _labelController;
  late TextEditingController _textController;

  @override
  void initState() {
    super.initState();
    _labelController = TextEditingController(text: widget.data?.label ?? '');
    _textController = TextEditingController(text: widget.data?.text ?? '');
  }

  @override
  void didUpdateWidget(CommentEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.nodeId != widget.nodeId) {
      _labelController.text = widget.data?.label ?? '';
      _textController.text = widget.data?.text ?? '';
    }
  }

  void _updateComment() {
    sd_api.updateCommentNode(
      nodeId: widget.nodeId,
      label: _labelController.text,
      text: _textController.text,
    );
    widget.model.refreshFromKernel();
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
            title: 'Comment Properties',
            nodeTypeName: 'Comment',
          ),
          const SizedBox(height: 16),
          const Text(
            'Label',
            style: TextStyle(fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 4),
          TextField(
            controller: _labelController,
            decoration: const InputDecoration(
              hintText: 'Optional title...',
              isDense: true,
              border: OutlineInputBorder(),
            ),
            onChanged: (_) => _updateComment(),
          ),
          const SizedBox(height: 16),
          const Text(
            'Text',
            style: TextStyle(fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 4),
          TextField(
            controller: _textController,
            decoration: const InputDecoration(
              hintText: 'Enter comment text...',
              border: OutlineInputBorder(),
            ),
            maxLines: 8,
            onChanged: (_) => _updateComment(),
          ),
          const SizedBox(height: 16),
          Text(
            'Size: ${widget.data!.width.toStringAsFixed(0)} × ${widget.data!.height.toStringAsFixed(0)}',
            style: const TextStyle(
              color: Colors.grey,
              fontSize: 12,
            ),
          ),
        ],
      ),
    );
  }

  @override
  void dispose() {
    _labelController.dispose();
    _textController.dispose();
    super.dispose();
  }
}
