import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';

/// Editor widget for sphere nodes
class SphereEditor extends StatefulWidget {
  final BigInt nodeId;
  final APISphereData? data;
  final StructureDesignerModel model;

  const SphereEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<SphereEditor> createState() => SphereEditorState();
}

class SphereEditorState extends State<SphereEditor> {
  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      key: PropertyEditorKeys.sphereEditor,
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const NodeEditorHeader(
              title: 'Sphere Properties',
              nodeTypeName: 'sphere',
            ),
            const SizedBox(height: 8),
            IVec3Input(
              label: 'Center',
              value: widget.data!.center,
              xInputKey: PropertyEditorKeys.sphereCenterXInput,
              yInputKey: PropertyEditorKeys.sphereCenterYInput,
              zInputKey: PropertyEditorKeys.sphereCenterZInput,
              onChanged: (newValue) {
                widget.model.setSphereData(
                  widget.nodeId,
                  APISphereData(
                    center: newValue,
                    radius: widget.data!.radius,
                  ),
                );
              },
            ),
            const SizedBox(height: 8),
            IntInput(
              label: 'Radius',
              value: widget.data!.radius,
              minimumValue: 1,
              inputKey: PropertyEditorKeys.sphereRadiusInput,
              onChanged: (newValue) {
                widget.model.setSphereData(
                  widget.nodeId,
                  APISphereData(
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
