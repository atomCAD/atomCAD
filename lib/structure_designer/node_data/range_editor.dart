import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for range nodes
class RangeEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIRangeData? data;
  final StructureDesignerModel model;

  const RangeEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<RangeEditor> createState() => RangeEditorState();
}

class RangeEditorState extends State<RangeEditor> {
  // Direct API calls are made in onChanged handlers

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
            title: 'Range Properties',
            nodeTypeName: 'range',
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Start',
            value: widget.data!.start,
            onChanged: (newValue) {
              widget.model.setRangeData(
                widget.nodeId,
                APIRangeData(
                  start: newValue,
                  step: widget.data!.step,
                  count: widget.data!.count,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Step',
            value: widget.data!.step,
            onChanged: (newValue) {
              widget.model.setRangeData(
                widget.nodeId,
                APIRangeData(
                  start: widget.data!.start,
                  step: newValue,
                  count: widget.data!.count,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Count',
            value: widget.data!.count,
            onChanged: (newValue) {
              widget.model.setRangeData(
                widget.nodeId,
                APIRangeData(
                  start: widget.data!.start,
                  step: widget.data!.step,
                  count: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
