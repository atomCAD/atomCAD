import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/common/crystal_system_display.dart';

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
  /// Get the data to display in UI
  APIUnitCellData _getDisplayData() {
    return widget.data!;
  }

  /// Update the backend with data
  void _updateUnitCellData(APIUnitCellData data) {
    widget.model.setUnitCellData(widget.nodeId, data);
  }

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
            const NodeEditorHeader(
            title: 'Unit Cell Properties',
            nodeTypeName: 'unit_cell',
          ),
            const SizedBox(height: 8),


            // Cell Lengths Section
            Text('Cell Lengths (Å)',
                style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Length a',
              value: _getDisplayData().cellLengthA,
              onChanged: (newValue) {
                final displayData = _getDisplayData();
                _updateUnitCellData(APIUnitCellData(
                  cellLengthA: newValue,
                  cellLengthB: displayData.cellLengthB,
                  cellLengthC: displayData.cellLengthC,
                  cellAngleAlpha: displayData.cellAngleAlpha,
                  cellAngleBeta: displayData.cellAngleBeta,
                  cellAngleGamma: displayData.cellAngleGamma,
                  crystalSystem: displayData.crystalSystem,
                ));
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Length b',
              value: _getDisplayData().cellLengthB,
              onChanged: (newValue) {
                final displayData = _getDisplayData();
                _updateUnitCellData(APIUnitCellData(
                  cellLengthA: displayData.cellLengthA,
                  cellLengthB: newValue,
                  cellLengthC: displayData.cellLengthC,
                  cellAngleAlpha: displayData.cellAngleAlpha,
                  cellAngleBeta: displayData.cellAngleBeta,
                  cellAngleGamma: displayData.cellAngleGamma,
                  crystalSystem: displayData.crystalSystem,
                ));
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Length c',
              value: _getDisplayData().cellLengthC,
              onChanged: (newValue) {
                final displayData = _getDisplayData();
                _updateUnitCellData(APIUnitCellData(
                  cellLengthA: displayData.cellLengthA,
                  cellLengthB: displayData.cellLengthB,
                  cellLengthC: newValue,
                  cellAngleAlpha: displayData.cellAngleAlpha,
                  cellAngleBeta: displayData.cellAngleBeta,
                  cellAngleGamma: displayData.cellAngleGamma,
                  crystalSystem: displayData.crystalSystem,
                ));
              },
            ),
            const SizedBox(height: 16),

            // Cell Angles Section
            Text('Cell Angles (°)',
                style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Angle α (alpha)',
              value: _getDisplayData().cellAngleAlpha,
              onChanged: (newValue) {
                final displayData = _getDisplayData();
                _updateUnitCellData(APIUnitCellData(
                  cellLengthA: displayData.cellLengthA,
                  cellLengthB: displayData.cellLengthB,
                  cellLengthC: displayData.cellLengthC,
                  cellAngleAlpha: newValue,
                  cellAngleBeta: displayData.cellAngleBeta,
                  cellAngleGamma: displayData.cellAngleGamma,
                  crystalSystem: displayData.crystalSystem,
                ));
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Angle β (beta)',
              value: _getDisplayData().cellAngleBeta,
              onChanged: (newValue) {
                final displayData = _getDisplayData();
                _updateUnitCellData(APIUnitCellData(
                  cellLengthA: displayData.cellLengthA,
                  cellLengthB: displayData.cellLengthB,
                  cellLengthC: displayData.cellLengthC,
                  cellAngleAlpha: displayData.cellAngleAlpha,
                  cellAngleBeta: newValue,
                  cellAngleGamma: displayData.cellAngleGamma,
                  crystalSystem: displayData.crystalSystem,
                ));
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Angle γ (gamma)',
              value: _getDisplayData().cellAngleGamma,
              onChanged: (newValue) {
                final displayData = _getDisplayData();
                _updateUnitCellData(APIUnitCellData(
                  cellLengthA: displayData.cellLengthA,
                  cellLengthB: displayData.cellLengthB,
                  cellLengthC: displayData.cellLengthC,
                  cellAngleAlpha: displayData.cellAngleAlpha,
                  cellAngleBeta: displayData.cellAngleBeta,
                  cellAngleGamma: newValue,
                  crystalSystem: displayData.crystalSystem,
                ));
              },
            ),
            const SizedBox(height: 16),
            
            // Crystal system display
            CrystalSystemDisplay(
              crystalSystem: widget.data!.crystalSystem,
              label: 'Detected Crystal System: ',
            ),
          ],
        ),
      ),
    );
  }
}
