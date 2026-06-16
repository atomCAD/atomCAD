import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for `patch_build` nodes — the "draw, don't assemble"
/// authoring step for surface-reconstruction patches. The slab (`source`),
/// `lattice`, `tiling_vectors`, and `cut_volume` are all wired inputs (the
/// `tiling_vectors` typically come from a `plane_tiling_vectors` node); the only
/// stored, editable property is the build threshold `epsilon`. See
/// `doc/design_surface_patches.md` §4.
class PatchBuildEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIPatchBuildData? data;
  final StructureDesignerModel model;

  const PatchBuildEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Patch Build Properties',
            nodeTypeName: 'patch_build',
          ),
          const SizedBox(height: 8),
          const Text(
            'Wire the authored slab into `source`, its crystal into `lattice`, '
            'the periodic directions into `tiling_vectors` (e.g. from a '
            '`plane_tiling_vectors` node), and one tile\'s volume into '
            '`cut_volume`.',
            style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
          ),
          const SizedBox(height: 12),
          FloatInput(
            label: 'Build threshold ε (Å)',
            value: data!.epsilon,
            onChanged: (newValue) {
              model.setPatchBuildData(
                nodeId,
                APIPatchBuildData(epsilon: newValue),
              );
            },
          ),
          const SizedBox(height: 4),
          const Text(
            'An atom counts as interior when its cut-volume membership ≤ ε. Keep '
            'it above any on-surface jitter but well below the interplanar '
            'spacing so it never grabs the layer below.',
            style: TextStyle(fontSize: 11, color: Colors.grey),
          ),
        ],
      ),
    );
  }
}
