import 'package:flutter/material.dart';
import 'package:flutter_cad/common/select_element_widget.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/select_crystal_type_widget.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/geo_to_atom_api.dart';

/// Editor widget for GeoToAtom nodes that allows configuring the crystal structure
class GeoToAtomEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIGeoToAtomData? data;

  const GeoToAtomEditor({
    super.key,
    required this.nodeId,
    this.data,
  });

  @override
  State<GeoToAtomEditor> createState() => _GeoToAtomEditorState();
}

class _GeoToAtomEditorState extends State<GeoToAtomEditor> {
  APIGeoToAtomData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(GeoToAtomEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APIGeoToAtomData newData) {
    setState(() => _stagedData = newData);
    _applyChanges();
  }

  void _applyChanges() {
    if (_stagedData != null) {
      // Call the API function to save data back to Rust
      setGeoToAtomData(
        nodeId: widget.nodeId,
        data: _stagedData!,
      );
    }
  }

  /// Builds a widget that displays the unit cell size and whether it's estimated
  Widget _buildUnitCellSizeDisplay() {
    // Get the primary and secondary atomic numbers
    final primary = _stagedData!.primaryAtomicNumber;
    final secondary = _stagedData!.secondaryAtomicNumber;
    
    // Skip if either is zero (Custom)
    if (primary == 0 || secondary == 0) {
      return const SizedBox.shrink();
    }
    
    // Get the unit cell size from the API
    final unitCellSize = getUnitCellSize(
      primaryAtomicNumber: primary,
      secondaryAtomicNumber: secondary,
    );
    
    // Check if it's estimated
    final isEstimated = isUnitCellSizeEstimated(
      primaryAtomicNumber: primary,
      secondaryAtomicNumber: secondary,
    );
    
    // Format the unit cell size with 3 decimal places
    final formattedSize = unitCellSize.toStringAsFixed(3);
    
    // Build the display text
    final displayText = 'Unit cell size: $formattedSize Å${isEstimated ? ' (estimated)' : ''}';
    
    return Text(
      displayText,
      style: TextStyle(
        fontSize: 14,
        fontWeight: isEstimated ? FontWeight.normal : FontWeight.w500,
        fontStyle: isEstimated ? FontStyle.italic : FontStyle.normal,
        color: isEstimated ? Colors.grey[700] : Colors.black,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.medium),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Crystal Structure',
              style: TextStyle(fontWeight: FontWeight.w500),
            ),
            const SizedBox(height: AppSpacing.medium),

            // Crystal type selection
            SelectCrystalTypeWidget(
              primaryAtomicNumber: _stagedData?.primaryAtomicNumber,
              secondaryAtomicNumber: _stagedData?.secondaryAtomicNumber,
              onChanged: (primary, secondary) {
                // When a crystal type is selected, update both atomic numbers
                if (_stagedData != null) {
                  _updateStagedData(APIGeoToAtomData(
                    primaryAtomicNumber: primary,
                    secondaryAtomicNumber: secondary,
                    hydrogenPassivation: _stagedData!.hydrogenPassivation,
                  ));
                }
              },
              label: 'Crystal Type:',
              hint: 'Select a crystal structure',
            ),

            const SizedBox(height: AppSpacing.medium),
            const Divider(),
            const SizedBox(height: AppSpacing.small),

            Text(
              'Custom Elements',
              style: TextStyle(fontWeight: FontWeight.w500),
            ),
            const SizedBox(height: AppSpacing.medium),

            // Primary atom selection
            SelectElementWidget(
              value: _stagedData?.primaryAtomicNumber,
              onChanged: (int? newValue) {
                if (newValue != null && _stagedData != null) {
                  _updateStagedData(APIGeoToAtomData(
                    primaryAtomicNumber: newValue,
                    secondaryAtomicNumber: _stagedData!.secondaryAtomicNumber,
                    hydrogenPassivation: _stagedData!.hydrogenPassivation,
                  ));
                }
              },
              label: 'Primary Element:',
              hint: 'Select primary element',
              required: true,
            ),

            const SizedBox(height: AppSpacing.medium),

            // Secondary atom selection
            SelectElementWidget(
              value: _stagedData?.secondaryAtomicNumber,
              onChanged: (int? newValue) {
                if (newValue != null && _stagedData != null) {
                  _updateStagedData(APIGeoToAtomData(
                    primaryAtomicNumber: _stagedData!.primaryAtomicNumber,
                    secondaryAtomicNumber: newValue,
                    hydrogenPassivation: _stagedData!.hydrogenPassivation,
                  ));
                }
              },
              label: 'Secondary Element:',
              hint: 'Select secondary element',
              required: true,
            ),
            
            const SizedBox(height: AppSpacing.medium),
            
            // Unit cell size display
            if (_stagedData?.primaryAtomicNumber != null && 
                _stagedData?.secondaryAtomicNumber != null) 
              _buildUnitCellSizeDisplay(),
            
            const SizedBox(height: AppSpacing.medium),
            const Divider(),
            const SizedBox(height: AppSpacing.small),
            
            Text(
              'Options',
              style: TextStyle(fontWeight: FontWeight.w500),
            ),
            const SizedBox(height: AppSpacing.medium),
            
            // Hydrogen passivation checkbox
            CheckboxListTile(
              title: Text('Hydrogen Passivation'),
              value: _stagedData?.hydrogenPassivation ?? false,
              onChanged: (bool? value) {
                if (value != null && _stagedData != null) {
                  _updateStagedData(APIGeoToAtomData(
                    primaryAtomicNumber: _stagedData!.primaryAtomicNumber,
                    secondaryAtomicNumber: _stagedData!.secondaryAtomicNumber,
                    hydrogenPassivation: value,
                  ));
                }
              },
              controlAffinity: ListTileControlAffinity.leading,
              contentPadding: EdgeInsets.zero,
            ),
          ],
        ),
      ),
    );
  }
}
