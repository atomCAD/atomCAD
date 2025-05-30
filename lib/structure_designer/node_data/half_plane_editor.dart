import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';

/// Editor widget for half_plane nodes
class HalfPlaneEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIHalfPlaneData? data;

  const HalfPlaneEditor({
    super.key,
    required this.nodeId,
    required this.data,
  });

  @override
  State<HalfPlaneEditor> createState() => HalfPlaneEditorState();
}

class HalfPlaneEditorState extends State<HalfPlaneEditor> {
  APIHalfPlaneData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(HalfPlaneEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APIHalfPlaneData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setHalfPlaneData(
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
            Text('Half Plane Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IVec2Input(
              label: 'Miller Index',
              value: _stagedData!.millerIndex,
              onChanged: (newValue) {
                _updateStagedData(APIHalfPlaneData(
                  millerIndex: newValue,
                  shift: _stagedData!.shift,
                ));
              },
            ),
            const SizedBox(height: 8),
            IntInput(
              label: 'Shift',
              value: _stagedData!.shift,
              onChanged: (newValue) {
                _updateStagedData(APIHalfPlaneData(
                  millerIndex: _stagedData!.millerIndex,
                  shift: newValue,
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
