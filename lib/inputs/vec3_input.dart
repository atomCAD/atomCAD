import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';

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

  void _handleValueChange(String text, String axis) {
    // Set the currently editing flag
    _currentlyEditingAxis = axis;
    
    final newValue = double.tryParse(text);
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

  @override
  Widget build(BuildContext context) {
    const inputDecoration = InputDecoration(
      border: OutlineInputBorder(),
      contentPadding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      isDense: true,
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label, style: const TextStyle(fontSize: 13)),
        const SizedBox(height: 2),
        Row(
          children: [
            Expanded(
              child: TextField(
                decoration: inputDecoration.copyWith(
                  labelText: 'X',
                  labelStyle: const TextStyle(fontSize: 13),
                ),
                style: const TextStyle(fontSize: 14),
                controller: _xController,
                focusNode: _xFocus,
                keyboardType: const TextInputType.numberWithOptions(decimal: true),
                onChanged: (text) => _handleValueChange(text, 'x'),
              ),
            ),
            const SizedBox(width: 4),
            Expanded(
              child: TextField(
                decoration: inputDecoration.copyWith(
                  labelText: 'Y',
                  labelStyle: const TextStyle(fontSize: 13),
                ),
                style: const TextStyle(fontSize: 14),
                controller: _yController,
                focusNode: _yFocus,
                keyboardType: const TextInputType.numberWithOptions(decimal: true),
                onChanged: (text) => _handleValueChange(text, 'y'),
              ),
            ),
            const SizedBox(width: 4),
            Expanded(
              child: TextField(
                decoration: inputDecoration.copyWith(
                  labelText: 'Z',
                  labelStyle: const TextStyle(fontSize: 13),
                ),
                style: const TextStyle(fontSize: 14),
                controller: _zController,
                focusNode: _zFocus,
                keyboardType: const TextInputType.numberWithOptions(decimal: true),
                onChanged: (text) => _handleValueChange(text, 'z'),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
