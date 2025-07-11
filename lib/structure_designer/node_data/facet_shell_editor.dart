import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for facet_shell nodes
class FacetShellEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIFacetShellData? data;
  final StructureDesignerModel model;

  const FacetShellEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<FacetShellEditor> createState() => FacetShellEditorState();
}

class FacetShellEditorState extends State<FacetShellEditor> {
  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(4.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Facet Shell Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          // Max Miller Index input
          IntInput(
            label: 'Max Miller Index',
            value: widget.data!.maxMillerIndex,
            minimumValue: 1, // Must be at least 1
            maximumValue: 10, // Set a reasonable upper limit
            onChanged: (newValue) {
              widget.model.setFacetShellCenter(
                widget.nodeId,
                widget.data!.center,
                newValue,
              );
            },
          ),
          const SizedBox(height: 12),
          // Center input
          IVec3Input(
            label: 'Center',
            value: widget.data!.center,
            onChanged: (newValue) {
              widget.model.setFacetShellCenter(
                widget.nodeId,
                newValue,
                widget.data!.maxMillerIndex,
              );
            },
          ),
          // The facets management will be added in the next step
          const SizedBox(height: 12),
          Text(
            'Facets: ${widget.data!.facets.length}',
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          Text(
            'Facet management UI will be added in the next step',
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  fontStyle: FontStyle.italic,
                  color: Colors.grey,
                ),
          ),
        ],
      ),
    );
  }
}
