import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';

/// Editor widget for atom_trans nodes
class AtomTransEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIAtomTransData? data;

  const AtomTransEditor({
    super.key,
    required this.nodeId,
    required this.data,
  });

  @override
  State<AtomTransEditor> createState() => AtomTransEditorState();
}

class AtomTransEditorState extends State<AtomTransEditor> {
  APIAtomTransData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(AtomTransEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APIAtomTransData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setAtomTransData(
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
            Text('Atom Transformation Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            Vec3Input(
              label: 'Translation',
              value: _stagedData!.translation,
              onChanged: (newValue) {
                _updateStagedData(APIAtomTransData(
                  translation: newValue,
                  rotation: _stagedData!.rotation,
                ));
              },
            ),
            const SizedBox(height: 8),
            Vec3Input(
              label: 'Rotation',
              value: _stagedData!.rotation,
              onChanged: (newValue) {
                _updateStagedData(APIAtomTransData(
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
