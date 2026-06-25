import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/miller_index_map.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Pin indices on the `drawing_plane` node. Mirror
/// `rust/src/structure_designer/nodes/drawing_plane.rs::get_node_type`
/// (structure 0, m_index 1, center 2, shift 3, subdivision 4, u 5, v 6).
const int M_INDEX_PIN_INDEX = 1;
const int U_PIN_INDEX = 5;
const int V_PIN_INDEX = 6;

/// Defaults seeded into the orientation fields when their checkbox is first
/// toggled on. `m` keeps the historical (001) default; `u`/`v` are the natural
/// in-plane axes for that plane.
const APIIVec3 _DEFAULT_MILLER = APIIVec3(x: 0, y: 0, z: 1);
const APIIVec3 _DEFAULT_U = APIIVec3(x: 1, y: 0, z: 0);
const APIIVec3 _DEFAULT_V = APIIVec3(x: 0, y: 1, z: 0);

/// Editor widget for drawing_plane nodes.
///
/// The three orientation inputs (`m_index`, `u`, `v`) are all optional and
/// follow the wired-pin > stored-field > unset resolution. Each is rendered
/// with a checkbox toggle (unchecked = unset) and disabled when its input pin
/// is wired (the wired value overrides the stored one at eval). See
/// `doc/design_drawing_plane_explicit_axes.md`.
class DrawingPlaneEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIDrawingPlaneData? data;
  final StructureDesignerModel model;

  const DrawingPlaneEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<DrawingPlaneEditor> createState() => DrawingPlaneEditorState();
}

class DrawingPlaneEditorState extends State<DrawingPlaneEditor> {
  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final data = widget.data!;
    final mPinConnected = _isPinConnected(M_INDEX_PIN_INDEX);
    final uPinConnected = _isPinConnected(U_PIN_INDEX);
    final vPinConnected = _isPinConnected(V_PIN_INDEX);

    return Padding(
      padding: const EdgeInsets.all(4.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Drawing Plane Properties',
            nodeTypeName: 'drawing_plane',
          ),
          const SizedBox(height: 8),
          // Max Miller Index input
          IntInput(
            label: 'Max Miller Index',
            value: data.maxMillerIndex,
            minimumValue: 1, // Must be at least 1
            maximumValue: 10, // Set a reasonable upper limit
            onChanged: (newValue) {
              _updateData(maxMillerIndex: newValue);
            },
          ),
          const SizedBox(height: 12),
          _buildMillerSection(data, mPinConnected),
          const SizedBox(height: 12),
          _buildAxisSection(
            label: 'First in-plane axis (u)',
            value: data.uAxis,
            pinConnected: uPinConnected,
            pinName: 'u',
            defaultValue: _DEFAULT_U,
            onChanged: (newValue) => _updateData(uAxis: () => newValue),
          ),
          const SizedBox(height: 12),
          _buildAxisSection(
            label: 'Second in-plane axis (v)',
            value: data.vAxis,
            pinConnected: vPinConnected,
            pinName: 'v',
            defaultValue: _DEFAULT_V,
            onChanged: (newValue) => _updateData(vAxis: () => newValue),
          ),
          const SizedBox(height: 12),
          IVec3Input(
            label: 'Center',
            value: data.center,
            onChanged: (newValue) {
              _updateData(center: newValue);
            },
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Shift',
            value: data.shift,
            onChanged: (newValue) {
              _updateData(shift: newValue);
            },
          ),
          const SizedBox(height: 8),
          // Subdivision input
          IntInput(
            label: 'Subdivision',
            value: data.subdivision,
            minimumValue: 1,
            onChanged: (newValue) {
              _updateData(subdivision: newValue);
            },
          ),
        ],
      ),
    );
  }

  /// The Miller index section: a checkbox toggling the stored index on/off
  /// (off = case D, where `m` is derived from `u`/`v`). When set, the
  /// visual map + numeric input are shown. When unset, the resolved (derived)
  /// index is shown read-only if available from the last evaluation.
  Widget _buildMillerSection(APIDrawingPlaneData data, bool pinConnected) {
    final miller = data.millerIndex;
    final maxMiller = data.maxMillerIndex;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (pinConnected)
          const Padding(
            padding: EdgeInsets.only(bottom: 4),
            child: Text(
              'Miller index supplied by `m_index` input. Disconnect to edit inline.',
              style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
            ),
          ),
        Opacity(
          opacity: pinConnected ? 0.5 : 1.0,
          child: IgnorePointer(
            ignoring: pinConnected,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Checkbox(
                      value: miller != null,
                      onChanged: (checked) =>
                          _onMillerCheckboxChanged(checked ?? false),
                    ),
                    const Text('Set Miller index (m)'),
                  ],
                ),
                if (miller != null) ...[
                  Padding(
                    padding: const EdgeInsets.only(left: 8, top: 4),
                    child: MillerIndexMap(
                      label: 'Miller Index Map',
                      value: miller,
                      onChanged: (newValue) =>
                          _updateData(millerIndex: () => newValue),
                      maxValue: maxMiller,
                      mapWidth: 360,
                      mapHeight: 180,
                      dotColor: Theme.of(context).brightness == Brightness.dark
                          ? Colors.grey.shade600
                          : Colors.grey.shade400,
                      selectedDotColor: Colors.red,
                    ),
                  ),
                  const SizedBox(height: 8),
                  Padding(
                    padding: const EdgeInsets.only(left: 8),
                    child: IVec3Input(
                      label: 'Miller Index (numeric)',
                      value: miller,
                      minimumValue: APIIVec3(
                          x: -maxMiller, y: -maxMiller, z: -maxMiller),
                      maximumValue:
                          APIIVec3(x: maxMiller, y: maxMiller, z: maxMiller),
                      onChanged: (newValue) =>
                          _updateData(millerIndex: () => newValue),
                    ),
                  ),
                ] else
                  Padding(
                    padding: const EdgeInsets.only(left: 32, bottom: 4),
                    child: _buildDerivedMillerLabel(data.resolvedMillerIndex),
                  ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  /// Read-only display of the derived Miller index (case D) from the last
  /// evaluation, or a placeholder when not yet available.
  Widget _buildDerivedMillerLabel(APIIVec3? resolved) {
    final text = resolved != null
        ? 'Derived from u × v: (${resolved.x}, ${resolved.y}, ${resolved.z})'
        : 'Derived from u × v (not yet evaluated)';
    return Text(
      text,
      style: const TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
    );
  }

  /// An optional in-plane axis (`u` or `v`): a checkbox toggling the stored
  /// value on/off, with an `IVec3Input` shown when set. Disabled when the
  /// matching input pin is wired.
  Widget _buildAxisSection({
    required String label,
    required APIIVec3? value,
    required bool pinConnected,
    required String pinName,
    required APIIVec3 defaultValue,
    required void Function(APIIVec3?) onChanged,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (pinConnected)
          Padding(
            padding: const EdgeInsets.only(bottom: 4),
            child: Text(
              'Axis supplied by `$pinName` input. Disconnect to edit inline.',
              style: const TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
            ),
          ),
        Opacity(
          opacity: pinConnected ? 0.5 : 1.0,
          child: IgnorePointer(
            ignoring: pinConnected,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Checkbox(
                      value: value != null,
                      onChanged: (checked) =>
                          onChanged((checked ?? false) ? defaultValue : null),
                    ),
                    Text(label),
                  ],
                ),
                if (value != null)
                  Padding(
                    padding: const EdgeInsets.only(left: 8),
                    child: IVec3Input(
                      label: 'Direction [u v w]',
                      value: value,
                      onChanged: (newValue) => onChanged(newValue),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  void _onMillerCheckboxChanged(bool checked) {
    _updateData(millerIndex: () => checked ? _DEFAULT_MILLER : null);
  }

  /// Single-channel update path. The nullable orientation fields
  /// (`millerIndex`, `uAxis`, `vAxis`) are wrapped in thunks so callers can
  /// pass `() => null` to clear them without colliding with "field not
  /// provided". `resolvedMillerIndex` is read-only and never written back.
  void _updateData({
    int? maxMillerIndex,
    APIIVec3? Function()? millerIndex,
    APIIVec3? center,
    int? shift,
    int? subdivision,
    APIIVec3? Function()? uAxis,
    APIIVec3? Function()? vAxis,
  }) {
    final data = widget.data!;
    widget.model.setDrawingPlaneData(
      widget.nodeId,
      APIDrawingPlaneData(
        maxMillerIndex: maxMillerIndex ?? data.maxMillerIndex,
        millerIndex: millerIndex != null ? millerIndex() : data.millerIndex,
        center: center ?? data.center,
        shift: shift ?? data.shift,
        subdivision: subdivision ?? data.subdivision,
        uAxis: uAxis != null ? uAxis() : data.uAxis,
        vAxis: vAxis != null ? vAxis() : data.vAxis,
        resolvedMillerIndex: data.resolvedMillerIndex,
      ),
    );
  }

  bool _isPinConnected(int pinIndex) {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    final target = BigInt.from(pinIndex);
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId && wire.destParamIndex == target) {
        return true;
      }
    }
    return false;
  }
}
