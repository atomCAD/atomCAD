import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for unit_cell nodes
class UnitCellEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIUnitCellData? data;
  final StructureDesignerModel model;

  const UnitCellEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<UnitCellEditor> createState() => UnitCellEditorState();
}

class UnitCellEditorState extends State<UnitCellEditor> {
  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Unit Cell Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 16),
            
            // Cell Lengths Section
            Text('Cell Lengths (Å)',
                style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Length a',
              value: widget.data!.cellLengthA,
              onChanged: (newValue) {
                widget.model.setUnitCellData(
                  widget.nodeId,
                  APIUnitCellData(
                    cellLengthA: newValue,
                    cellLengthB: widget.data!.cellLengthB,
                    cellLengthC: widget.data!.cellLengthC,
                    cellAngleAlpha: widget.data!.cellAngleAlpha,
                    cellAngleBeta: widget.data!.cellAngleBeta,
                    cellAngleGamma: widget.data!.cellAngleGamma,
                  ),
                );
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Length b',
              value: widget.data!.cellLengthB,
              onChanged: (newValue) {
                widget.model.setUnitCellData(
                  widget.nodeId,
                  APIUnitCellData(
                    cellLengthA: widget.data!.cellLengthA,
                    cellLengthB: newValue,
                    cellLengthC: widget.data!.cellLengthC,
                    cellAngleAlpha: widget.data!.cellAngleAlpha,
                    cellAngleBeta: widget.data!.cellAngleBeta,
                    cellAngleGamma: widget.data!.cellAngleGamma,
                  ),
                );
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Length c',
              value: widget.data!.cellLengthC,
              onChanged: (newValue) {
                widget.model.setUnitCellData(
                  widget.nodeId,
                  APIUnitCellData(
                    cellLengthA: widget.data!.cellLengthA,
                    cellLengthB: widget.data!.cellLengthB,
                    cellLengthC: newValue,
                    cellAngleAlpha: widget.data!.cellAngleAlpha,
                    cellAngleBeta: widget.data!.cellAngleBeta,
                    cellAngleGamma: widget.data!.cellAngleGamma,
                  ),
                );
              },
            ),
            const SizedBox(height: 16),
            
            // Cell Angles Section
            Text('Cell Angles (°)',
                style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Angle α (alpha)',
              value: widget.data!.cellAngleAlpha,
              onChanged: (newValue) {
                widget.model.setUnitCellData(
                  widget.nodeId,
                  APIUnitCellData(
                    cellLengthA: widget.data!.cellLengthA,
                    cellLengthB: widget.data!.cellLengthB,
                    cellLengthC: widget.data!.cellLengthC,
                    cellAngleAlpha: newValue,
                    cellAngleBeta: widget.data!.cellAngleBeta,
                    cellAngleGamma: widget.data!.cellAngleGamma,
                  ),
                );
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Angle β (beta)',
              value: widget.data!.cellAngleBeta,
              onChanged: (newValue) {
                widget.model.setUnitCellData(
                  widget.nodeId,
                  APIUnitCellData(
                    cellLengthA: widget.data!.cellLengthA,
                    cellLengthB: widget.data!.cellLengthB,
                    cellLengthC: widget.data!.cellLengthC,
                    cellAngleAlpha: widget.data!.cellAngleAlpha,
                    cellAngleBeta: newValue,
                    cellAngleGamma: widget.data!.cellAngleGamma,
                  ),
                );
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Angle γ (gamma)',
              value: widget.data!.cellAngleGamma,
              onChanged: (newValue) {
                widget.model.setUnitCellData(
                  widget.nodeId,
                  APIUnitCellData(
                    cellLengthA: widget.data!.cellLengthA,
                    cellLengthB: widget.data!.cellLengthB,
                    cellLengthC: widget.data!.cellLengthC,
                    cellAngleAlpha: widget.data!.cellAngleAlpha,
                    cellAngleBeta: widget.data!.cellAngleBeta,
                    cellAngleGamma: newValue,
                  ),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
