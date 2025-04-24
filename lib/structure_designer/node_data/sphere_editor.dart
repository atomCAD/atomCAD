import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';

/// Editor widget for sphere nodes
class SphereEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;
  final APISphereData? data;

  const SphereEditor({
    super.key,
    required this.nodeNetworkName,
    required this.nodeId,
    required this.data,
  });

  @override
  State<SphereEditor> createState() => SphereEditorState();
}

class SphereEditorState extends State<SphereEditor> {
  APISphereData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(SphereEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APISphereData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setSphereData(
        nodeNetworkName: widget.nodeNetworkName,
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
            Text('Sphere Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IVec3Input(
              label: 'Center',
              value: _stagedData!.center,
              onChanged: (newValue) {
                _updateStagedData(APISphereData(
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
                _updateStagedData(APISphereData(
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
