import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable widget for editing Vec2 (floating point) values
class Vec2Input extends StatefulWidget {
  final String label;
  final APIVec2 value;
  final ValueChanged<APIVec2> onChanged;

  /// Optional callback triggered when a value is successfully pasted
  final VoidCallback? onPasted;

  const Vec2Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.onPasted,
  });

  @override
  State<Vec2Input> createState() => _Vec2InputState();
}

class _Vec2InputState extends State<Vec2Input> {
  late TextEditingController _xController;
  late TextEditingController _yController;

  // Add FocusNodes to track focus
  late FocusNode _xFocus;
  late FocusNode _yFocus;

  // Track which field is currently being edited
  String? _currentlyEditingAxis;

  @override
  void initState() {
    super.initState();
    _xController =
        TextEditingController(text: widget.value.x.toStringAsFixed(6));
    _yController =
        TextEditingController(text: widget.value.y.toStringAsFixed(6));

    // Initialize focus nodes
    _xFocus = FocusNode();
    _yFocus = FocusNode();

    // Add listeners to focus nodes
    _xFocus.addListener(_handleXFocusChange);
    _yFocus.addListener(_handleYFocusChange);
  }

  // Handle focus changes
  void _handleXFocusChange() {
    if (_xFocus.hasFocus) {
      _currentlyEditingAxis = 'x';
    } else if (_currentlyEditingAxis == 'x') {
      _currentlyEditingAxis = null;
    }
  }

  void _handleYFocusChange() {
    if (_yFocus.hasFocus) {
      _currentlyEditingAxis = 'y';
    } else if (_currentlyEditingAxis == 'y') {
      _currentlyEditingAxis = null;
    }
  }

  @override
  void didUpdateWidget(Vec2Input oldWidget) {
    super.didUpdateWidget(oldWidget);

    // Only update controllers if NOT currently editing this specific axis
    if (_currentlyEditingAxis != 'x' && oldWidget.value.x != widget.value.x) {
      final selection = _xController.selection;
      _xController.text = widget.value.x.toStringAsFixed(6);
      _xController.selection = selection;
    }

    if (_currentlyEditingAxis != 'y' && oldWidget.value.y != widget.value.y) {
      final selection = _yController.selection;
      _yController.text = widget.value.y.toStringAsFixed(6);
      _yController.selection = selection;
    }
  }

  @override
  void dispose() {
    _xController.dispose();
    _yController.dispose();

    // Clean up focus nodes
    _xFocus.removeListener(_handleXFocusChange);
    _yFocus.removeListener(_handleYFocusChange);
    _xFocus.dispose();
    _yFocus.dispose();

    super.dispose();
  }

  void _applyValueChange(String axis) {
    final newValue = double.tryParse(_getController(axis).text);
    if (newValue != null) {
      switch (axis) {
        case 'x':
          widget.onChanged(
              APIVec2(x: newValue, y: widget.value.y));
          break;
        case 'y':
          widget.onChanged(
              APIVec2(x: widget.value.x, y: newValue));
          break;
      }
    }
  }

  TextEditingController _getController(String axis) {
    switch (axis) {
      case 'x':
        return _xController;
      case 'y':
        return _yController;
      default:
        throw Exception('Invalid axis');
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label, style: AppTextStyles.label),
        const SizedBox(height: 4),
        Row(
          children: [
            Expanded(
              child: Focus(
                onFocusChange: (hasFocus) {
                  if (!hasFocus && _currentlyEditingAxis == 'x') {
                    _applyValueChange('x');
                    _currentlyEditingAxis = null;
                  }
                },
                child: TextField(
                  controller: _xController,
                  focusNode: _xFocus,
                  style: AppTextStyles.inputField,
                  decoration: AppInputDecorations.standard.copyWith(
                    labelText: 'X',
                    labelStyle: TextStyle(
                      fontSize: 13,
                      color: AppColors.xAxisColor,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  keyboardType:
                      const TextInputType.numberWithOptions(decimal: true),
                  onChanged: (value) {
                    // Mark that we're editing this field
                    _currentlyEditingAxis = 'x';
                  },
                  onSubmitted: (value) {
                    _applyValueChange('x');
                    _currentlyEditingAxis = null;
                  },
                ),
              ),
            ),
            const SizedBox(width: 4),
            Expanded(
              child: Focus(
                onFocusChange: (hasFocus) {
                  if (!hasFocus && _currentlyEditingAxis == 'y') {
                    _applyValueChange('y');
                    _currentlyEditingAxis = null;
                  }
                },
                child: TextField(
                  controller: _yController,
                  focusNode: _yFocus,
                  style: AppTextStyles.inputField,
                  decoration: AppInputDecorations.standard.copyWith(
                    labelText: 'Y',
                    labelStyle: TextStyle(
                      fontSize: 13,
                      color: AppColors.yAxisColor,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  keyboardType:
                      const TextInputType.numberWithOptions(decimal: true),
                  onChanged: (value) {
                    // Mark that we're editing this field
                    _currentlyEditingAxis = 'y';
                  },
                  onSubmitted: (value) {
                    _applyValueChange('y');
                    _currentlyEditingAxis = null;
                  },
                ),
              ),
            ),
            const SizedBox(width: 4),
            // Copy button
            SizedBox(
              width: AppSpacing.smallButtonWidth / 2,
              height: AppSpacing.buttonHeight,
              child: Tooltip(
                message: 'Copy',
                child: ElevatedButton(
                  style: AppButtonStyles.primary,
                  onPressed: _copyToClipboard,
                  child: const Text('C', style: AppTextStyles.buttonText),
                ),
              ),
            ),
            const SizedBox(width: 4),
            // Paste button
            SizedBox(
              width: AppSpacing.smallButtonWidth / 2,
              height: AppSpacing.buttonHeight,
              child: Tooltip(
                message: 'Paste',
                child: ElevatedButton(
                  style: AppButtonStyles.primary,
                  onPressed: _pasteFromClipboard,
                  child: const Text('P', style: AppTextStyles.buttonText),
                ),
              ),
            ),
          ],
        ),
      ],
    );
  }

  // Copy the current values to the clipboard as space-separated string
  void _copyToClipboard() {
    final String value =
        '${widget.value.x.toStringAsFixed(6)} ${widget.value.y.toStringAsFixed(6)}';
    Clipboard.setData(ClipboardData(text: value));
  }

  // Parse space-separated values from clipboard and update the widget
  void _pasteFromClipboard() async {
    final ClipboardData? clipboardData =
        await Clipboard.getData(Clipboard.kTextPlain);
    if (clipboardData != null && clipboardData.text != null) {
      final String text = clipboardData.text!;
      final List<String> parts = text.trim().split(RegExp(r'\s+'));

      if (parts.length >= 2) {
        final double? x = double.tryParse(parts[0]);
        final double? y = double.tryParse(parts[1]);

        if (x != null && y != null) {
          widget.onChanged(APIVec2(x: x, y: y));

          // Notify parent that a value was successfully pasted
          if (widget.onPasted != null) {
            widget.onPasted!();
          }
        }
      }
    }
  }
}
