import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/miller_index_map.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// Editor widget for individual facets within a facet shell
class FacetEditor extends StatelessWidget {
  final BigInt nodeId;
  final BigInt facetIndex;
  final APIFacet facet;
  final int maxMillerIndex;
  final StructureDesignerModel model;

  const FacetEditor({
    super.key,
    required this.nodeId,
    required this.facetIndex,
    required this.facet,
    required this.maxMillerIndex,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.symmetric(vertical: 4.0),
      elevation: 2,
      child: Padding(
        padding: const EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Facet $facetIndex',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),

            // Miller Index input
            IVec3Input(
              label: 'Miller Index',
              value: facet.millerIndex,
              minimumValue: APIIVec3(
                x: -maxMillerIndex,
                y: -maxMillerIndex,
                z: -maxMillerIndex,
              ),
              maximumValue: APIIVec3(
                x: maxMillerIndex,
                y: maxMillerIndex,
                z: maxMillerIndex,
              ),
              onChanged: (value) {
                // Immediately update the facet when miller index changes
                model.updateFacet(
                  nodeId,
                  facetIndex,
                  APIFacet(
                    millerIndex: value,
                    shift: facet.shift,
                    symmetrize: facet.symmetrize,
                    visible: facet.visible,
                  ),
                );
              },
            ),

            const SizedBox(height: 12),

            // Shift input
            IntInput(
              label: 'Shift',
              value: facet.shift,
              onChanged: (value) {
                // Immediately update the facet when shift changes
                model.updateFacet(
                  nodeId,
                  facetIndex,
                  APIFacet(
                    millerIndex: facet.millerIndex,
                    shift: value,
                    symmetrize: facet.symmetrize,
                    visible: facet.visible,
                  ),
                );
              },
            ),
            const SizedBox(height: 12),

            // Symmetrize checkbox
            Row(
              children: [
                Checkbox(
                  value: facet.symmetrize,
                  onChanged: (value) {
                    // Immediately update the facet when symmetrize changes
                    model.updateFacet(
                      nodeId,
                      facetIndex,
                      APIFacet(
                        millerIndex: facet.millerIndex,
                        shift: facet.shift,
                        symmetrize: value ?? false,
                        visible: facet.visible,
                      ),
                    );
                  },
                ),
                const Text('Symmetrize'),
              ],
            ),

            // Split symmetry members button - only show if facet is symmetrized
            if (facet.symmetrize) ...[
              const SizedBox(height: 12),
              ElevatedButton(
                style: AppButtonStyles.primary,
                onPressed: () {
                  // Split this symmetrized facet into individual facets
                  model.splitSymmetryMembers(nodeId, facetIndex);
                },
                child: const Text('Split sym. members'),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
