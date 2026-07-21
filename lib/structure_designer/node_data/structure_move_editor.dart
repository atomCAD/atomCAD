import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Index of the `subdivision` input pin on `structure_move`.
/// 0 = input, 1 = translation, 2 = subdivision.
const int _SUBDIVISION_PIN_INDEX = 2;

/// Editor widget for structure_move nodes.
///
/// When the optional `subdivision` input pin is wired, the wired value wins at
/// eval (and drives the drag gizmo's step size), so the field renders disabled
/// but keeps its stored value for re-activation on disconnect — the standard
/// "disable on wired input" pattern.
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
  /// True when the optional `subdivision` input pin is wired. Detected by
  /// walking the current network view's wires (see node_data/AGENTS.md
  /// "Disable on wired input" pattern).
  bool _isSubdivisionPinConnected() {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId &&
          wire.destParamIndex == BigInt.from(_SUBDIVISION_PIN_INDEX)) {
        return true;
      }
    }
    return false;
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final subdivisionConnected = _isSubdivisionPinConnected();

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
          if (subdivisionConnected)
            Padding(
              padding: const EdgeInsets.only(bottom: 8.0),
              child: Text(
                'Subdivision supplied by `subdivision` input. Disconnect to '
                'edit inline.',
                style: TextStyle(
                  fontStyle: FontStyle.italic,
                  fontSize: 12,
                  color: Theme.of(context).colorScheme.primary,
                ),
              ),
            ),
          Opacity(
            opacity: subdivisionConnected ? 0.5 : 1.0,
            child: IgnorePointer(
              ignoring: subdivisionConnected,
              child: IntInput(
                label: 'Subdivision',
                value: widget.data!.latticeSubdivision,
                minimumValue: 1,
                onChanged: (newValue) {
                  widget.model.setStructureMoveData(
                    widget.nodeId,
                    APIStructureMoveData(
                      translation: widget.data!.translation,
                      latticeSubdivision: newValue,
                    ),
                  );
                },
              ),
            ),
          ),
        ],
      ),
    );
  }
}
