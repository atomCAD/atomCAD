import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/derived_output_type_display.dart';

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

    final derivedFromF = widget.node.derivedShape?.derivedFromInputPin == 'f';

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
            DerivedOutputTypeDisplay(node: widget.node)
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
