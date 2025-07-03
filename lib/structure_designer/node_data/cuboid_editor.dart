import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';

/// Editor widget for cuboid nodes
class CuboidEditor extends StatefulWidget {
  final BigInt nodeId;
  final APICuboidData? data;

  const CuboidEditor({
    super.key,
    required this.nodeId,
    required this.data,
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
      child: SingleChildScrollView(
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
                setCuboidData(
                  nodeId: widget.nodeId,
                  data: APICuboidData(
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
              onChanged: (newValue) {
                setCuboidData(
                  nodeId: widget.nodeId,
                  data: APICuboidData(
                    minCorner: widget.data!.minCorner,
                    extent: newValue,
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
