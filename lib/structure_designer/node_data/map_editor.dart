import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for map nodes
class MapEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIMapData? data;
  final StructureDesignerModel model;

  const MapEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<MapEditor> createState() => MapEditorState();
}

class MapEditorState extends State<MapEditor> {
  @override
  void dispose() {
    super.dispose();
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
            title: 'Map Properties',
            nodeTypeName: 'map',
          ),
          const SizedBox(height: 8),

          // Input Type
          DataTypeInput(
            label: 'Input Type',
            value: widget.data!.inputType,
            onChanged: (newValue) {
              widget.model.setMApData(
                widget.nodeId,
                APIMapData(
                  inputType: newValue,
                  outputType: widget.data!.outputType,
                ),
              );
            },
          ),
          const SizedBox(height: 8),

          // Output Type
          DataTypeInput(
            label: 'Output Type',
            value: widget.data!.outputType,
            onChanged: (newValue) {
              widget.model.setMApData(
                widget.nodeId,
                APIMapData(
                  inputType: widget.data!.inputType,
                  outputType: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
