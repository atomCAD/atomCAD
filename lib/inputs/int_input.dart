import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/int_spin_field.dart';

/// A labelled integer input: a caption over an [IntSpinField] with its
/// `−` / `+` buttons. All the editing behaviour lives in the spin field.
///
/// At its widest the widget is [AppSpacing.intSpinFieldMaxWidth] +
/// [AppSpacing.intSpinChromeWidth] wide; a caller that pins the width must
/// leave at least [AppSpacing.intSpinFieldMinWidth] + the chrome, or the row
/// overflows.
class IntInput extends StatelessWidget {
  final String label;
  final int value;
  final ValueChanged<int> onChanged;
  final int? minimumValue;
  final int? maximumValue;
  final Key? inputKey;

  const IntInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.minimumValue,
    this.maximumValue,
    this.inputKey,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: AppTextStyles.label),
        const SizedBox(height: 4),
        IntSpinField(
          value: value,
          onChanged: onChanged,
          minimumValue: minimumValue,
          maximumValue: maximumValue,
          inputKey: inputKey,
        ),
      ],
    );
  }
}
