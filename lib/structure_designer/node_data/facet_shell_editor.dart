import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'facet_editor.dart';

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
  /// Calculates the default shift value for a new facet based on existing facets.
  /// Facets with symmetrize=true count as 6 facets in the average.
  /// Returns the rounded average, or 1 if there are no existing facets.
  int calculateDefaultShift() {
    if (widget.data == null || widget.data!.facets.isEmpty) {
      return 1; // Default value when no facets exist
    }

    int totalShift = 0;
    int facetCount = 0;

    for (final facet in widget.data!.facets) {
      // If symmetrize is true, count this shift 6 times (once for each symmetry plane)
      final weight = facet.symmetrize ? 6 : 1;
      totalShift += facet.shift * weight;
      facetCount += weight;
    }

    // Calculate average and round to nearest integer
    return (totalShift / facetCount).round();
  }

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

          const SizedBox(height: 16),
          Text('Facets', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),

          // Facet list with selection capability
          Container(
            decoration: BoxDecoration(
              border: Border.all(color: Colors.grey.shade300),
              borderRadius: BorderRadius.circular(4),
            ),
            height: 150,
            child: widget.data!.facets.isEmpty
                ? const Center(child: Text('No facets defined'))
                : ListView.builder(
                    itemCount: widget.data!.facets.length,
                    itemBuilder: (context, index) {
                      final facet = widget.data!.facets[index];
                      final isSelected =
                          widget.data!.selectedFacetIndex == BigInt.from(index);

                      return ListTile(
                        dense: true,
                        selected: isSelected,
                        selectedTileColor: Colors.lightBlue.withOpacity(0.1),
                        title: Text(
                            'Facet $index: ${facet.millerIndex.x}, ${facet.millerIndex.y}, ${facet.millerIndex.z}'),
                        subtitle: Text(
                            'Shift: ${facet.shift}, Symmetrize: ${facet.symmetrize}'),
                        onTap: () {
                          // Toggle selection
                          widget.model.selectFacet(
                            widget.nodeId,
                            isSelected ? null : BigInt.from(index),
                          );
                        },
                      );
                    },
                  ),
          ),

          const SizedBox(height: 8),

          // Buttons row for facet actions
          Row(
            children: [
              ElevatedButton(
                onPressed: () {
                  final defaultShift = calculateDefaultShift();
                  widget.model.addFacet(
                    widget.nodeId,
                    APIFacet(
                      millerIndex: const APIIVec3(x: 1, y: 0, z: 0),
                      shift: defaultShift,
                      symmetrize: true,
                      visible: true,
                    ),
                  );
                },
                child: const Text('Add Facet'),
              ),
              const SizedBox(width: 8),
              ElevatedButton(
                onPressed: widget.data!.selectedFacetIndex != null
                    ? () {
                        widget.model.removeFacet(
                          widget.nodeId,
                          widget.data!.selectedFacetIndex!,
                        );
                      }
                    : null,
                child: const Text('Remove Selected'),
              ),
              const SizedBox(width: 8),
              ElevatedButton(
                onPressed: widget.data!.facets.isNotEmpty
                    ? () => widget.model.clearFacets(widget.nodeId)
                    : null,
                child: const Text('Clear All'),
              ),
            ],
          ),

          const SizedBox(height: 16),

          // Facet editor for the selected facet
          if (widget.data!.selectedFacetIndex != null &&
              widget.data!.selectedFacetIndex! <
                  BigInt.from(widget.data!.facets.length))
            FacetEditor(
              nodeId: widget.nodeId,
              facetIndex: widget.data!.selectedFacetIndex!,
              facet:
                  widget.data!.facets[widget.data!.selectedFacetIndex!.toInt()],
              maxMillerIndex: widget.data!.maxMillerIndex,
              model: widget.model,
            )
          else
            Text(
              'Select a facet to edit its properties',
              style: Theme.of(context)
                  .textTheme
                  .bodyMedium
                  ?.copyWith(fontStyle: FontStyle.italic),
            )
        ],
      ),
    );
  }
}
