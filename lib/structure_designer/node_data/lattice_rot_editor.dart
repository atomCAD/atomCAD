import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/crystal_system_display.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';

/// Editor widget for lattice_rot nodes
class LatticeRotEditor extends StatefulWidget {
  final BigInt nodeId;
  final APILatticeRotData? data;
  final StructureDesignerModel model;

  const LatticeRotEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<LatticeRotEditor> createState() => _LatticeRotEditorState();
}

class _LatticeRotEditorState extends State<LatticeRotEditor> {
  int? _selectedAxisIndex;
  int _selectedStep = 0;

  @override
  void initState() {
    super.initState();
    _initializeFromData();
  }

  @override
  void didUpdateWidget(LatticeRotEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      _initializeFromData();
    }
  }

  void _initializeFromData() {
    if (widget.data == null) return;
    
    setState(() {
      _selectedAxisIndex = widget.data!.axisIndex;
      _selectedStep = widget.data!.step;
    });
  }

  void _updateData() {
    if (widget.data == null) return;

    widget.model.setLatticeRotData(
      widget.nodeId,
      APILatticeRotData(
        axisIndex: _selectedAxisIndex,
        step: _selectedStep,
        pivotPoint: widget.data!.pivotPoint,
        rotationalSymmetries: widget.data!.rotationalSymmetries, // This will be ignored by the setter
        crystalSystem: widget.data!.crystalSystem, // Preserve existing crystal system
      ),
    );
  }

  String _formatSymmetryOption(int index, APIRotationalSymmetry symmetry) {
    final axis = symmetry.axis;
    return '$index: ${symmetry.nFold}-fold (${axis.x.toStringAsFixed(2)}, ${axis.y.toStringAsFixed(2)}, ${axis.z.toStringAsFixed(2)})';
  }

  /// Get the angle in degrees for the current step and selected axis
  double _getCurrentAngleDegrees() {
    if (_selectedAxisIndex == null || widget.data == null || _selectedStep == 0) return 0.0;
    
    final symmetries = widget.data!.rotationalSymmetries;
    if (symmetries.isEmpty) return 0.0;
    
    final safeAxisIndex = _selectedAxisIndex! % symmetries.length;
    final selectedSymmetry = symmetries[safeAxisIndex];
    
    final anglePerStep = 360.0 / selectedSymmetry.nFold;
    return (_selectedStep % selectedSymmetry.nFold) * anglePerStep;
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final hasSymmetries = widget.data!.rotationalSymmetries.isNotEmpty;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Lattice Rotation Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 16),
          
          // Crystal system display
          CrystalSystemDisplay(
            crystalSystem: widget.data!.crystalSystem,
          ),
          const SizedBox(height: 16),
          
          // Pivot point input
          IVec3Input(
            label: 'Pivot Point',
            value: widget.data!.pivotPoint,
            onChanged: (newValue) {
              widget.model.setLatticeRotData(
                widget.nodeId,
                APILatticeRotData(
                  axisIndex: _selectedAxisIndex,
                  step: _selectedStep,
                  pivotPoint: newValue,
                  rotationalSymmetries: widget.data!.rotationalSymmetries,
                  crystalSystem: widget.data!.crystalSystem,
                ),
              );
            },
          ),
          const SizedBox(height: 16),
          
          if (!hasSymmetries) ...[
            // No symmetries available
            Card(
              color: Theme.of(context).colorScheme.surfaceVariant,
              child: Padding(
                padding: const EdgeInsets.all(16.0),
                child: Column(
                  children: [
                    Icon(
                      Icons.info_outline,
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                      size: 48,
                    ),
                    const SizedBox(height: 8),
                    Text(
                      'No Rotational Symmetries Available',
                      style: Theme.of(context).textTheme.titleSmall,
                      textAlign: TextAlign.center,
                    ),
                    const SizedBox(height: 4),
                    Text(
                      'This crystal system (${widget.data!.crystalSystem}) does not have rotational symmetries. Only identity rotation is allowed.',
                      style: Theme.of(context).textTheme.bodySmall,
                      textAlign: TextAlign.center,
                    ),
                  ],
                ),
              ),
            ),
          ] else ...[
            // Rotational symmetry axis dropdown
            Text('Symmetry Axis',
                style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            DropdownButtonFormField<int?>(
              value: _selectedAxisIndex,
              decoration: const InputDecoration(
                border: OutlineInputBorder(),
                contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              ),
              items: [
                // Add "No Rotation" option
                const DropdownMenuItem<int?>(
                  value: null,
                  child: Text('No Rotation'),
                ),
                // Add all available symmetries with their indices
                ...widget.data!.rotationalSymmetries.asMap().entries.map((entry) {
                  final index = entry.key;
                  final symmetry = entry.value;
                  return DropdownMenuItem<int?>(
                    value: index,
                    child: Text(_formatSymmetryOption(index, symmetry)),
                  );
                }),
              ],
              onChanged: (int? newValue) {
                setState(() {
                  _selectedAxisIndex = newValue;
                  // Reset step to 0 when axis changes
                  _selectedStep = 0;
                });
                _updateData();
              },
            ),
            const SizedBox(height: 16),
            
            // Step input (only show if an axis is selected)
            if (_selectedAxisIndex != null) ...[
              IntInput(
                label: 'Rotation Step',
                value: _selectedStep,
                onChanged: (newValue) {
                  setState(() {
                    _selectedStep = newValue;
                  });
                  _updateData();
                },
              ),
              const SizedBox(height: 8),
              
              // Show current angle
              Card(
                color: Theme.of(context).colorScheme.surfaceVariant.withOpacity(0.5),
                child: Padding(
                  padding: const EdgeInsets.all(12.0),
                  child: Row(
                    children: [
                      Icon(
                        Icons.rotate_right,
                        color: Theme.of(context).colorScheme.onSurfaceVariant,
                        size: 20,
                      ),
                      const SizedBox(width: 8),
                      Text(
                        'Current rotation: ${_getCurrentAngleDegrees().toStringAsFixed(1)}°',
                        style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                          color: Theme.of(context).colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ],
        ],
      ),
    );
  }
}
