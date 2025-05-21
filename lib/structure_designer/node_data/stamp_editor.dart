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
  // Store which x_dir is currently selected to calculate y_dir options
  int? _currentXDir;

  @override
  void initState() {
    super.initState();
    _currentXDir = widget.data?.selectedStampPlacement?.xDir;
  }

  @override
  void didUpdateWidget(StampEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      _currentXDir = widget.data?.selectedStampPlacement?.xDir;
    }
  }

  // Get all x direction options
  List<DropdownMenuItem<int>> _getXDirOptions() {
    return [
      const DropdownMenuItem<int>(value: 0, child: Text('+X')),
      const DropdownMenuItem<int>(value: 1, child: Text('-X')),
      const DropdownMenuItem<int>(value: 2, child: Text('+Y')),
      const DropdownMenuItem<int>(value: 3, child: Text('-Y')),
      const DropdownMenuItem<int>(value: 4, child: Text('+Z')),
      const DropdownMenuItem<int>(value: 5, child: Text('-Z')),
    ];
  }

  // Get y direction options based on current x_dir selection
  List<DropdownMenuItem<int>> _getYDirOptions() {
    if (_currentXDir == null) {
      return [];
    }

    // Determine which axes are available for y_dir based on x_dir
    // X axis options (0, 1) => Y and Z axes are available
    // Y axis options (2, 3) => X and Z axes are available
    // Z axis options (4, 5) => X and Y axes are available
    if (_currentXDir == 0 || _currentXDir == 1) {
      // X axis selected, Y and Z axes available
      return [
        const DropdownMenuItem<int>(value: 0, child: Text('+Y')),
        const DropdownMenuItem<int>(value: 1, child: Text('-Y')),
        const DropdownMenuItem<int>(value: 2, child: Text('+Z')),
        const DropdownMenuItem<int>(value: 3, child: Text('-Z')),
      ];
    } else if (_currentXDir == 2 || _currentXDir == 3) {
      // Y axis selected, X and Z axes available
      return [
        const DropdownMenuItem<int>(value: 0, child: Text('+X')),
        const DropdownMenuItem<int>(value: 1, child: Text('-X')),
        const DropdownMenuItem<int>(value: 2, child: Text('+Z')),
        const DropdownMenuItem<int>(value: 3, child: Text('-Z')),
      ];
    } else {
      // Z axis selected, X and Y axes available
      return [
        const DropdownMenuItem<int>(value: 0, child: Text('+X')),
        const DropdownMenuItem<int>(value: 1, child: Text('-X')),
        const DropdownMenuItem<int>(value: 2, child: Text('+Y')),
        const DropdownMenuItem<int>(value: 3, child: Text('-Y')),
      ];
    }
  }

  @override
  Widget build(BuildContext context) {
    // If no data or no stamp placement is selected, show a message
    if (widget.data == null || widget.data!.selectedStampPlacement == null) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(AppSpacing.medium),
          child: Text('No stamp placement selected. Click on a crystal atom to add a stamp placement.'),
        ),
      );
    }

    // Get the current values
    final stampPlacement = widget.data!.selectedStampPlacement!;
    _currentXDir = stampPlacement.xDir;

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
              
              // X direction dropdown
              Row(
                children: [
                  const SizedBox(width: 60, child: Text('X-Dir:')),
                  const SizedBox(width: 4),
                  Expanded(
                    child: DropdownButton<int>(
                      isExpanded: true,
                      isDense: true,
                      value: _currentXDir,
                      items: _getXDirOptions(),
                      onChanged: (value) {
                        if (value != null) {
                          setState(() {
                            _currentXDir = value;
                          });
                          widget.model.setStampXDir(widget.nodeId, value);
                        }
                      },
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 4),
              
              // Y direction dropdown
              Row(
                children: [
                  const SizedBox(width: 60, child: Text('Y-Dir:')),
                  const SizedBox(width: 4),
                  Expanded(
                    child: DropdownButton<int>(
                      isExpanded: true,
                      isDense: true,
                      value: stampPlacement.yDir,
                      items: _getYDirOptions(),
                      onChanged: (value) {
                        if (value != null) {
                          widget.model.setStampYDir(widget.nodeId, value);
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
