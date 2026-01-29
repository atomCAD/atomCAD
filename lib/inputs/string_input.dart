import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable widget for editing string values
class StringInput extends StatefulWidget {
  final String label;
  final String value;
  final ValueChanged<String> onChanged;
  final Key? inputKey;

  const StringInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.inputKey,
  });

  @override
  State<StringInput> createState() => _StringInputState();
}

class _StringInputState extends State<StringInput> {
  late TextEditingController _controller;
  late FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value);
    _focusNode = FocusNode();
    _focusNode.addListener(() {
      if (!_focusNode.hasFocus) {
        // When focus is lost, update the value
        _updateValueFromText(_controller.text);
      }
    });
  }

  @override
  void didUpdateWidget(StringInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value) {
      final selection = _controller.selection;
      _controller.text = widget.value;

      // Ensure the selection is valid for the new text length
      final newTextLength = _controller.text.length;
      if (selection.isValid && selection.end <= newTextLength) {
        _controller.selection = selection;
      } else {
        // Set cursor to end of text if selection is invalid
        _controller.selection = TextSelection.collapsed(offset: newTextLength);
      }
    }
  }

  void _updateValueFromText(String text) {
    // For strings, we just pass through the text as-is
    widget.onChanged(text);
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: double.infinity,
      child: TextField(
        key: widget.inputKey,
        decoration: AppInputDecorations.standard.copyWith(
          labelText: widget.label,
        ),
        controller: _controller,
        focusNode: _focusNode,
        keyboardType: TextInputType.text,
        style: AppTextStyles.inputField,
        onSubmitted: (text) {
          _updateValueFromText(text);
        },
      ),
    );
  }
}
