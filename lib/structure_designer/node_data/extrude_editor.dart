import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for extrude nodes.
///
/// The extrusion direction has three sources, in precedence order:
///   1. a wired `dir` input pin (parameter index 3) — overrides everything;
///   2. plane-normal mode (`plane_normal == true`) — the direction tracks the
///      drawing plane's normal, recomputed on every evaluation;
///   3. the stored `extrude_direction` vector (direct mode).
/// See issue #364.
class ExtrudeEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIExtrudeData? data;
  final StructureDesignerModel model;

  const ExtrudeEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ExtrudeEditor> createState() => ExtrudeEditorState();
}

/// The `dir` input pin's parameter index on the extrude node
/// (shape=0, structure=1, height=2, dir=3, inf=4, subdivision=5).
const int _dirParamIndex = 3;

class ExtrudeEditorState extends State<ExtrudeEditor> {
  /// True when the `dir` input pin is wired — a wired direction overrides both
  /// the mode checkbox and the stored vector, so we disable both.
  bool _isDirConnected() {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId &&
          wire.destParamIndex == BigInt.from(_dirParamIndex)) {
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

    final data = widget.data!;
    final dirConnected = _isDirConnected();

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Extrude Properties',
            nodeTypeName: 'extrude',
          ),
          const SizedBox(height: 8),
          CheckboxListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Infinite'),
            value: data.infinite,
            onChanged: (newValue) {
              if (newValue == null) return;
              widget.model.setExtrudeData(
                widget.nodeId,
                APIExtrudeData(
                  height: data.height,
                  extrudeDirection: data.extrudeDirection,
                  infinite: newValue,
                  subdivision: data.subdivision,
                  planeNormal: data.planeNormal,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Height',
            value: data.height,
            minimumValue: 1,
            onChanged: (newValue) {
              widget.model.setExtrudeData(
                widget.nodeId,
                APIExtrudeData(
                  height: newValue,
                  extrudeDirection: data.extrudeDirection,
                  infinite: data.infinite,
                  subdivision: data.subdivision,
                  planeNormal: data.planeNormal,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          if (dirConnected) ...[
            const Text(
              'Direction supplied by `dir` input. Disconnect to edit inline.',
              style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
            ),
            const SizedBox(height: 8),
          ],
          Opacity(
            opacity: dirConnected ? 0.5 : 1.0,
            child: IgnorePointer(
              ignoring: dirConnected,
              child: _buildDirectionControls(context, data),
            ),
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Subdivision',
            value: data.subdivision,
            minimumValue: 1,
            onChanged: (newValue) {
              widget.model.setExtrudeData(
                widget.nodeId,
                APIExtrudeData(
                  height: data.height,
                  extrudeDirection: data.extrudeDirection,
                  infinite: data.infinite,
                  subdivision: newValue,
                  planeNormal: data.planeNormal,
                ),
              );
            },
          ),
        ],
      ),
    );
  }

  Widget _buildDirectionControls(BuildContext context, APIExtrudeData data) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        CheckboxListTile(
          contentPadding: EdgeInsets.zero,
          title: const Text('Extrude perpendicular to plane'),
          subtitle: const Text(
            'Direction follows the drawing plane, even if it is reoriented '
            'later.',
            style: TextStyle(fontSize: 11),
          ),
          value: data.planeNormal,
          onChanged: (newValue) {
            if (newValue == null) return;
            _setPlaneNormal(data, newValue);
          },
        ),
        if (!data.planeNormal) ...[
          const SizedBox(height: 8),
          IVec3Input(
            label: 'Extrude Direction',
            value: data.extrudeDirection,
            onChanged: (newValue) {
              widget.model.setExtrudeData(
                widget.nodeId,
                APIExtrudeData(
                  height: data.height,
                  extrudeDirection: newValue,
                  infinite: data.infinite,
                  subdivision: data.subdivision,
                  planeNormal: data.planeNormal,
                ),
              );
            },
          ),
        ],
      ],
    );
  }

  /// Toggles plane-normal mode. When switching *off* (to direct mode), seed the
  /// stored direction from the drawing plane's current normal so explicit
  /// editing starts from the direction the user was just looking at, rather than
  /// a stale stored vector. Best-effort — if the plane normal is unavailable
  /// (node not selected/evaluated), keep the existing stored direction.
  void _setPlaneNormal(APIExtrudeData data, bool planeNormal) {
    var direction = data.extrudeDirection;
    if (!planeNormal) {
      final millerDir =
          structure_designer_api.getExtrudeDrawingPlaneMillerDirection(
        nodeId: widget.nodeId,
      );
      if (millerDir != null) {
        direction = millerDir;
      }
    }
    widget.model.setExtrudeData(
      widget.nodeId,
      APIExtrudeData(
        height: data.height,
        extrudeDirection: direction,
        infinite: data.infinite,
        subdivision: data.subdivision,
        planeNormal: planeNormal,
      ),
    );
  }
}
