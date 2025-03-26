import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable widget for editing Vec3 (floating point) values
class Vec3Input extends StatefulWidget {
  final String label;
  final APIVec3 value;
  final ValueChanged<APIVec3> onChanged;

  const Vec3Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  State<Vec3Input> createState() => _Vec3InputState();
}

class _Vec3InputState extends State<Vec3Input> {
  late TextEditingController _xController;
  late TextEditingController _yController;
  late TextEditingController _zController;
  
  // Add FocusNodes to track focus
  late FocusNode _xFocus;
  late FocusNode _yFocus;
  late FocusNode _zFocus;
  
  // Track which field is currently being edited
  String? _currentlyEditingAxis;

  @override
  void initState() {
    super.initState();
    _xController = TextEditingController(text: widget.value.x.toStringAsFixed(6));
    _yController = TextEditingController(text: widget.value.y.toStringAsFixed(6));
    _zController = TextEditingController(text: widget.value.z.toStringAsFixed(6));
    
    // Initialize focus nodes
    _xFocus = FocusNode();
    _yFocus = FocusNode();
    _zFocus = FocusNode();
    
    // Add listeners to focus nodes
    _xFocus.addListener(_handleXFocusChange);
    _yFocus.addListener(_handleYFocusChange);
    _zFocus.addListener(_handleZFocusChange);
  }
  
  // Handle focus changes
  void _handleXFocusChange() {
    if (_xFocus.hasFocus) {
      _currentlyEditingAxis = 'x';
    } else if (_currentlyEditingAxis == 'x') {
      _currentlyEditingAxis = null;
    }
  }
  
  void _handleYFocusChange() {
    if (_yFocus.hasFocus) {
      _currentlyEditingAxis = 'y';
    } else if (_currentlyEditingAxis == 'y') {
      _currentlyEditingAxis = null;
    }
  }
  
  void _handleZFocusChange() {
    if (_zFocus.hasFocus) {
      _currentlyEditingAxis = 'z';
    } else if (_currentlyEditingAxis == 'z') {
      _currentlyEditingAxis = null;
    }
  }

  @override
  void didUpdateWidget(Vec3Input oldWidget) {
    super.didUpdateWidget(oldWidget);
    
    // Only update controllers if NOT currently editing this specific axis
    if (_currentlyEditingAxis != 'x' && oldWidget.value.x != widget.value.x) {
      final selection = _xController.selection;
      _xController.text = widget.value.x.toStringAsFixed(6);
      _xController.selection = selection;
    }
    
    if (_currentlyEditingAxis != 'y' && oldWidget.value.y != widget.value.y) {
      final selection = _yController.selection;
      _yController.text = widget.value.y.toStringAsFixed(6);
      _yController.selection = selection;
    }
    
    if (_currentlyEditingAxis != 'z' && oldWidget.value.z != widget.value.z) {
      final selection = _zController.selection;
      _zController.text = widget.value.z.toStringAsFixed(6);
      _zController.selection = selection;
    }
  }

  @override
  void dispose() {
    _xController.dispose();
    _yController.dispose();
    _zController.dispose();
    
    // Clean up focus nodes
    _xFocus.removeListener(_handleXFocusChange);
    _yFocus.removeListener(_handleYFocusChange);
    _zFocus.removeListener(_handleZFocusChange);
    _xFocus.dispose();
    _yFocus.dispose();
    _zFocus.dispose();
    
    super.dispose();
  }

  void _applyValueChange(String axis) {
    final newValue = double.tryParse(_getController(axis).text);
    if (newValue != null) {
      switch (axis) {
        case 'x':
          widget.onChanged(APIVec3(
              x: newValue, y: widget.value.y, z: widget.value.z));
          break;
        case 'y':
          widget.onChanged(APIVec3(
              x: widget.value.x, y: newValue, z: widget.value.z));
          break;
        case 'z':
          widget.onChanged(APIVec3(
              x: widget.value.x, y: widget.value.y, z: newValue));
          break;
      }
    }
  }

  TextEditingController _getController(String axis) {
    switch (axis) {
      case 'x':
        return _xController;
      case 'y':
        return _yController;
      case 'z':
        return _zController;
      default:
        throw Exception('Invalid axis');
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label, style: AppTextStyles.label),
        const SizedBox(height: 4),
        Row(
          children: [
            Expanded(
              child: Focus(
                onFocusChange: (hasFocus) {
                  if (!hasFocus && _currentlyEditingAxis == 'x') {
                    _applyValueChange('x');
                    _currentlyEditingAxis = null;
                  }
                },
                child: TextField(
                  controller: _xController,
                  focusNode: _xFocus,
                  style: AppTextStyles.inputField,
                  decoration: AppInputDecorations.standard.copyWith(
                    labelText: 'X',
                    labelStyle: TextStyle(
                      fontSize: 13,
                      color: AppColors.xAxisColor,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  keyboardType:
                      const TextInputType.numberWithOptions(decimal: true),
                  onChanged: (value) {
                    // Mark that we're editing this field
                    _currentlyEditingAxis = 'x';
                  },
                  onSubmitted: (value) {
                    _applyValueChange('x');
                    _currentlyEditingAxis = null;
                  },
                ),
              ),
            ),
            const SizedBox(width: 4),
            Expanded(
              child: Focus(
                onFocusChange: (hasFocus) {
                  if (!hasFocus && _currentlyEditingAxis == 'y') {
                    _applyValueChange('y');
                    _currentlyEditingAxis = null;
                  }
                },
                child: TextField(
                  controller: _yController,
                  focusNode: _yFocus,
                  style: AppTextStyles.inputField,
                  decoration: AppInputDecorations.standard.copyWith(
                    labelText: 'Y',
                    labelStyle: TextStyle(
                      fontSize: 13,
                      color: AppColors.yAxisColor,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  keyboardType:
                      const TextInputType.numberWithOptions(decimal: true),
                  onChanged: (value) {
                    // Mark that we're editing this field
                    _currentlyEditingAxis = 'y';
                  },
                  onSubmitted: (value) {
                    _applyValueChange('y');
                    _currentlyEditingAxis = null;
                  },
                ),
              ),
            ),
            const SizedBox(width: 4),
            Expanded(
              child: Focus(
                onFocusChange: (hasFocus) {
                  if (!hasFocus && _currentlyEditingAxis == 'z') {
                    _applyValueChange('z');
                    _currentlyEditingAxis = null;
                  }
                },
                child: TextField(
                  controller: _zController,
                  focusNode: _zFocus,
                  style: AppTextStyles.inputField,
                  decoration: AppInputDecorations.standard.copyWith(
                    labelText: 'Z',
                    labelStyle: TextStyle(
                      fontSize: 13,
                      color: AppColors.zAxisColor,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  keyboardType:
                      const TextInputType.numberWithOptions(decimal: true),
                  onChanged: (value) {
                    // Mark that we're editing this field
                    _currentlyEditingAxis = 'z';
                  },
                  onSubmitted: (value) {
                    _applyValueChange('z');
                    _currentlyEditingAxis = null;
                  },
                ),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
