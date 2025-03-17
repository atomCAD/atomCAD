import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';

/// Editor widget for half_space nodes
class HalfSpaceEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;
  final APIHalfSpaceData? data;

  const HalfSpaceEditor({
    super.key,
    required this.nodeNetworkName,
    required this.nodeId,
    required this.data,
  });

  @override
  State<HalfSpaceEditor> createState() => HalfSpaceEditorState();
}

class HalfSpaceEditorState extends State<HalfSpaceEditor> {
  APIHalfSpaceData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(HalfSpaceEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APIHalfSpaceData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setHalfSpaceData(
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
            Text('Half Space Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IVec3Input(
              label: 'Miller Index',
              value: _stagedData!.millerIndex,
              onChanged: (newValue) {
                _updateStagedData(APIHalfSpaceData(
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
                _updateStagedData(APIHalfSpaceData(
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
