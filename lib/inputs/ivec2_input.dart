import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

/// A reusable widget for editing IVec2 values
class IVec2Input extends StatefulWidget {
  final String label;
  final APIIVec2 value;
  final ValueChanged<APIIVec2> onChanged;

  const IVec2Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  State<IVec2Input> createState() => _IVec2InputState();
}

class _IVec2InputState extends State<IVec2Input> {
  late TextEditingController _xController;
  late TextEditingController _yController;

  @override
  void initState() {
    super.initState();
    _xController = TextEditingController(text: widget.value.x.toString());
    _yController = TextEditingController(text: widget.value.y.toString());
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
    super.dispose();
  }

  void _handleValueChange(String text, String axis) {
    final newValue = int.tryParse(text);
    if (newValue != null) {
      switch (axis) {
        case 'x':
          widget.onChanged(APIIVec2(x: newValue, y: widget.value.y));
          break;
        case 'y':
          widget.onChanged(APIIVec2(x: widget.value.x, y: newValue));
          break;
      }
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
              child: TextField(
                decoration: inputDecoration.copyWith(labelText: 'X'),
                controller: _xController,
                keyboardType: TextInputType.number,
                onChanged: (text) => _handleValueChange(text, 'x'),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: inputDecoration.copyWith(labelText: 'Y'),
                controller: _yController,
                keyboardType: TextInputType.number,
                onChanged: (text) => _handleValueChange(text, 'y'),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
