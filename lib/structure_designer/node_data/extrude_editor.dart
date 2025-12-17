import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for extrude nodes
class ExtrudeEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIExtrudeData? data;
  final StructureDesignerModel model;

  const ExtrudeEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ExtrudeEditor> createState() => ExtrudeEditorState();
}

class ExtrudeEditorState extends State<ExtrudeEditor> {
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
            title: 'Extrude Properties',
            nodeTypeName: 'extrude',
          ),
          const SizedBox(height: 8),
          CheckboxListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Infinite'),
            value: widget.data!.infinite,
            onChanged: (newValue) {
              if (newValue == null) return;
              widget.model.setExtrudeData(
                widget.nodeId,
                APIExtrudeData(
                  height: widget.data!.height,
                  extrudeDirection: widget.data!.extrudeDirection,
                  infinite: newValue,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Height',
            value: widget.data!.height,
            minimumValue: 1,
            onChanged: (newValue) {
              widget.model.setExtrudeData(
                widget.nodeId,
                APIExtrudeData(
                  height: newValue,
                  extrudeDirection: widget.data!.extrudeDirection,
                  infinite: widget.data!.infinite,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IVec3Input(
            label: 'Extrude Direction',
            value: widget.data!.extrudeDirection,
            onChanged: (newValue) {
              widget.model.setExtrudeData(
                widget.nodeId,
                APIExtrudeData(
                  height: widget.data!.height,
                  extrudeDirection: newValue,
                  infinite: widget.data!.infinite,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
