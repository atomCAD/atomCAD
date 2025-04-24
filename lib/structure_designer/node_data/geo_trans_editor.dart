import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';

/// Editor widget for geo_trans nodes
class GeoTransEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;
  final APIGeoTransData? data;

  const GeoTransEditor({
    super.key,
    required this.nodeNetworkName,
    required this.nodeId,
    required this.data,
  });

  @override
  State<GeoTransEditor> createState() => GeoTransEditorState();
}

class GeoTransEditorState extends State<GeoTransEditor> {
  APIGeoTransData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(GeoTransEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APIGeoTransData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setGeoTransData(
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
            Text('Geo Transformation Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IVec3Input(
              label: 'Translation',
              value: _stagedData!.translation,
              onChanged: (newValue) {
                _updateStagedData(APIGeoTransData(
                  transformOnlyFrame: _stagedData!.transformOnlyFrame,
                  translation: newValue,
                  rotation: _stagedData!.rotation,
                ));
              },
            ),
            const SizedBox(height: 8),
            IVec3Input(
              label: 'Rotation',
              value: _stagedData!.rotation,
              onChanged: (newValue) {
                _updateStagedData(APIGeoTransData(
                  transformOnlyFrame: _stagedData!.transformOnlyFrame,
                  translation: _stagedData!.translation,
                  rotation: newValue,
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
