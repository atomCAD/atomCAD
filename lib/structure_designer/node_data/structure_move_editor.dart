import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Indexes of the subdivision input pins on `structure_move`.
/// 0 = input, 1 = translation, 2 = subdivision (uniform Int),
/// 3 = subdiv_xyz (per-axis IVec3, overrides pin 2 when wired).
const int _SUBDIVISION_PIN_INDEX = 2;
const int _SUBDIV_XYZ_PIN_INDEX = 3;

/// Editor widget for structure_move nodes.
///
/// The stored subdivision is per-axis (IVec3), but a uniform value is the
/// common case, so the panel shows a single int field by default; a "Per-axis"
/// checkbox swaps in the three-component editor. When either subdivision input
/// pin is wired, the wired value wins at eval (`subdiv_xyz` over `subdivision`
/// over stored; it also drives the drag gizmo's step size), so the fields
/// render disabled but keep their stored value for re-activation on
/// disconnect — the standard "disable on wired input" pattern.
class StructureMoveEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIStructureMoveData? data;
  final StructureDesignerModel model;
  final String title;
  final String nodeTypeName;

  const StructureMoveEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
    this.title = 'Structure Move Properties',
    this.nodeTypeName = 'structure_move',
  });

  @override
  State<StructureMoveEditor> createState() => _StructureMoveEditorState();
}

class _StructureMoveEditorState extends State<StructureMoveEditor> {
  /// User override for the per-axis checkbox. Null means "derive from the
  /// stored value" (checked iff the components differ), so a node whose
  /// subdivision is already non-uniform opens with the vector editor.
  bool? _perAxisOverride;

  @override
  void didUpdateWidget(StructureMoveEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.nodeId != widget.nodeId) {
      _perAxisOverride = null;
    }
  }

  /// True when the input pin at [pinIndex] is wired. Detected by walking the
  /// current network view's wires (see node_data/AGENTS.md "Disable on wired
  /// input" pattern).
  bool _isPinConnected(int pinIndex) {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId &&
          wire.destParamIndex == BigInt.from(pinIndex)) {
        return true;
      }
    }
    return false;
  }

  bool _isUniform(APIIVec3 v) => v.x == v.y && v.y == v.z;

  void _commitSubdivision(APIIVec3 newValue) {
    widget.model.setStructureMoveData(
      widget.nodeId,
      APIStructureMoveData(
        translation: widget.data!.translation,
        latticeSubdivision: newValue,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final subdivisionConnected = _isPinConnected(_SUBDIVISION_PIN_INDEX);
    final subdivXyzConnected = _isPinConnected(_SUBDIV_XYZ_PIN_INDEX);
    final anySubdivisionConnected = subdivisionConnected || subdivXyzConnected;
    // `subdiv_xyz` overrides `subdivision`, so name the pin that actually wins.
    final winningPinName = subdivXyzConnected ? 'subdiv_xyz' : 'subdivision';

    final subdivision = widget.data!.latticeSubdivision;
    final perAxis = _perAxisOverride ?? !_isUniform(subdivision);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          NodeEditorHeader(
            title: widget.title,
            nodeTypeName: widget.nodeTypeName,
          ),
          const SizedBox(height: 16),

          // Translation input
          IVec3Input(
            label: 'Translation',
            value: widget.data!.translation,
            onChanged: (newValue) {
              widget.model.setStructureMoveData(
                widget.nodeId,
                APIStructureMoveData(
                  translation: newValue,
                  latticeSubdivision: widget.data!.latticeSubdivision,
                ),
              );
            },
          ),
          const SizedBox(height: 16),

          // Subdivision input
          if (anySubdivisionConnected)
            Padding(
              padding: const EdgeInsets.only(bottom: 8.0),
              child: Text(
                'Subdivision supplied by `$winningPinName` input. Disconnect '
                'to edit inline.',
                style: TextStyle(
                  fontStyle: FontStyle.italic,
                  fontSize: 12,
                  color: Theme.of(context).colorScheme.primary,
                ),
              ),
            ),
          Opacity(
            opacity: anySubdivisionConnected ? 0.5 : 1.0,
            child: IgnorePointer(
              ignoring: anySubdivisionConnected,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (perAxis)
                    IVec3Input(
                      label: 'Subdivision',
                      value: subdivision,
                      minimumValue: const APIIVec3(x: 1, y: 1, z: 1),
                      onChanged: _commitSubdivision,
                    )
                  else
                    IntInput(
                      label: 'Subdivision',
                      value: subdivision.x,
                      minimumValue: 1,
                      onChanged: (newValue) => _commitSubdivision(
                        APIIVec3(x: newValue, y: newValue, z: newValue),
                      ),
                    ),
                  Row(
                    children: [
                      Checkbox(
                        value: perAxis,
                        onChanged: (checked) {
                          setState(() {
                            _perAxisOverride = checked ?? false;
                          });
                          // Unchecking collapses the vector to its x component
                          // so the int field never lies about the stored value.
                          if (checked == false && !_isUniform(subdivision)) {
                            _commitSubdivision(APIIVec3(
                              x: subdivision.x,
                              y: subdivision.x,
                              z: subdivision.x,
                            ));
                          }
                        },
                      ),
                      const Text('Per-axis', style: TextStyle(fontSize: 13)),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
