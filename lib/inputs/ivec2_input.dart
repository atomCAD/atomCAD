import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable widget for editing IVec2 values
class IVec2Input extends StatefulWidget {
  final String label;
  final APIIVec2 value;
  final ValueChanged<APIIVec2> onChanged;
  final APIIVec2? minimumValue;
  final APIIVec2? maximumValue;

  const IVec2Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.minimumValue,
    this.maximumValue,
  });

  @override
  State<IVec2Input> createState() => _IVec2InputState();
}

class _IVec2InputState extends State<IVec2Input> {
  late TextEditingController _xController;
  late TextEditingController _yController;

  late FocusNode _xFocusNode;
  late FocusNode _yFocusNode;

  // Helper method to handle scroll events for any axis
  void _handleScrollEvent(PointerScrollEvent event, String axis) {
    // Check if shift key is pressed for larger increments
    final useShiftIncrement = RawKeyboard.instance.keysPressed.any((key) =>
        key == LogicalKeyboardKey.shift ||
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

    _xFocusNode = FocusNode();
    _yFocusNode = FocusNode();

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
  }

  @override
  void didUpdateWidget(IVec2Input oldWidget) {
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
  }

  @override
  void dispose() {
    _xController.dispose();
    _yController.dispose();
    _xFocusNode.dispose();
    _yFocusNode.dispose();
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
          widget.onChanged(APIIVec2(x: validValue, y: widget.value.y));
          break;
        case 'y':
          widget.onChanged(APIIVec2(x: widget.value.x, y: validValue));
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
          widget.onChanged(APIIVec2(x: validValue, y: widget.value.y));
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
          widget.onChanged(APIIVec2(x: widget.value.x, y: validValue));
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
          widget.onChanged(APIIVec2(x: validValue, y: widget.value.y));
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
          widget.onChanged(APIIVec2(x: widget.value.x, y: validValue));
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
    }

    return tooltipLines.join('\n');
  }

  // Helper method to build a TextField widget for an axis
  Widget _buildAxisTextField({
    required String axis,
    required String axisLabel,
    required Color axisColor,
    required TextEditingController controller,
    required FocusNode focusNode,
  }) {
    return ConstrainedBox(
      constraints: AppSpacing.inputFieldConstraints,
      child: Tooltip(
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
              decoration: AppInputDecorations.standard.copyWith(
                labelText: axisLabel,
                labelStyle: TextStyle(
                  fontSize: 13,
                  color: axisColor,
                  fontWeight: FontWeight.bold,
                ),
              ),
              controller: controller,
              focusNode: focusNode,
              keyboardType: TextInputType.number,
              style: AppTextStyles.inputField,
              onSubmitted: (text) => _updateValueFromText(text, axis),
            ),
          ),
        ),
      ),
    );
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
            Flexible(
              child: _buildAxisTextField(
                axis: 'x',
                axisLabel: 'X',
                axisColor: AppColors.xAxisColor,
                controller: _xController,
                focusNode: _xFocusNode,
              ),
            ),
            const SizedBox(width: 4), // Reduced spacing from 8 to 4
            Flexible(
              child: _buildAxisTextField(
                axis: 'y',
                axisLabel: 'Y',
                axisColor: AppColors.yAxisColor,
                controller: _yController,
                focusNode: _yFocusNode,
              ),
            ),
          ],
        ),
      ],
    );
  }
}
