import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';

/// A reusable widget for editing integer values
class IntInput extends StatefulWidget {
  final String label;
  final int value;
  final ValueChanged<int> onChanged;

  const IntInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
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

  void _updateValueFromText(String text) {
    final newValue = int.tryParse(text);
    if (newValue != null) {
      widget.onChanged(newValue);
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _incrementValue() {
    final currentValue = int.tryParse(_controller.text) ?? widget.value;
    final newValue = currentValue + 1;
    _controller.text = newValue.toString();
    widget.onChanged(newValue);
  }

  void _decrementValue() {
    final currentValue = int.tryParse(_controller.text) ?? widget.value;
    final newValue = currentValue - 1;
    _controller.text = newValue.toString();
    widget.onChanged(newValue);
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label),
        Listener(
          onPointerSignal: (event) {
            if (event is PointerScrollEvent) {
              // Scrolling down (positive delta) decreases the value
              // Scrolling up (negative delta) increases the value
              if (event.scrollDelta.dy > 0) {
                _decrementValue();
              } else if (event.scrollDelta.dy < 0) {
                _incrementValue();
              }
            }
          },
          child: TextField(
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
            ),
            controller: _controller,
            focusNode: _focusNode,
            keyboardType: TextInputType.number,
            onSubmitted: (text) {
              _updateValueFromText(text);
            },
          ),
        ),
      ],
    );
  }
}
