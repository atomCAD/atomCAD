import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';

/// A reusable widget for editing IVec3 values
class IVec3Input extends StatefulWidget {
  final String label;
  final APIIVec3 value;
  final ValueChanged<APIIVec3> onChanged;

  const IVec3Input({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  State<IVec3Input> createState() => IVec3InputState();
}

class IVec3InputState extends State<IVec3Input> {
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
  void didUpdateWidget(IVec3Input oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.value.x.toString() != _xController.text) {
      _xController.text = widget.value.x.toString();
    }
    if (widget.value.y.toString() != _yController.text) {
      _yController.text = widget.value.y.toString();
    }
    if (widget.value.z.toString() != _zController.text) {
      _zController.text = widget.value.z.toString();
    }
  }

  @override
  void dispose() {
    _xController.dispose();
    _yController.dispose();
    _zController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label),
        Row(
          children: [
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'X'),
                controller: _xController,
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? widget.value.x;
                  widget.onChanged(APIIVec3(
                      x: newValue, y: widget.value.y, z: widget.value.z));
                },
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'Y'),
                controller: _yController,
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? widget.value.y;
                  widget.onChanged(APIIVec3(
                      x: widget.value.x, y: newValue, z: widget.value.z));
                },
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'Z'),
                controller: _zController,
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? widget.value.z;
                  widget.onChanged(APIIVec3(
                      x: widget.value.x, y: widget.value.y, z: newValue));
                },
              ),
            ),
          ],
        ),
      ],
    );
  }
}
