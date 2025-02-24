import 'package:flutter/material.dart';

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

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value.toString());
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
          keyboardType: TextInputType.number,
          onChanged: (text) {
            final newValue = int.tryParse(text);
            if (newValue != null) {
              widget.onChanged(newValue);
            }
          },
        ),
      ],
    );
  }
}
