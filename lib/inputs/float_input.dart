import 'package:flutter/material.dart';

/// A reusable widget for editing floating-point values
class FloatInput extends StatefulWidget {
  final String label;
  final double value;
  final ValueChanged<double> onChanged;

  const FloatInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  State<FloatInput> createState() => _FloatInputState();
}

class _FloatInputState extends State<FloatInput> {
  late TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value.toString());
  }

  @override
  void didUpdateWidget(FloatInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value) {
      final selection = _controller.selection;
      _controller.text = widget.value.toString();
      _controller.selection = selection;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label),
        TextField(
          decoration: const InputDecoration(
            border: OutlineInputBorder(),
          ),
          controller: _controller,
          keyboardType: const TextInputType.numberWithOptions(decimal: true),
          onChanged: (text) {
            final newValue = double.tryParse(text);
            if (newValue != null) {
              widget.onChanged(newValue);
            }
          },
        ),
      ],
    );
  }
}
