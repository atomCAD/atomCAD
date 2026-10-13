import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/tag_name_suggestions.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for the `compare_atoms` node — compares two atomic structures
/// up to a tolerance and, optionally, a rigid motion
/// (`doc/design_structure_comparison.md` §4).
///
/// Four stored properties and no pins for them, so nothing here greys out on a
/// wire. The verdict and the report are output pins, read by hovering them;
/// the panel holds settings only.
class CompareAtomsEditor extends StatelessWidget {
  final BigInt nodeId;
  final APICompareAtomsData? data;
  final StructureDesignerModel model;

  const CompareAtomsEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  /// Writes the four properties back with one of them replaced.
  /// `availableTags` is sent empty — the Rust setter drops it.
  void _commit({
    String? tag,
    APICompareMotion? motion,
    double? tolerance,
    bool? compareBonds,
  }) {
    final data = this.data;
    if (data == null) return;
    model.setCompareAtomsData(
      nodeId,
      APICompareAtomsData(
        tag: tag ?? data.tag,
        motion: motion ?? data.motion,
        tolerance: tolerance ?? data.tolerance,
        compareBonds: compareBonds ?? data.compareBonds,
        availableTags: const [],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final data = this.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Compare Atoms',
            nodeTypeName: 'compare_atoms',
          ),
          const SizedBox(height: 16),
          StringInput(
            // Key on the stored value so the field rebuilds its controller
            // when a suggestion chip changes it underneath.
            key: ValueKey('compare_atoms_tag_${data.tag}'),
            label: 'Tag (empty = all atoms)',
            value: data.tag,
            onChanged: (value) => _commit(tag: value.trim()),
          ),
          TagNameSuggestions(
            available: data.availableTags,
            currentName: data.tag,
            onPick: (name) => _commit(tag: name),
            caption: 'Tags in a or b',
          ),
          const SizedBox(height: 16),
          _MotionDropdown(
            value: data.motion,
            onChanged: (v) => _commit(motion: v),
          ),
          const SizedBox(height: 16),
          FloatInput(
            label: 'Tolerance (Å, at most 0.3)',
            value: data.tolerance,
            onChanged: (v) => _commit(tolerance: v),
          ),
          const SizedBox(height: 8),
          CheckboxListTile(
            title: const Text('Compare bonds'),
            value: data.compareBonds,
            onChanged: (v) {
              if (v != null) _commit(compareBonds: v);
            },
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
          ),
        ],
      ),
    );
  }
}

/// The motion the two structures may differ by.
class _MotionDropdown extends StatelessWidget {
  final APICompareMotion value;
  final ValueChanged<APICompareMotion> onChanged;

  const _MotionDropdown({required this.value, required this.onChanged});

  static String _label(APICompareMotion m) => switch (m) {
        APICompareMotion.none => 'None (compared in place)',
        APICompareMotion.translation => 'Translation',
        APICompareMotion.rigid => 'Rigid motion',
        APICompareMotion.rigidMirror => 'Rigid motion or mirror',
      };

  @override
  Widget build(BuildContext context) {
    return DropdownButtonFormField<APICompareMotion>(
      key: ValueKey('compare_atoms_motion_$value'),
      decoration: const InputDecoration(
        labelText: 'Motion',
        border: OutlineInputBorder(),
        isDense: true,
        contentPadding: EdgeInsets.symmetric(horizontal: 8, vertical: 8),
      ),
      isExpanded: true,
      value: value,
      items: [
        for (final m in APICompareMotion.values)
          DropdownMenuItem(value: m, child: Text(_label(m))),
      ],
      onChanged: (v) {
        if (v != null) onChanged(v);
      },
    );
  }
}
