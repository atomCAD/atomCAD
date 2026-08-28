import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable widget for editing floating-point values
class FloatInput extends StatefulWidget {
  final String label;
  final double value;
  final ValueChanged<double> onChanged;
  final Key? inputKey;

  /// When false the field is greyed out and refuses focus. Used where a value
  /// is stored and worth showing but nothing reads it in the current
  /// configuration — the `isosurface` colormap domain with no `color_field`
  /// wired, for instance. The value still renders, so it stays inspectable.
  final bool enabled;

  /// When true the field renders **empty** instead of its value. Only
  /// meaningful together with `enabled: false`: it is for the case where there
  /// is no number that would be used and showing the stored one would mislead —
  /// the `isosurface` level rows with no field wired, for instance. The normal
  /// disabled case still shows its value, because that value is what a wire or
  /// a mode change would make live again.
  final bool blank;

  const FloatInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.inputKey,
    this.enabled = true,
    this.blank = false,
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
    _controller = TextEditingController(text: _displayText);
    _focusNode = FocusNode();
    _focusNode.addListener(() {
      if (!_focusNode.hasFocus) {
        // When focus is lost, update the value
        _updateValueFromText(_controller.text);
      }
    });
  }

  String get _displayText => widget.blank ? '' : widget.value.toString();

  @override
  void didUpdateWidget(FloatInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value || oldWidget.blank != widget.blank) {
      updateTextControllerWithSelection(_controller, _displayText);
    }
  }

  void _updateValueFromText(String text) {
    if (widget.blank && text.isEmpty) {
      // Nothing was typed into a deliberately empty field — leave it empty
      // rather than writing a fabricated 0.
      return;
    }
    final newValue = double.tryParse(text);
    if (newValue != null) {
      widget.onChanged(newValue);
    } else {
      // If parsing fails, restore the previous valid value
      _controller.text = _displayText;
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
          enabled: widget.enabled,
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
