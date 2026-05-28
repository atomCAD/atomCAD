import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for map nodes.
///
/// Phase D of function-pin unification: when `f` is wired with a starts-with-
/// compatible function source the output type is derived (read-only display);
/// when `f` is disconnected the stored `MapData.output_type` is the fallback
/// and the field is editable. Restored to the stored value automatically on
/// `f` disconnect — `MapData.output_type` is never overwritten by derivation.
/// See `doc/design_function_pin_unification.md` (Phase D).
class MapEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIMapData? data;
  final StructureDesignerModel model;

  /// The wider node view, used to read the wired-`f` derivation state for
  /// the read-only output-type display.
  final NodeView node;

  const MapEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
    required this.node,
  });

  @override
  State<MapEditor> createState() => MapEditorState();
}

class MapEditorState extends State<MapEditor> {
  @override
  void dispose() {
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final derivedFromF =
        widget.node.derivedShape?.derivedFromInputPin == 'f';

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Map Properties',
            nodeTypeName: 'map',
          ),
          const SizedBox(height: 8),

          // Input Type
          DataTypeInput(
            label: 'Input Type',
            value: widget.data!.inputType,
            onChanged: (newValue) {
              widget.model.setMApData(
                widget.nodeId,
                APIMapData(
                  inputType: newValue,
                  outputType: widget.data!.outputType,
                ),
              );
            },
          ),
          const SizedBox(height: 8),

          // Output Type — derived from `f` when wired (read-only) or
          // editable when `f` is disconnected (the stored fallback).
          if (derivedFromF)
            _DerivedOutputTypeDisplay(node: widget.node)
          else
            DataTypeInput(
              label: 'Output Type',
              value: widget.data!.outputType,
              onChanged: (newValue) {
                widget.model.setMApData(
                  widget.nodeId,
                  APIMapData(
                    inputType: widget.data!.inputType,
                    outputType: newValue,
                  ),
                );
              },
            ),
        ],
      ),
    );
  }
}

/// Read-only display of map's derived output type. The output pin's resolved
/// type is `Iter[derived]`; the stored fallback is the bare derived element,
/// so we strip the wrapping `Iter[...]` for the user-facing label.
class _DerivedOutputTypeDisplay extends StatelessWidget {
  final NodeView node;

  const _DerivedOutputTypeDisplay({required this.node});

  String _displayedType() {
    if (node.outputPins.isEmpty) return '?';
    final pin = node.outputPins.first;
    final t = pin.resolvedDataType ?? pin.dataType;
    // The pin type is `Iter[derived]`; show the element so it lines up with
    // the editable-mode "Output Type" field.
    const prefix = 'Iter[';
    if (t.startsWith(prefix) && t.endsWith(']')) {
      return t.substring(prefix.length, t.length - 1);
    }
    return t;
  }

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: 'Derived from `f`. '
          'Disconnect `f` to edit the stored fallback inline.',
      child: Container(
        padding:
            const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        decoration: BoxDecoration(
          color: Colors.white10,
          borderRadius: BorderRadius.circular(4),
          border: Border.all(color: Colors.white24),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Icon(Icons.link, size: 16, color: Colors.white54),
            const SizedBox(width: 8),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    'Output Type',
                    style: TextStyle(color: Colors.white70, fontSize: 12),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    _displayedType(),
                    style: const TextStyle(
                      color: Colors.white,
                      fontFamily: 'monospace',
                    ),
                  ),
                  const SizedBox(height: 4),
                  const Text(
                    'derived from f',
                    style: TextStyle(
                      color: Colors.white54,
                      fontStyle: FontStyle.italic,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
