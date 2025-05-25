import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for stamp nodes
class StampEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIStampView? data;
  final StructureDesignerModel model;

  const StampEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<StampEditor> createState() => _StampEditorState();
}

class _StampEditorState extends State<StampEditor> {
  @override
  void initState() {
    super.initState();
  }

  @override
  void didUpdateWidget(StampEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
  }

  // Get all rotation options
  List<DropdownMenuItem<int>> _getRotationOptions() {
    return [
      const DropdownMenuItem<int>(value: 0, child: Text('Identity')),
      const DropdownMenuItem<int>(value: 1, child: Text('180° around [1 0 0]')),
      const DropdownMenuItem<int>(value: 2, child: Text('180° around [0 1 0]')),
      const DropdownMenuItem<int>(value: 3, child: Text('180° around [0 0 1]')),
      const DropdownMenuItem<int>(
          value: 4, child: Text('+120° around [1 1 1]')),
      const DropdownMenuItem<int>(
          value: 5, child: Text('–120° around [1 1 1]')),
      const DropdownMenuItem<int>(
          value: 6, child: Text('+120° around [–1 1 1]')),
      const DropdownMenuItem<int>(
          value: 7, child: Text('–120° around [–1 1 1]')),
      const DropdownMenuItem<int>(
          value: 8, child: Text('+120° around [1 –1 1]')),
      const DropdownMenuItem<int>(
          value: 9, child: Text('–120° around [1 –1 1]')),
      const DropdownMenuItem<int>(
          value: 10, child: Text('+120° around [1 1 –1]')),
      const DropdownMenuItem<int>(
          value: 11, child: Text('–120° around [1 1 –1]')),
    ];
  }

  @override
  Widget build(BuildContext context) {
    // If no data or no stamp placement is selected, show a message
    if (widget.data == null || widget.data!.selectedStampPlacement == null) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(AppSpacing.medium),
          child: Text(
              'No stamp placement selected. Click on a crystal atom to add a stamp placement.'),
        ),
      );
    }

    // Get the current values
    final stampPlacement = widget.data!.selectedStampPlacement!;

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8.0, vertical: 4.0),
      child: Card(
        margin: EdgeInsets.zero,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10.0, vertical: 8.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              // Title
              Text(
                'Stamp Placement',
                style: Theme.of(context).textTheme.titleMedium,
              ),
              const SizedBox(height: 4),

              // Position info (in unit cells)
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  _buildCoordinateDisplay('X', stampPlacement.position.x / 4),
                  _buildCoordinateDisplay('Y', stampPlacement.position.y / 4),
                  _buildCoordinateDisplay('Z', stampPlacement.position.z / 4),
                ],
              ),
              const SizedBox(height: 8),

              // Rotation dropdown
              Row(
                children: [
                  const SizedBox(width: 60, child: Text('Rotation:')),
                  const SizedBox(width: 4),
                  Expanded(
                    child: DropdownButton<int>(
                      isExpanded: true,
                      isDense: true,
                      value: stampPlacement.rotation,
                      items: _getRotationOptions(),
                      onChanged: (value) {
                        if (value != null) {
                          widget.model.setStampRotation(widget.nodeId, value);
                        }
                      },
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),

              // Delete button
              Center(
                child: ElevatedButton.icon(
                  style: AppButtonStyles.primary,
                  icon: const Icon(Icons.delete),
                  label: const Text('Delete Placement'),
                  onPressed: () {
                    widget.model.deleteSelectedStampPlacement(widget.nodeId);
                  },
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildCoordinateDisplay(String label, double value) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text('$label: ', style: const TextStyle(fontWeight: FontWeight.bold)),
        Text(value.toStringAsFixed(2)),
      ],
    );
  }
}
