import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';

/// Editor widget for circle nodes
class CircleEditor extends StatefulWidget {
  final BigInt nodeId;
  final APICircleData? data;

  const CircleEditor({
    super.key,
    required this.nodeId,
    required this.data,
  });

  @override
  State<CircleEditor> createState() => CircleEditorState();
}

class CircleEditorState extends State<CircleEditor> {
  APICircleData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(CircleEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APICircleData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setCircleData(
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
            Text('Circle Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IVec2Input(
              label: 'Center',
              value: _stagedData!.center,
              onChanged: (newValue) {
                _updateStagedData(APICircleData(
                  center: newValue,
                  radius: _stagedData!.radius,
                ));
              },
            ),
            const SizedBox(height: 8),
            IntInput(
              label: 'Radius',
              value: _stagedData!.radius,
              onChanged: (newValue) {
                _updateStagedData(APICircleData(
                  center: _stagedData!.center,
                  radius: newValue,
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
