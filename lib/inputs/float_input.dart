import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable widget for editing floating-point values
class FloatInput extends StatefulWidget {
  final String label;
  final double value;
  final ValueChanged<double> onChanged;
  final Key? inputKey;

  const FloatInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.inputKey,
  });

  @override
  State<FloatInput> createState() => _FloatInputState();
}

class _FloatInputState extends State<FloatInput> {
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
  void didUpdateWidget(FloatInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value) {
      updateTextControllerWithSelection(_controller, widget.value.toString());
    }
  }

  void _updateValueFromText(String text) {
    final newValue = double.tryParse(text);
    if (newValue != null) {
      widget.onChanged(newValue);
    } else {
      // If parsing fails, restore the previous valid value
      _controller.text = widget.value.toString();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label),
        TextField(
          key: widget.inputKey,
          decoration: const InputDecoration(
            border: OutlineInputBorder(),
          ),
          controller: _controller,
          focusNode: _focusNode,
          keyboardType: const TextInputType.numberWithOptions(decimal: true),
          onSubmitted: (text) {
            _updateValueFromText(text);
          },
        ),
      ],
    );
  }
}
