import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for `patch_latticefill` nodes — tiles a surface-reconstruction
/// patch across a region and welds it in. Stored, editable properties are
/// `passivate` and `tolerance`; the `target` / `region` / `patch` / `origin`
/// inputs are wired. A compatibility badge shows the welded-vs-orphaned collar
/// and over-coordination stats from the last evaluation (§6). See
/// `doc/design_surface_patches.md` §5.
class PatchLatticeFillEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIPatchLatticeFillData? data;
  final StructureDesignerModel model;

  const PatchLatticeFillEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  void _commit({
    bool? passivate,
    double? tolerance,
    bool? testHeightAtOrigin,
    bool? debugProject,
    bool? debugFrontier,
  }) {
    final current = data!;
    model.setPatchLatticefillData(
      nodeId,
      APIPatchLatticeFillData(
        passivate: passivate ?? current.passivate,
        tolerance: tolerance ?? current.tolerance,
        testHeightAtOrigin: testHeightAtOrigin ?? current.testHeightAtOrigin,
        debugProjectToTestPlane:
            debugProject ?? current.debugProjectToTestPlane,
        debugShowFrontierTiles: debugFrontier ?? current.debugShowFrontierTiles,
      ),
    );
  }

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
            title: 'Patch Lattice Fill Properties',
            nodeTypeName: 'patch_latticefill',
          ),
          const SizedBox(height: 8),
          CheckboxListTile(
            title: const Text('Hydrogen Passivation'),
            subtitle: const Text(
              'Saturate the danglers left after welding and dropping ghosts',
            ),
            value: data!.passivate,
            onChanged: (value) => _commit(passivate: value ?? true),
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Weld tolerance (Å)',
            value: data!.tolerance,
            onChanged: (newValue) => _commit(tolerance: newValue),
          ),
          const SizedBox(height: 4),
          const Text(
            'Atoms within this distance fuse into one. Keep below the smallest '
            'interatomic spacing so distinct sites never over-merge.',
            style: TextStyle(fontSize: 11, color: Colors.grey),
          ),
          const SizedBox(height: 8),
          CheckboxListTile(
            title: const Text('Test height at lattice origin'),
            subtitle: const Text(
              'Off (default): cell selection derives the test height from the '
              'target slab — robust to a target offset from the origin. On: '
              'tests at the lattice origin height (simpler, but selects nothing '
              'if the target does not straddle the origin).',
            ),
            value: data!.testHeightAtOrigin,
            onChanged: (value) => _commit(testHeightAtOrigin: value ?? false),
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
          ),
          const SizedBox(height: 12),
          _CompatibilityBadge(report: data!.report),
          const SizedBox(height: 12),
          const Divider(),
          const Text(
            'Debug (cell selection)',
            style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12),
          ),
          CheckboxListTile(
            title: const Text('Project atoms to test plane'),
            subtitle: const Text(
              'Flatten the placed atoms onto the plane cell selection tests '
              '(no weld). Shows why a tile was included. Non-physical.',
            ),
            value: data!.debugProjectToTestPlane,
            onChanged: (value) => _commit(debugProject: value ?? false),
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
          ),
          CheckboxListTile(
            title: const Text('Show frontier tiles'),
            subtitle: const Text(
              'Also place the one-cell-wider ring of tiles, flagging the '
              'excluded neighbours frozen so the boundary is visible.',
            ),
            value: data!.debugShowFrontierTiles,
            onChanged: (value) => _commit(debugFrontier: value ?? false),
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
          ),
        ],
      ),
    );
  }
}

/// Compatibility badge (§6): summarizes the weld outcome of the last apply.
/// Green when every collar welded and nothing is over-coordinated; amber
/// otherwise, naming the likely failure mode. Hidden until the node has
/// evaluated.
class _CompatibilityBadge extends StatelessWidget {
  final APICompatibilityReport? report;

  const _CompatibilityBadge({required this.report});

  @override
  Widget build(BuildContext context) {
    final report = this.report;
    if (report == null) {
      return const Text(
        'Compatibility: not yet evaluated. Display this node to compute the '
        'weld stats.',
        style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
      );
    }

    final placed = report.placedCells;
    final welded = report.weldedGhosts;
    final orphaned = report.orphanedGhosts;
    final overcoordinated = report.overcoordinatedAtoms;
    // Nothing tiled: the other counts are all zero, but that is failure, not
    // success — the test plane missed the target (usually the wrong test-height
    // mode for an off-origin target). Flag it distinctly.
    final nothingPlaced = placed == BigInt.zero;
    final clean = !nothingPlaced &&
        orphaned == BigInt.zero &&
        overcoordinated == BigInt.zero;

    final Color color = nothingPlaced
        ? Colors.red.shade700
        : (clean ? Colors.green.shade700 : Colors.orange.shade800);
    final IconData icon = nothingPlaced
        ? Icons.error_outline
        : (clean ? Icons.check_circle : Icons.warning_amber);
    final String headline = nothingPlaced
        ? 'No tiles placed'
        : (clean ? 'Compatible' : 'Check fit');

    final hints = <String>[];
    if (nothingPlaced) {
      hints.add(
        'No cell was selected, so the patch added nothing. The test plane '
        'missed the target — if the target is offset from the lattice origin '
        'along the surface normal, uncheck "Test height at lattice origin".',
      );
    }
    if (orphaned > BigInt.zero) {
      hints.add(
        'Orphaned collars suggest the patch sits too high (floating, '
        'un-welded). They are dropped as reconstruction edges.',
      );
    }
    if (overcoordinated > BigInt.zero) {
      hints.add(
        'Over-coordinated atoms suggest the patch sits too low / into the '
        'sub-surface.',
      );
    }

    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(8.0),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(4.0),
        border: Border.all(color: color, width: 1.0),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(icon, color: color, size: 18),
              const SizedBox(width: 6),
              Text(
                headline,
                style: TextStyle(color: color, fontWeight: FontWeight.bold),
              ),
            ],
          ),
          const SizedBox(height: 6),
          _statLine('Tiles placed', placed),
          _statLine('Welded collars / ghosts', welded),
          _statLine('Orphaned (dropped) ghosts', orphaned),
          _statLine('Over-coordinated atoms', overcoordinated),
          for (final hint in hints) ...[
            const SizedBox(height: 6),
            Text(
              hint,
              style: const TextStyle(fontSize: 11, color: Colors.grey),
            ),
          ],
        ],
      ),
    );
  }

  Widget _statLine(String label, BigInt value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1.0),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: const TextStyle(fontSize: 12)),
          Text(
            '$value',
            style: const TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}
