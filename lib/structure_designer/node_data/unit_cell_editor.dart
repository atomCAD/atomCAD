import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
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
  bool _useCrystallographicConvention = true;

  /// Convert from crystallographic convention to atomCAD convention
  /// Crystallography: a,b,c → atomCAD: b,c,a
  /// Crystallography: α,β,γ → atomCAD: β,γ,α
  APIUnitCellData _crystallographicToAtomCAD(APIUnitCellData crystData) {
    return APIUnitCellData(
      cellLengthA: crystData.cellLengthB, // a_atomcad = b_cryst
      cellLengthB: crystData.cellLengthC, // b_atomcad = c_cryst
      cellLengthC: crystData.cellLengthA, // c_atomcad = a_cryst
      cellAngleAlpha: crystData.cellAngleBeta, // α_atomcad = β_cryst
      cellAngleBeta: crystData.cellAngleGamma, // β_atomcad = γ_cryst
      cellAngleGamma: crystData.cellAngleAlpha, // γ_atomcad = α_cryst
      crystalSystem: crystData.crystalSystem, // Preserve crystal system
    );
  }

  /// Convert from atomCAD convention to crystallographic convention
  /// atomCAD: a,b,c → Crystallography: c,a,b
  /// atomCAD: α,β,γ → Crystallography: γ,α,β
  APIUnitCellData _atomCADToCrystallographic(APIUnitCellData atomcadData) {
    return APIUnitCellData(
      cellLengthA: atomcadData.cellLengthC, // a_cryst = c_atomcad
      cellLengthB: atomcadData.cellLengthA, // b_cryst = a_atomcad
      cellLengthC: atomcadData.cellLengthB, // c_cryst = b_atomcad
      cellAngleAlpha: atomcadData.cellAngleGamma, // α_cryst = γ_atomcad
      cellAngleBeta: atomcadData.cellAngleAlpha, // β_cryst = α_atomcad
      cellAngleGamma: atomcadData.cellAngleBeta, // γ_cryst = β_atomcad
      crystalSystem: atomcadData.crystalSystem, // Preserve crystal system
    );
  }

  /// Get the data to display in UI (converted if needed)
  APIUnitCellData _getDisplayData() {
    if (_useCrystallographicConvention) {
      return _atomCADToCrystallographic(widget.data!);
    }
    return widget.data!;
  }

  /// Update the backend with converted data if needed
  void _updateUnitCellData(APIUnitCellData displayData) {
    APIUnitCellData backendData;
    if (_useCrystallographicConvention) {
      backendData = _crystallographicToAtomCAD(displayData);
    } else {
      backendData = displayData;
    }
    widget.model.setUnitCellData(widget.nodeId, backendData);
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
            Text('Unit Cell Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),

            // Coordinate system convention checkbox
            Row(
              children: [
                Checkbox(
                  value: _useCrystallographicConvention,
                  onChanged: (value) {
                    setState(() {
                      _useCrystallographicConvention = value ?? true;
                    });
                  },
                ),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Crystallographic convention',
                        style: Theme.of(context).textTheme.bodySmall,
                      ),
                      Text(
                        'The canonical crystallography cs. is Y right, Z up, vs. atomCAD X right, Y up (both right handed). atomCAD(a,b,c) = crystallography(b,c,a), atomCAD(alpha,beta,gamma)=crystallography(beta,gamma,alpha). See user\'s guide',
                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                              fontSize: 10,
                              color: Theme.of(context)
                                  .textTheme
                                  .bodySmall
                                  ?.color
                                  ?.withOpacity(0.7),
                            ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 16),

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
