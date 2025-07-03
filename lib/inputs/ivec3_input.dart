import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

/// A reusable widget for editing IVec3 values
class IVec3Input extends StatefulWidget {
  final String label;
  final APIIVec3 value;
  final ValueChanged<APIIVec3> onChanged;

  const IVec3Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  State<IVec3Input> createState() => _IVec3InputState();
}

class _IVec3InputState extends State<IVec3Input> {
  late TextEditingController _xController;
  late TextEditingController _yController;
  late TextEditingController _zController;
  
  late FocusNode _xFocusNode;
  late FocusNode _yFocusNode;
  late FocusNode _zFocusNode;

  @override
  void initState() {
    super.initState();
    _xController = TextEditingController(text: widget.value.x.toString());
    _yController = TextEditingController(text: widget.value.y.toString());
    _zController = TextEditingController(text: widget.value.z.toString());
    
    _xFocusNode = FocusNode();
    _yFocusNode = FocusNode();
    _zFocusNode = FocusNode();
    
    _xFocusNode.addListener(() {
      if (!_xFocusNode.hasFocus) {
        // When focus is lost, update the value
        _updateValueFromText(_xController.text, 'x');
      }
    });
    
    _yFocusNode.addListener(() {
      if (!_yFocusNode.hasFocus) {
        // When focus is lost, update the value
        _updateValueFromText(_yController.text, 'y');
      }
    });
    
    _zFocusNode.addListener(() {
      if (!_zFocusNode.hasFocus) {
        // When focus is lost, update the value
        _updateValueFromText(_zController.text, 'z');
      }
    });
  }

  @override
  void didUpdateWidget(IVec3Input oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value.x != widget.value.x) {
      final selection = _xController.selection;
      _xController.text = widget.value.x.toString();
      _xController.selection = selection;
    }
    if (oldWidget.value.y != widget.value.y) {
      final selection = _yController.selection;
      _yController.text = widget.value.y.toString();
      _yController.selection = selection;
    }
    if (oldWidget.value.z != widget.value.z) {
      final selection = _zController.selection;
      _zController.text = widget.value.z.toString();
      _zController.selection = selection;
    }
  }

  @override
  void dispose() {
    _xController.dispose();
    _yController.dispose();
    _zController.dispose();
    _xFocusNode.dispose();
    _yFocusNode.dispose();
    _zFocusNode.dispose();
    super.dispose();
  }

  void _updateValueFromText(String text, String axis) {
    final newValue = int.tryParse(text);
    if (newValue != null) {
      switch (axis) {
        case 'x':
          widget.onChanged(
              APIIVec3(x: newValue, y: widget.value.y, z: widget.value.z));
          break;
        case 'y':
          widget.onChanged(
              APIIVec3(x: widget.value.x, y: newValue, z: widget.value.z));
          break;
        case 'z':
          widget.onChanged(
              APIIVec3(x: widget.value.x, y: widget.value.y, z: newValue));
          break;
      }
    } else {
      // If parsing fails, restore the previous valid value for this axis
      switch (axis) {
        case 'x':
          _xController.text = widget.value.x.toString();
          _xController.selection = TextSelection.fromPosition(
            TextPosition(offset: _xController.text.length),
          );
          break;
        case 'y':
          _yController.text = widget.value.y.toString();
          _yController.selection = TextSelection.fromPosition(
            TextPosition(offset: _yController.text.length),
          );
          break;
        case 'z':
          _zController.text = widget.value.z.toString();
          _zController.selection = TextSelection.fromPosition(
            TextPosition(offset: _zController.text.length),
          );
          break;
      }
    }
  }

  void _incrementValue(String axis) {
    int currentValue;
    switch (axis) {
      case 'x':
        currentValue = int.tryParse(_xController.text) ?? widget.value.x;
        final newValue = currentValue + 1;
        _xController.text = newValue.toString();
        widget.onChanged(APIIVec3(x: newValue, y: widget.value.y, z: widget.value.z));
        break;
      case 'y':
        currentValue = int.tryParse(_yController.text) ?? widget.value.y;
        final newValue = currentValue + 1;
        _yController.text = newValue.toString();
        widget.onChanged(APIIVec3(x: widget.value.x, y: newValue, z: widget.value.z));
        break;
      case 'z':
        currentValue = int.tryParse(_zController.text) ?? widget.value.z;
        final newValue = currentValue + 1;
        _zController.text = newValue.toString();
        widget.onChanged(APIIVec3(x: widget.value.x, y: widget.value.y, z: newValue));
        break;
    }
  }

  void _decrementValue(String axis) {
    int currentValue;
    switch (axis) {
      case 'x':
        currentValue = int.tryParse(_xController.text) ?? widget.value.x;
        final newValue = currentValue - 1;
        _xController.text = newValue.toString();
        widget.onChanged(APIIVec3(x: newValue, y: widget.value.y, z: widget.value.z));
        break;
      case 'y':
        currentValue = int.tryParse(_yController.text) ?? widget.value.y;
        final newValue = currentValue - 1;
        _yController.text = newValue.toString();
        widget.onChanged(APIIVec3(x: widget.value.x, y: newValue, z: widget.value.z));
        break;
      case 'z':
        currentValue = int.tryParse(_zController.text) ?? widget.value.z;
        final newValue = currentValue - 1;
        _zController.text = newValue.toString();
        widget.onChanged(APIIVec3(x: widget.value.x, y: widget.value.y, z: newValue));
        break;
    }
  }

  @override
  Widget build(BuildContext context) {
    const inputDecoration = InputDecoration(
      border: OutlineInputBorder(),
      contentPadding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label),
        const SizedBox(height: 4),
        Row(
          children: [
            Expanded(
              child: Listener(
                onPointerSignal: (event) {
                  if (event is PointerScrollEvent) {
                    if (event.scrollDelta.dy > 0) {
                      _decrementValue('x');
                    } else if (event.scrollDelta.dy < 0) {
                      _incrementValue('x');
                    }
                  }
                },
                child: TextField(
                  decoration: inputDecoration.copyWith(labelText: 'X'),
                  controller: _xController,
                  focusNode: _xFocusNode,
                  keyboardType: TextInputType.number,
                  onSubmitted: (text) => _updateValueFromText(text, 'x'),
                ),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Listener(
                onPointerSignal: (event) {
                  if (event is PointerScrollEvent) {
                    if (event.scrollDelta.dy > 0) {
                      _decrementValue('y');
                    } else if (event.scrollDelta.dy < 0) {
                      _incrementValue('y');
                    }
                  }
                },
                child: TextField(
                  decoration: inputDecoration.copyWith(labelText: 'Y'),
                  controller: _yController,
                  focusNode: _yFocusNode,
                  keyboardType: TextInputType.number,
                  onSubmitted: (text) => _updateValueFromText(text, 'y'),
                ),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Listener(
                onPointerSignal: (event) {
                  if (event is PointerScrollEvent) {
                    if (event.scrollDelta.dy > 0) {
                      _decrementValue('z');
                    } else if (event.scrollDelta.dy < 0) {
                      _incrementValue('z');
                    }
                  }
                },
                child: TextField(
                  decoration: inputDecoration.copyWith(labelText: 'Z'),
                  controller: _zController,
                  focusNode: _zFocusNode,
                  keyboardType: TextInputType.number,
                  onSubmitted: (text) => _updateValueFromText(text, 'z'),
                ),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
