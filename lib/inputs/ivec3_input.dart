import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/int_spin_field.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

/// A labelled IVec3 input: three [IntSpinField]s in a row, one per axis.
///
/// The axis boxes carry no `−` / `+` buttons — three of them already fill the
/// 300 px property panel, and the buttons would cost 52 px per box. Wheel and
/// arrow-key stepping still work on each box.
class IVec3Input extends StatelessWidget {
  final String label;
  final APIIVec3 value;
  final ValueChanged<APIIVec3> onChanged;
  final APIIVec3? minimumValue;
  final APIIVec3? maximumValue;

  /// Optional keys for individual axis text fields (for testing)
  final Key? xInputKey;
  final Key? yInputKey;
  final Key? zInputKey;

  const IVec3Input({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.minimumValue,
    this.maximumValue,
    this.xInputKey,
    this.yInputKey,
    this.zInputKey,
  });

  Widget _axisField({
    required String axisLabel,
    required Color axisColor,
    required int axisValue,
    required int? axisMin,
    required int? axisMax,
    required APIIVec3 Function(int) compose,
    Key? inputKey,
  }) {
    return IntSpinField(
      value: axisValue,
      minimumValue: axisMin,
      maximumValue: axisMax,
      showStepButtons: false,
      fieldConstraints: AppSpacing.inputFieldConstraints,
      decoration: axisInputDecoration(axisLabel, axisColor),
      inputKey: inputKey,
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
                compose: (v) => APIIVec3(x: v, y: value.y, z: value.z),
                inputKey: xInputKey,
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
                compose: (v) => APIIVec3(x: value.x, y: v, z: value.z),
                inputKey: yInputKey,
              ),
            ),
            const SizedBox(width: 4),
            Flexible(
              child: _axisField(
                axisLabel: 'Z',
                axisColor: AppColors.zAxisColor,
                axisValue: value.z,
                axisMin: minimumValue?.z,
                axisMax: maximumValue?.z,
                compose: (v) => APIIVec3(x: value.x, y: value.y, z: v),
                inputKey: zInputKey,
              ),
            ),
          ],
        ),
      ],
    );
  }
}
