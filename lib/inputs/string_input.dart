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
        // When focus is lost, commit the value.
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

  /// Notify the owner, but **only when the text actually differs from the
  /// value the owner already holds**.
  ///
  /// Focus loss fires on every click elsewhere in the app, not only after an
  /// edit, so an unguarded call made "the user looked at this field" and "the
  /// user changed this field" indistinguishable. That is not harmless: several
  /// `onChanged` handlers rebuild their node's data from scratch, and the
  /// import nodes hold a parsed file payload that is *not* part of that data
  /// (it is `#[serde(skip)]` and megabytes in size). A redundant write threw
  /// the payload away, leaving `import_cube` reporting "No cube file imported"
  /// for a file it had loaded seconds earlier — and because the payload is
  /// absent from the undo snapshot, no undo entry was pushed and nothing in
  /// the UI explained it.
  ///
  /// The Rust setters now preserve their payload across an unchanged name too,
  /// so this is one of two independent guards; it is also what keeps a stray
  /// click from marking the project dirty.
  void _updateValueFromText(String text) {
    if (text == widget.value) return;
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
