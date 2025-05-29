import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';

/// Editor widget for rectangle nodes
class RectEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIRectData? data;

  const RectEditor({
    super.key,
    required this.nodeId,
    required this.data,
  });

  @override
  State<RectEditor> createState() => RectEditorState();
}

class RectEditorState extends State<RectEditor> {
  APIRectData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(RectEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APIRectData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setRectData(
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
            Text('Rectangle Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IVec2Input(
              label: 'Min Corner',
              value: _stagedData!.minCorner,
              onChanged: (newValue) {
                _updateStagedData(APIRectData(
                  minCorner: newValue,
                  extent: _stagedData!.extent,
                ));
              },
            ),
            const SizedBox(height: 8),
            IVec2Input(
              label: 'Extent',
              value: _stagedData!.extent,
              onChanged: (newValue) {
                _updateStagedData(APIRectData(
                  minCorner: _stagedData!.minCorner,
                  extent: newValue,
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
