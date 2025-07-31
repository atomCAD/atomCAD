import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for geo_trans nodes
class GeoTransEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIGeoTransData? data;
  final StructureDesignerModel model;

  const GeoTransEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<GeoTransEditor> createState() => GeoTransEditorState();
}

class GeoTransEditorState extends State<GeoTransEditor> {
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
          Text('Geo Transformation Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          IVec3Input(
            label: 'Translation',
            value: widget.data!.translation,
            onChanged: (newValue) {
              widget.model.setGeoTransData(
                widget.nodeId,
                APIGeoTransData(
                  transformOnlyFrame: widget.data!.transformOnlyFrame,
                  translation: newValue,
                  rotation: widget.data!.rotation,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IVec3Input(
            label: 'Rotation',
            value: widget.data!.rotation,
            onChanged: (newValue) {
              widget.model.setGeoTransData(
                widget.nodeId,
                APIGeoTransData(
                  transformOnlyFrame: widget.data!.transformOnlyFrame,
                  translation: widget.data!.translation,
                  rotation: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
