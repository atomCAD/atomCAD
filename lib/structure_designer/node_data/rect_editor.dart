import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';

/// Editor widget for rectangle nodes
class RectEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIRectData? data;
  final StructureDesignerModel model;

  const RectEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<RectEditor> createState() => RectEditorState();
}

class RectEditorState extends State<RectEditor> {
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
          Text('Rectangle Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          IVec2Input(
            label: 'Min Corner',
            value: widget.data!.minCorner,
            onChanged: (newValue) {
              widget.model.setRectData(
                widget.nodeId,
                APIRectData(
                  minCorner: newValue,
                  extent: widget.data!.extent,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IVec2Input(
            label: 'Extent',
            value: widget.data!.extent,
            onChanged: (newValue) {
              widget.model.setRectData(
                widget.nodeId,
                APIRectData(
                  minCorner: widget.data!.minCorner,
                  extent: newValue,
                ),
              );
            },
            minimumValue: APIIVec2(x: 1, y: 1),
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}
