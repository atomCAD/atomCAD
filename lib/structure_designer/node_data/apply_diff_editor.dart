import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for apply_diff nodes
class ApplyDiffEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIApplyDiffData? data;
  final StructureDesignerModel model;

  const ApplyDiffEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ApplyDiffEditor> createState() => ApplyDiffEditorState();
}

class ApplyDiffEditorState extends State<ApplyDiffEditor> {
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
            title: 'Apply Diff Properties',
            nodeTypeName: 'apply_diff',
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Tolerance',
            value: widget.data!.tolerance,
            onChanged: (newValue) {
              widget.model.setApplyDiffData(
                widget.nodeId,
                APIApplyDiffData(
                  tolerance: newValue,
                  errorOnStale: widget.data!.errorOnStale,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          CheckboxListTile(
            title: const Text('Error on stale'),
            value: widget.data!.errorOnStale,
            onChanged: (newValue) {
              if (newValue != null) {
                widget.model.setApplyDiffData(
                  widget.nodeId,
                  APIApplyDiffData(
                    tolerance: widget.data!.tolerance,
                    errorOnStale: newValue,
                  ),
                );
              }
            },
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
          ),
        ],
      ),
    );
  }
}
