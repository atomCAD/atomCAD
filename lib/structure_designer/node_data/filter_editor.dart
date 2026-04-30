import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for filter nodes
class FilterEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIFilterData? data;
  final StructureDesignerModel model;

  const FilterEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<FilterEditor> createState() => FilterEditorState();
}

class FilterEditorState extends State<FilterEditor> {
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
            title: 'Filter Properties',
            nodeTypeName: 'filter',
          ),
          const SizedBox(height: 8),

          // Element Type
          DataTypeInput(
            label: 'Element Type',
            value: widget.data!.elementType,
            onChanged: (newValue) {
              widget.model.setFilterData(
                widget.nodeId,
                APIFilterData(elementType: newValue),
              );
            },
          ),
        ],
      ),
    );
  }
}
