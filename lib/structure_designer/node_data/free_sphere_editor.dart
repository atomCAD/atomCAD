import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for free_sphere nodes.
/// Center and radius are authored directly in real-space (Å) coordinates —
/// the non-lattice-aligned analog of the `sphere` node.
class FreeSphereEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIFreeSphereData? data;
  final StructureDesignerModel model;

  const FreeSphereEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<FreeSphereEditor> createState() => FreeSphereEditorState();
}

class FreeSphereEditorState extends State<FreeSphereEditor> {
  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const NodeEditorHeader(
              title: 'Free Sphere Properties',
              nodeTypeName: 'free_sphere',
            ),
            const SizedBox(height: 8),
            Vec3Input(
              label: 'Center (Å)',
              value: widget.data!.center,
              onChanged: (newValue) {
                widget.model.setFreeSphereData(
                  widget.nodeId,
                  APIFreeSphereData(
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
                widget.model.setFreeSphereData(
                  widget.nodeId,
                  APIFreeSphereData(
                    center: widget.data!.center,
                    radius: newValue,
                  ),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
