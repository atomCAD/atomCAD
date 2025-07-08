import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable widget for editing integer values
class IntInput extends StatefulWidget {
  final String label;
  final int value;
  final ValueChanged<int> onChanged;
  final int? minimumValue;
  final int? maximumValue;

  const IntInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.minimumValue,
    this.maximumValue,
  });

  @override
  State<IntInput> createState() => _IntInputState();
}

class _IntInputState extends State<IntInput> {
  late TextEditingController _controller;
  late FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value.toString());
    _focusNode = FocusNode();
    _focusNode.addListener(() {
      if (!_focusNode.hasFocus) {
        // When focus is lost, update the value
        _updateValueFromText(_controller.text);
      }
    });
  }

  @override
  void didUpdateWidget(IntInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value) {
      final selection = _controller.selection;
      _controller.text = widget.value.toString();
      _controller.selection = selection;
    }
  }

  int? _validateInput(String text) {
    // Try to parse the input as an integer
    final value = int.tryParse(text);
    if (value == null) {
      return null;
    }

    // Check range constraints if specified
    if (widget.minimumValue != null && value < widget.minimumValue!) {
      return null;
    }
    if (widget.maximumValue != null && value > widget.maximumValue!) {
      return null;
    }

    // Input is valid
    return value;
  }

  void _updateValueFromText(String text) {
    final validValue = _validateInput(text);
    if (validValue != null) {
      widget.onChanged(validValue);
    } else {
      // If validation fails, restore the previous valid value
      _controller.text = widget.value.toString();

      // Position cursor at the end of the text
      _controller.selection = TextSelection.fromPosition(
        TextPosition(offset: _controller.text.length),
      );
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _incrementValue({bool useShiftIncrement = false}) {
    final currentValue = int.tryParse(_controller.text) ?? widget.value;
    // Use increment of 10 if shift is pressed, otherwise increment by 1
    final increment = useShiftIncrement ? 10 : 1;
    var newValue = currentValue + increment;

    // Clamp at maxValue if specified
    if (widget.maximumValue != null && newValue > widget.maximumValue!) {
      newValue = widget.maximumValue!;
    }

    // Still validate to handle other cases and ensure the value is within bounds
    final validatedValue = _validateInput(newValue.toString());
    if (validatedValue != null) {
      _controller.text = validatedValue.toString();
      widget.onChanged(validatedValue);
    }
  }

  void _decrementValue({bool useShiftIncrement = false}) {
    final currentValue = int.tryParse(_controller.text) ?? widget.value;
    // Use decrement of 10 if shift is pressed, otherwise decrement by 1
    final decrement = useShiftIncrement ? 10 : 1;
    var newValue = currentValue - decrement;

    // Clamp at minValue if specified
    if (widget.minimumValue != null && newValue < widget.minimumValue!) {
      newValue = widget.minimumValue!;
    }

    // Still validate to handle other cases and ensure the value is within bounds
    final validatedValue = _validateInput(newValue.toString());
    if (validatedValue != null) {
      _controller.text = validatedValue.toString();
      widget.onChanged(validatedValue);
    }
  }

  @override
  // Build a tooltip message based on constraints
  String _buildTooltipMessage() {
    List<String> tooltipLines = [
      'Use mouse scroll wheel to quickly change the value',
      'Hold SHIFT while scrolling for 10x increments'
    ];

    if (widget.minimumValue != null) {
      tooltipLines.add('Minimum value: ${widget.minimumValue}');
    }

    if (widget.maximumValue != null) {
      tooltipLines.add('Maximum value: ${widget.maximumValue}');
    }

    return tooltipLines.join('\n');
  }

  Widget build(BuildContext context) {
    final tooltipMessage = _buildTooltipMessage();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label, style: AppTextStyles.label),
        const SizedBox(height: 4),
        ConstrainedBox(
          constraints: AppSpacing.inputFieldConstraints,
          child: Tooltip(
            message: tooltipMessage,
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
                    // Check if shift key is pressed for larger increments
                    final useShiftIncrement = RawKeyboard.instance.keysPressed
                        .any((key) =>
                            key == LogicalKeyboardKey.shift ||
                            key == LogicalKeyboardKey.shiftLeft ||
                            key == LogicalKeyboardKey.shiftRight);

                    // Scrolling down (positive delta) decreases the value
                    // Scrolling up (negative delta) increases the value
                    if (event.scrollDelta.dy > 0) {
                      _decrementValue(useShiftIncrement: useShiftIncrement);
                    } else if (event.scrollDelta.dy < 0) {
                      _incrementValue(useShiftIncrement: useShiftIncrement);
                    }
                  }
                },
                child: TextField(
                  decoration: AppInputDecorations.standard,
                  controller: _controller,
                  focusNode: _focusNode,
                  keyboardType: TextInputType.number,
                  style: AppTextStyles.inputField,
                  onSubmitted: (text) {
                    _updateValueFromText(text);
                  },
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}
