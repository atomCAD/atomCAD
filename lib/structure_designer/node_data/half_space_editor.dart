import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for half_space nodes
class HalfSpaceEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIHalfSpaceData? data;
  final StructureDesignerModel model;

  const HalfSpaceEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<HalfSpaceEditor> createState() => HalfSpaceEditorState();
}

class HalfSpaceEditorState extends State<HalfSpaceEditor> {
  // Direct API calls are made in onChanged handlers

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
            Text('Half Space Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IVec3Input(
              label: 'Miller Index',
              value: widget.data!.millerIndex,
              onChanged: (newValue) {
                widget.model.setHalfSpaceData(
                  widget.nodeId,
                  APIHalfSpaceData(
                    millerIndex: newValue,
                    center: widget.data!.center,
                  ),
                );
              },
            ),
            const SizedBox(height: 8),
            IVec3Input(
              label: 'Center',
              value: widget.data!.center,
              onChanged: (newValue) {
                widget.model.setHalfSpaceData(
                  widget.nodeId,
                  APIHalfSpaceData(
                    millerIndex: widget.data!.millerIndex,
                    center: newValue,
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
