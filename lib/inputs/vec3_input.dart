import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';

/// A reusable widget for editing Vec3 (floating point) values
class Vec3Input extends StatefulWidget {
  final String label;
  final APIVec3 value;
  final ValueChanged<APIVec3> onChanged;

  const Vec3Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  State<Vec3Input> createState() => _Vec3InputState();
}

class _Vec3InputState extends State<Vec3Input> {
  late TextEditingController _xController;
  late TextEditingController _yController;
  late TextEditingController _zController;

  @override
  void initState() {
    super.initState();
    _xController = TextEditingController(text: widget.value.x.toString());
    _yController = TextEditingController(text: widget.value.y.toString());
    _zController = TextEditingController(text: widget.value.z.toString());
  }

  @override
  void didUpdateWidget(Vec3Input oldWidget) {
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
    super.dispose();
  }

  void _handleValueChange(String text, String axis) {
    final newValue = double.tryParse(text);
    if (newValue != null) {
      switch (axis) {
        case 'x':
          widget.onChanged(APIVec3(
              x: newValue, y: widget.value.y, z: widget.value.z));
          break;
        case 'y':
          widget.onChanged(APIVec3(
              x: widget.value.x, y: newValue, z: widget.value.z));
          break;
        case 'z':
          widget.onChanged(APIVec3(
              x: widget.value.x, y: widget.value.y, z: newValue));
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
                keyboardType: const TextInputType.numberWithOptions(decimal: true),
                onChanged: (text) => _handleValueChange(text, 'x'),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: inputDecoration.copyWith(labelText: 'Y'),
                controller: _yController,
                keyboardType: const TextInputType.numberWithOptions(decimal: true),
                onChanged: (text) => _handleValueChange(text, 'y'),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: inputDecoration.copyWith(labelText: 'Z'),
                controller: _zController,
                keyboardType: const TextInputType.numberWithOptions(decimal: true),
                onChanged: (text) => _handleValueChange(text, 'z'),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
