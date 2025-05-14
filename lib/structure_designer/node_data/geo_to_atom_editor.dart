import 'package:flutter/material.dart';
import 'package:flutter_cad/common/select_element_widget.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/select_crystal_type_widget.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';

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
                  ));
                }
              },
              label: 'Secondary Element:',
              hint: 'Select secondary element',
              required: true,
            ),
          ],
        ),
      ),
    );
  }
}
