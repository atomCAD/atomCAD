import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/inputs/int_input.dart';

/// Editor widget for polygon nodes
class RegPolyEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIRegPolyData? data;
  final StructureDesignerModel model;

  const RegPolyEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<RegPolyEditor> createState() => RegPolyEditorState();
}

class RegPolyEditorState extends State<RegPolyEditor> {
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
            title: 'Polygon Properties',
            nodeTypeName: 'reg_poly',
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Number of Sides',
            value: widget.data!.numSides,
            onChanged: (newValue) {
              // Ensure at least 3 sides for a valid polygon
              final validSides = newValue < 3 ? 3 : newValue;
              widget.model.setRegPolyData(
                widget.nodeId,
                APIRegPolyData(
                  numSides: validSides,
                  radius: widget.data!.radius,
                ),
              );
            },
            minimumValue: 3,
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Radius',
            value: widget.data!.radius,
            onChanged: (newValue) {
              // Ensure radius is at least 1
              final validRadius = newValue < 1 ? 1 : newValue;
              widget.model.setRegPolyData(
                widget.nodeId,
                APIRegPolyData(
                  numSides: widget.data!.numSides,
                  radius: validRadius,
                ),
              );
            },
            minimumValue: 1,
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}
