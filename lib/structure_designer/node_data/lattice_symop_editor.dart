import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for lattice_symop nodes
class LatticeSymopEditor extends StatefulWidget {
  final BigInt nodeId;
  final APILatticeSymopData? data;
  final StructureDesignerModel model;

  const LatticeSymopEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<LatticeSymopEditor> createState() => _LatticeSymopEditorState();
}

class _LatticeSymopEditorState extends State<LatticeSymopEditor> {
  APIRotationalSymmetry? _selectedSymmetry;
  double _selectedAngle = 0.0;

  @override
  void initState() {
    super.initState();
    _initializeFromData();
  }

  @override
  void didUpdateWidget(LatticeSymopEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      _initializeFromData();
    }
  }

  void _initializeFromData() {
    if (widget.data == null) return;
    
    setState(() {
      // Find matching symmetry from available symmetries
      if (widget.data!.rotationAxis != null && widget.data!.rotationalSymmetries.isNotEmpty) {
        final currentAxis = widget.data!.rotationAxis!;
        _selectedSymmetry = widget.data!.rotationalSymmetries.firstWhere(
          (sym) => _vectorsAreClose(sym.axis, currentAxis),
          orElse: () => widget.data!.rotationalSymmetries.first,
        );
      } else {
        // If rotation_axis is None, set _selectedSymmetry to null (represents "No Rotation")
        _selectedSymmetry = null;
      }

      // Set angle, defaulting to 0 if not in valid options for the selected symmetry
      final validAngles = _getValidAnglesForSymmetry(_selectedSymmetry);
      _selectedAngle = validAngles.contains(widget.data!.rotationAngleDegrees) 
          ? widget.data!.rotationAngleDegrees 
          : 0.0;
    });
  }

  bool _vectorsAreClose(APIVec3 a, APIVec3 b, {double tolerance = 1e-6}) {
    return (a.x - b.x).abs() < tolerance &&
           (a.y - b.y).abs() < tolerance &&
           (a.z - b.z).abs() < tolerance;
  }

  /// Calculate valid rotation angles for a given symmetry
  /// For an n-fold rotation, valid angles are: 0°, 360°/n, 2×360°/n, ..., (n-1)×360°/n
  List<double> _getValidAnglesForSymmetry(APIRotationalSymmetry? symmetry) {
    if (symmetry == null) return [0.0];
    
    final nFold = symmetry.nFold;
    if (nFold <= 0) return [0.0]; // Safety check
    
    final angleStep = 360.0 / nFold;
    
    return List.generate(nFold, (i) => i * angleStep);
  }

  void _updateData() {
    if (widget.data == null) return;

    widget.model.setLatticeSymopData(
      widget.nodeId,
      APILatticeSymopData(
        translation: widget.data!.translation,
        rotationAxis: _selectedSymmetry?.axis,
        rotationAngleDegrees: _selectedAngle,
        transformOnlyFrame: widget.data!.transformOnlyFrame,
        rotationalSymmetries: widget.data!.rotationalSymmetries, // This will be ignored by the setter
        crystalSystem: widget.data!.crystalSystem, // Preserve existing crystal system
      ),
    );
  }

  String _formatSymmetryOption(APIRotationalSymmetry symmetry) {
    final axis = symmetry.axis;
    return '${symmetry.nFold}-fold: (${axis.x.toStringAsFixed(2)}, ${axis.y.toStringAsFixed(2)}, ${axis.z.toStringAsFixed(2)})';
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Lattice Symmetry Operation Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 16),
          
          // Crystal system display
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: Colors.blue.shade50,
              border: Border.all(color: Colors.blue.shade200),
              borderRadius: BorderRadius.circular(8),
            ),
            child: Row(
              children: [
                Icon(Icons.grain, color: Colors.blue.shade700, size: 20),
                const SizedBox(width: 8),
                Text(
                  'Crystal System: ',
                  style: TextStyle(
                    fontWeight: FontWeight.w500,
                    color: Colors.blue.shade700,
                  ),
                ),
                Text(
                  widget.data!.crystalSystem,
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    color: Colors.blue.shade800,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 16),
          
          // Translation input
          IVec3Input(
            label: 'Translation',
            value: widget.data!.translation,
            onChanged: (newValue) {
              widget.model.setLatticeSymopData(
                widget.nodeId,
                APILatticeSymopData(
                  translation: newValue,
                  rotationAxis: _selectedSymmetry?.axis,
                  rotationAngleDegrees: _selectedAngle,
                  transformOnlyFrame: widget.data!.transformOnlyFrame,
                  rotationalSymmetries: widget.data!.rotationalSymmetries,
                  crystalSystem: widget.data!.crystalSystem,
                ),
              );
            },
          ),
          const SizedBox(height: 16),
          
          // Rotational symmetry dropdown
          Text('Rotational Symmetry Axis',
              style: Theme.of(context).textTheme.titleSmall),
          const SizedBox(height: 8),
          DropdownButtonFormField<APIRotationalSymmetry?>(
            value: _selectedSymmetry,
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            ),
            items: [
              // Add "No Rotation" option
              const DropdownMenuItem<APIRotationalSymmetry?>(
                value: null,
                child: Text('No Rotation'),
              ),
              // Add all available symmetries
              ...widget.data!.rotationalSymmetries.map((symmetry) {
                return DropdownMenuItem<APIRotationalSymmetry?>(
                  value: symmetry,
                  child: Text(_formatSymmetryOption(symmetry)),
                );
              }),
            ],
            onChanged: (APIRotationalSymmetry? newValue) {
              setState(() {
                _selectedSymmetry = newValue;
                // Reset angle to 0 when symmetry changes
                _selectedAngle = 0.0;
              });
              _updateData();
            },
          ),
          const SizedBox(height: 16),
          
          // Rotation angle dropdown
          Text('Rotation Angle (degrees)',
              style: Theme.of(context).textTheme.titleSmall),
          const SizedBox(height: 8),
          DropdownButtonFormField<double>(
            value: _getValidAnglesForSymmetry(_selectedSymmetry).contains(_selectedAngle) 
                ? _selectedAngle 
                : _getValidAnglesForSymmetry(_selectedSymmetry).first,
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            ),
            items: _getValidAnglesForSymmetry(_selectedSymmetry).map((angle) {
              return DropdownMenuItem<double>(
                value: angle,
                child: Text('${angle.toStringAsFixed(1)}°'),
              );
            }).toList(),
            onChanged: (double? newValue) {
              if (newValue != null) {
                setState(() {
                  _selectedAngle = newValue;
                });
                _updateData();
              }
            },
          ),
          const SizedBox(height: 16),
          
          // Transform only frame checkbox
          CheckboxListTile(
            title: const Text('Transform Only Frame'),
            subtitle: const Text('If checked, only the reference frame is transformed, the geometry remains in place'),
            value: widget.data!.transformOnlyFrame,
            onChanged: (bool? value) {
              if (value != null) {
                widget.model.setLatticeSymopData(
                  widget.nodeId,
                  APILatticeSymopData(
                    translation: widget.data!.translation,
                    rotationAxis: _selectedSymmetry?.axis,
                    rotationAngleDegrees: _selectedAngle,
                    transformOnlyFrame: value,
                    rotationalSymmetries: widget.data!.rotationalSymmetries,
                    crystalSystem: widget.data!.crystalSystem,
                  ),
                );
              }
            },
          ),
        ],
      ),
    );
  }
}
