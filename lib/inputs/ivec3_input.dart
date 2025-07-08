import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';

/// A reusable widget for editing IVec3 values
class IVec3Input extends StatefulWidget {
  final String label;
  final APIIVec3 value;
  final ValueChanged<APIIVec3> onChanged;
  final APIIVec3? minimumValue;
  final APIIVec3? maximumValue;

  const IVec3Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.minimumValue,
    this.maximumValue,
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

  // Helper method to handle scroll events for any axis
  void _handleScrollEvent(PointerScrollEvent event, String axis) {
    // Check if shift key is pressed for larger increments
    final useShiftIncrement = RawKeyboard.instance.keysPressed
        .any((key) => key == LogicalKeyboardKey.shift ||
                 key == LogicalKeyboardKey.shiftLeft ||
                 key == LogicalKeyboardKey.shiftRight);
    
    if (event.scrollDelta.dy > 0) {
      _decrementValue(axis, useShiftIncrement: useShiftIncrement);
    } else if (event.scrollDelta.dy < 0) {
      _incrementValue(axis, useShiftIncrement: useShiftIncrement);
    }
  }

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

  int? _validateInput(String text, String axis) {
    // Try to parse the input as an integer
    final value = int.tryParse(text);
    if (value == null) {
      return null;
    }

    // Check range constraints if specified
    if (widget.minimumValue != null) {
      switch (axis) {
        case 'x':
          if (value < widget.minimumValue!.x) {
            return null;
          }
          break;
        case 'y':
          if (value < widget.minimumValue!.y) {
            return null;
          }
          break;
        case 'z':
          if (value < widget.minimumValue!.z) {
            return null;
          }
          break;
      }
    }

    if (widget.maximumValue != null) {
      switch (axis) {
        case 'x':
          if (value > widget.maximumValue!.x) {
            return null;
          }
          break;
        case 'y':
          if (value > widget.maximumValue!.y) {
            return null;
          }
          break;
        case 'z':
          if (value > widget.maximumValue!.z) {
            return null;
          }
          break;
      }
    }

    // Input is valid
    return value;
  }

  void _updateValueFromText(String text, String axis) {
    final validValue = _validateInput(text, axis);
    if (validValue != null) {
      switch (axis) {
        case 'x':
          widget.onChanged(
              APIIVec3(x: validValue, y: widget.value.y, z: widget.value.z));
          break;
        case 'y':
          widget.onChanged(
              APIIVec3(x: widget.value.x, y: validValue, z: widget.value.z));
          break;
        case 'z':
          widget.onChanged(
              APIIVec3(x: widget.value.x, y: widget.value.y, z: validValue));
          break;
      }
    } else {
      // If validation fails, restore the previous valid value for this axis
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

  void _incrementValue(String axis, {bool useShiftIncrement = false}) {
    int currentValue;
    // Use increment of 10 if shift is pressed, otherwise increment by 1
    final increment = useShiftIncrement ? 10 : 1;
    
    switch (axis) {
      case 'x':
        currentValue = int.tryParse(_xController.text) ?? widget.value.x;
        var newValue = currentValue + increment;
        
        // Clamp at maxValue if specified
        if (widget.maximumValue != null && newValue > widget.maximumValue!.x) {
          newValue = widget.maximumValue!.x;
        }
        
        final validValue = _validateInput(newValue.toString(), axis);
        if (validValue != null) {
          _xController.text = validValue.toString();
          widget.onChanged(
              APIIVec3(x: validValue, y: widget.value.y, z: widget.value.z));
        }
        break;
        
      case 'y':
        currentValue = int.tryParse(_yController.text) ?? widget.value.y;
        var newValue = currentValue + increment;
        
        // Clamp at maxValue if specified
        if (widget.maximumValue != null && newValue > widget.maximumValue!.y) {
          newValue = widget.maximumValue!.y;
        }
        
        final validValue = _validateInput(newValue.toString(), axis);
        if (validValue != null) {
          _yController.text = validValue.toString();
          widget.onChanged(
              APIIVec3(x: widget.value.x, y: validValue, z: widget.value.z));
        }
        break;
        
      case 'z':
        currentValue = int.tryParse(_zController.text) ?? widget.value.z;
        var newValue = currentValue + increment;
        
        // Clamp at maxValue if specified
        if (widget.maximumValue != null && newValue > widget.maximumValue!.z) {
          newValue = widget.maximumValue!.z;
        }
        
        final validValue = _validateInput(newValue.toString(), axis);
        if (validValue != null) {
          _zController.text = validValue.toString();
          widget.onChanged(
              APIIVec3(x: widget.value.x, y: widget.value.y, z: validValue));
        }
        break;
    }
  }

  void _decrementValue(String axis, {bool useShiftIncrement = false}) {
    int currentValue;
    // Use decrement of 10 if shift is pressed, otherwise decrement by 1
    final decrement = useShiftIncrement ? 10 : 1;
    
    switch (axis) {
      case 'x':
        currentValue = int.tryParse(_xController.text) ?? widget.value.x;
        var newValue = currentValue - decrement;
        
        // Clamp at minValue if specified
        if (widget.minimumValue != null && newValue < widget.minimumValue!.x) {
          newValue = widget.minimumValue!.x;
        }
        
        final validValue = _validateInput(newValue.toString(), axis);
        if (validValue != null) {
          _xController.text = validValue.toString();
          widget.onChanged(APIIVec3(x: validValue, y: widget.value.y, z: widget.value.z));
        }
        break;
        
      case 'y':
        currentValue = int.tryParse(_yController.text) ?? widget.value.y;
        var newValue = currentValue - decrement;
        
        // Clamp at minValue if specified
        if (widget.minimumValue != null && newValue < widget.minimumValue!.y) {
          newValue = widget.minimumValue!.y;
        }
        
        final validValue = _validateInput(newValue.toString(), axis);
        if (validValue != null) {
          _yController.text = validValue.toString();
          widget.onChanged(APIIVec3(x: widget.value.x, y: validValue, z: widget.value.z));
        }
        break;
        
      case 'z':
        currentValue = int.tryParse(_zController.text) ?? widget.value.z;
        var newValue = currentValue - decrement;
        
        // Clamp at minValue if specified
        if (widget.minimumValue != null && newValue < widget.minimumValue!.z) {
          newValue = widget.minimumValue!.z;
        }
        
        final validValue = _validateInput(newValue.toString(), axis);
        if (validValue != null) {
          _zController.text = validValue.toString();
          widget.onChanged(APIIVec3(x: widget.value.x, y: widget.value.y, z: validValue));
        }
        break;
    }
  }

  // Build a tooltip message for a specific axis based on constraints
  String _buildAxisTooltipMessage(String axis) {
    final tooltipLines = [
      'Use mouse wheel to increment/decrement',
      'Hold SHIFT + mouse wheel for 10x steps',
    ];
    
    switch (axis) {
      case 'x':
        if (widget.minimumValue != null) {
          tooltipLines.add('Minimum value: ${widget.minimumValue!.x}');
        }
        if (widget.maximumValue != null) {
          tooltipLines.add('Maximum value: ${widget.maximumValue!.x}');
        }
        break;
      case 'y':
        if (widget.minimumValue != null) {
          tooltipLines.add('Minimum value: ${widget.minimumValue!.y}');
        }
        if (widget.maximumValue != null) {
          tooltipLines.add('Maximum value: ${widget.maximumValue!.y}');
        }
        break;
      case 'z':
        if (widget.minimumValue != null) {
          tooltipLines.add('Minimum value: ${widget.minimumValue!.z}');
        }
        if (widget.maximumValue != null) {
          tooltipLines.add('Maximum value: ${widget.maximumValue!.z}');
        }
        break;
    }
    
    return tooltipLines.join('\n');
  }
  
  // Helper method to build a TextField widget for an axis
  Widget _buildAxisTextField({
    required String axis,
    required String axisLabel,
    required TextEditingController controller,
    required FocusNode focusNode,
    required InputDecoration baseDecoration,
  }) {
    return Tooltip(
      message: _buildAxisTooltipMessage(axis),
      preferBelow: true,
      child: MouseRegion(
        // When mouse enters, block scrolling if service is available
        onEnter: (PointerEnterEvent event) {
          try {
            final service = context.read<MouseWheelBlockService>();
            service.block();
          } catch (e) {
            // Provider not available, do nothing
          }
        },
        // When mouse exits, unblock scrolling if service is available
        onExit: (PointerExitEvent event) {
          try {
            final service = context.read<MouseWheelBlockService>();
            service.unblock();
          } catch (e) {
            // Provider not available, do nothing
          }
        },
        child: Listener(
          onPointerSignal: (event) {
            if (event is PointerScrollEvent) {
              _handleScrollEvent(event, axis);
            }
          },
          child: TextField(
            decoration: baseDecoration.copyWith(labelText: axisLabel),
            controller: controller,
            focusNode: focusNode,
            keyboardType: TextInputType.number,
            onSubmitted: (text) => _updateValueFromText(text, axis),
          ),
        ),
      ),
    );
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
              child: _buildAxisTextField(
                axis: 'x',
                axisLabel: 'X',
                controller: _xController,
                focusNode: _xFocusNode,
                baseDecoration: inputDecoration,
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: _buildAxisTextField(
                axis: 'y',
                axisLabel: 'Y',
                controller: _yController,
                focusNode: _yFocusNode,
                baseDecoration: inputDecoration,
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: _buildAxisTextField(
                axis: 'z',
                axisLabel: 'Z',
                controller: _zController,
                focusNode: _zFocusNode,
                baseDecoration: inputDecoration,
              ),
            ),
          ],
        ),
      ],
    );
  }
}
