import 'package:flutter/material.dart';

/// A reusable widget for editing integer values
class IntInput extends StatelessWidget {
  final String label;
  final int value;
  final ValueChanged<int> onChanged;

  const IntInput({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label),
        TextField(
          decoration: const InputDecoration(
            border: OutlineInputBorder(),
          ),
          controller: TextEditingController(text: value.toString()),
          keyboardType: TextInputType.number,
          onChanged: (text) {
            final newValue = int.tryParse(text);
            if (newValue != null) {
              onChanged(newValue);
            }
          },
        ),
      ],
    );
  }
}
