import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/miller_index_map.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Interim picker UI for the turntable navigation-up axis (issue #349, Phase 3).
///
/// This is a deliberately replaceable front-end (design decision D7): every
/// picking flow funnels through the same Rust setters, and #391's unified
/// direction/Miller-plane gizmo later replaces this dialog body. It reuses the
/// existing `MillerIndexMap` + numeric fields — zero new picker technology.
enum _ViewUpMode { plane, direction }

void showViewUpAxisDialog(BuildContext context, StructureDesignerModel model) {
  showDialog(
    context: context,
    barrierDismissible: false, // DraggableDialog handles its own barrier
    builder: (context) => _ViewUpAxisDialog(model: model),
  );
}

class _ViewUpAxisDialog extends StatefulWidget {
  final StructureDesignerModel model;

  const _ViewUpAxisDialog({required this.model});

  @override
  State<_ViewUpAxisDialog> createState() => _ViewUpAxisDialogState();
}

class _ViewUpAxisDialogState extends State<_ViewUpAxisDialog> {
  // Plane (hkl) and lattice direction [uvw] are strictly separate inputs (D2):
  // for non-cubic lattices the (hkl) plane normal is not the [uvw] direction.
  // The index value is shared across the two modes for editing convenience;
  // Apply resolves it through the mode-appropriate setter.
  _ViewUpMode _mode = _ViewUpMode.plane;
  APIIVec3 _index = const APIIVec3(x: 0, y: 0, z: 1);
  String? _error;

  @override
  void initState() {
    super.initState();
    // Seed the fields from the current axis so the dialog opens reflecting live
    // state (a bracketed "(h k l)" / "[u v w]" label), not the bare default.
    _adoptFromLabel(widget.model.viewUpInfo?.label);
  }

  void _apply() {
    final String? error;
    switch (_mode) {
      case _ViewUpMode.plane:
        error = widget.model.setViewUpFromMillerPlane(_index);
        break;
      case _ViewUpMode.direction:
        error = widget.model.setViewUpFromLatticeDirection(_index);
        break;
    }
    setState(() => _error = error);
  }

  void _fromDisplayedPlane() {
    final error = widget.model.setViewUpFromActiveDrawingPlane();
    setState(() {
      _error = error;
      // On success, mirror the just-applied axis into the fields so a
      // subsequent Apply re-resolves the *same* axis instead of jumping back to
      // whatever stale index the fields held. The kernel labels a drawing plane
      // as "(h k l)", so parse that back into the Plane-mode index.
      if (error == null) {
        _adoptFromLabel(widget.model.viewUpInfo?.label);
      }
    });
  }

  /// Parse a provenance label ("(h k l)" / "[u v w]") back into the mode + index
  /// so the fields reflect the current axis. No-op on labels that aren't a
  /// bracketed index triple (e.g. "Z").
  void _adoptFromLabel(String? label) {
    if (label == null || label.length < 2) return;
    final _ViewUpMode mode;
    if (label.startsWith('(') && label.endsWith(')')) {
      mode = _ViewUpMode.plane;
    } else if (label.startsWith('[') && label.endsWith(']')) {
      mode = _ViewUpMode.direction;
    } else {
      return;
    }
    final parts = label
        .substring(1, label.length - 1)
        .split(RegExp(r'\s+'))
        .where((s) => s.isNotEmpty)
        .toList();
    if (parts.length != 3) return;
    final xs = parts.map(int.tryParse).toList();
    if (xs.any((v) => v == null)) return;
    _mode = mode;
    _index = APIIVec3(x: xs[0]!, y: xs[1]!, z: xs[2]!);
  }

  void _reset() {
    widget.model.resetViewUp();
    setState(() => _error = null);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final info = widget.model.viewUpInfo;
    final indexLabel =
        _mode == _ViewUpMode.plane ? 'Plane (h k l)' : 'Direction [u v w]';

    return DraggableDialog(
      width: 420,
      dismissible: true,
      backgroundColor: theme.dialogBackgroundColor,
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Navigation Up Axis', style: theme.textTheme.titleLarge),
            const SizedBox(height: 4),
            Text(
              'Pick the axis that stays vertical on screen while orbiting.',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 12),

            // Current axis + lattice source (D5): the fallback is never silent.
            if (info != null) ...[
              Text('Current: ${info.label}',
                  style: theme.textTheme.bodyMedium),
              const SizedBox(height: 2),
              Text('Resolving against: ${info.latticeSourceLabel}',
                  style: theme.textTheme.bodySmall
                      ?.copyWith(fontStyle: FontStyle.italic)),
              const SizedBox(height: 12),
            ],

            // Mode toggle — two labeled modes, never mixed (D2).
            SegmentedButton<_ViewUpMode>(
              segments: const [
                ButtonSegment(
                    value: _ViewUpMode.plane, label: Text('Plane (hkl)')),
                ButtonSegment(
                    value: _ViewUpMode.direction,
                    label: Text('Direction [uvw]')),
              ],
              selected: {_mode},
              onSelectionChanged: (s) => setState(() => _mode = s.first),
            ),
            const SizedBox(height: 12),

            MillerIndexMap(
              label: indexLabel,
              value: _index,
              onChanged: (v) => setState(() => _index = v),
              mapWidth: 380,
              mapHeight: 180,
              dotColor: theme.brightness == Brightness.dark
                  ? Colors.grey.shade600
                  : Colors.grey.shade400,
              selectedDotColor: Colors.red,
            ),
            const SizedBox(height: 8),
            IVec3Input(
              label: '$indexLabel (numeric)',
              value: _index,
              onChanged: (v) => setState(() => _index = v),
            ),

            if (_error != null) ...[
              const SizedBox(height: 12),
              Text(_error!,
                  style: theme.textTheme.bodySmall
                      ?.copyWith(color: theme.colorScheme.error)),
            ],

            const SizedBox(height: 16),
            // One-click path for the motivating workflow: pull the axis from the
            // active node's displayed drawing plane.
            Align(
              alignment: Alignment.centerLeft,
              child: OutlinedButton.icon(
                icon: const Icon(Icons.layers, size: 18),
                label: const Text('From displayed plane'),
                onPressed: _fromDisplayedPlane,
              ),
            ),
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _reset,
                  child: const Text('Reset (Z)'),
                ),
                const SizedBox(width: 8),
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Close'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _apply,
                  child: const Text('Apply'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
