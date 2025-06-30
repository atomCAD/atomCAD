import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/inputs/int_input.dart';

/// Editor widget for polygon nodes
class RegPolyEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIRegPolyData? data;

  const RegPolyEditor({
    super.key,
    required this.nodeId,
    required this.data,
  });

  @override
  State<RegPolyEditor> createState() => RegPolyEditorState();
}

class RegPolyEditorState extends State<RegPolyEditor> {
  APIRegPolyData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(RegPolyEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APIRegPolyData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setRegPolyData(
        nodeId: widget.nodeId,
        data: _stagedData!,
      );
      // No need to update _data here as it will be updated in the parent widget
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_stagedData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Polygon Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IntInput(
              label: 'Number of Sides',
              value: _stagedData!.numSides,
              onChanged: (newValue) {
                // Ensure at least 3 sides for a valid polygon
                final validSides = newValue < 3 ? 3 : newValue;
                _updateStagedData(APIRegPolyData(
                  numSides: validSides,
                  radius: _stagedData!.radius,
                ));
              },
            ),
            const SizedBox(height: 8),
            IntInput(
              label: 'Radius',
              value: _stagedData!.radius,
              onChanged: (newValue) {
                // Ensure radius is at least 1
                final validRadius = newValue < 1 ? 1 : newValue;
                _updateStagedData(APIRegPolyData(
                  numSides: _stagedData!.numSides,
                  radius: validRadius,
                ));
              },
            ),
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _stagedData != widget.data
                      ? () {
                          setState(() => _stagedData = widget.data);
                        }
                      : null,
                  child: const Text('Reset'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _stagedData != widget.data ? _applyChanges : null,
                  child: const Text('Apply'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
