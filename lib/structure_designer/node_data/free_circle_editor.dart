import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/vec2_input.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for free_circle nodes.
/// Center and radius are authored directly in real-space (Å) coordinates
/// within the drawing-plane frame — the non-lattice-aligned analog of the
/// `circle` node.
class FreeCircleEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIFreeCircleData? data;
  final StructureDesignerModel model;

  const FreeCircleEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<FreeCircleEditor> createState() => FreeCircleEditorState();
}

class FreeCircleEditorState extends State<FreeCircleEditor> {
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
            title: 'Free Circle Properties',
            nodeTypeName: 'free_circle',
          ),
          const SizedBox(height: 8),
          Vec2Input(
            label: 'Center (Å)',
            value: widget.data!.center,
            onChanged: (newValue) {
              widget.model.setFreeCircleData(
                widget.nodeId,
                APIFreeCircleData(
                  center: newValue,
                  radius: widget.data!.radius,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Radius (Å)',
            value: widget.data!.radius,
            onChanged: (newValue) {
              widget.model.setFreeCircleData(
                widget.nodeId,
                APIFreeCircleData(
                  center: widget.data!.center,
                  radius: newValue,
                ),
              );
            },
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}
