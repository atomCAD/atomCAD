import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/int_spin_field.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

/// A labelled IVec2 input: two [IntSpinField]s in a row, one per axis.
///
/// Like [IVec3Input] the axis boxes carry no `−` / `+` buttons; wheel and
/// arrow-key stepping still work on each box.
class IVec2Input extends StatelessWidget {
  final String label;
  final APIIVec2 value;
  final ValueChanged<APIIVec2> onChanged;
  final APIIVec2? minimumValue;
  final APIIVec2? maximumValue;

  const IVec2Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.minimumValue,
    this.maximumValue,
  });

  Widget _axisField({
    required String axisLabel,
    required Color axisColor,
    required int axisValue,
    required int? axisMin,
    required int? axisMax,
    required APIIVec2 Function(int) compose,
  }) {
    return IntSpinField(
      value: axisValue,
      minimumValue: axisMin,
      maximumValue: axisMax,
      showStepButtons: false,
      fieldConstraints: AppSpacing.inputFieldConstraints,
      decoration: axisInputDecoration(axisLabel, axisColor),
      onChanged: (v) => onChanged(compose(v)),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: AppTextStyles.label),
        const SizedBox(height: 4),
        Row(
          children: [
            Flexible(
              child: _axisField(
                axisLabel: 'X',
                axisColor: AppColors.xAxisColor,
                axisValue: value.x,
                axisMin: minimumValue?.x,
                axisMax: maximumValue?.x,
                compose: (v) => APIIVec2(x: v, y: value.y),
              ),
            ),
            const SizedBox(width: 4),
            Flexible(
              child: _axisField(
                axisLabel: 'Y',
                axisColor: AppColors.yAxisColor,
                axisValue: value.y,
                axisMin: minimumValue?.y,
                axisMax: maximumValue?.y,
                compose: (v) => APIIVec2(x: value.x, y: v),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
