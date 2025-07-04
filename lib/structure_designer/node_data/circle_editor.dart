import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';

/// Editor widget for circle nodes
class CircleEditor extends StatefulWidget {
  final BigInt nodeId;
  final APICircleData? data;
  final StructureDesignerModel model;

  const CircleEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<CircleEditor> createState() => CircleEditorState();
}

class CircleEditorState extends State<CircleEditor> {
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
            Text('Circle Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IVec2Input(
              label: 'Center',
              value: widget.data!.center,
              onChanged: (newValue) {
                widget.model.setCircleData(
                  widget.nodeId,
                  APICircleData(
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
              onChanged: (newValue) {
                widget.model.setCircleData(
                  widget.nodeId,
                  APICircleData(
                    center: widget.data!.center,
                    radius: newValue,
                  ),
                );
              },
              minimumValue: 1,
            ),
            const SizedBox(height: 16),
          ],
        ),
      ),
    );
  }
}
