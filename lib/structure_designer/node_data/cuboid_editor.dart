import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for cuboid nodes
class CuboidEditor extends StatefulWidget {
  final BigInt nodeId;
  final APICuboidData? data;
  final StructureDesignerModel model;

  const CuboidEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<CuboidEditor> createState() => CuboidEditorState();
}

class CuboidEditorState extends State<CuboidEditor> {
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
          Text('Cuboid Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          IVec3Input(
            label: 'Min Corner',
            value: widget.data!.minCorner,
            onChanged: (newValue) {
              widget.model.setCuboidData(
                widget.nodeId,
                APICuboidData(
                  minCorner: newValue,
                  extent: widget.data!.extent,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IVec3Input(
            label: 'Extent',
            value: widget.data!.extent,
            minimumValue: APIIVec3(x: 1, y: 1, z: 1),
            onChanged: (newValue) {
              widget.model.setCuboidData(
                widget.nodeId,
                APICuboidData(
                  minCorner: widget.data!.minCorner,
                  extent: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
