import 'package:flex_color_picker/flex_color_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

/// Editor for a 0–1 RGB `Vec3` that a `FieldEditorHint.Color` annotation asks
/// to render as a color (`doc/design_array_node_and_field_hints.md` §Hint
/// widgets). Two tiers:
///
///   - **Inline row** (always visible): a rounded swatch button next to three
///     compact 0–1 R/G/B [FloatInput]s. The float fields are the
///     **authoritative** editing surface — the wire value is an exact `Vec3`,
///     so precise numeric entry never requires the picker. UI-entered values
///     clamp to [0, 1]; an out-of-range stored or wired value still renders
///     (clamped for the swatch preview only), per the cosmetic-hint invariant.
///   - **Picker dialog** (clicking the swatch): a [DraggableDialog] hosting
///     `flex_color_picker`'s wheel + sliders + hex entry. Live-commit, no
///     Apply/Cancel — same convention as `showTypeEditorDialog`; Ctrl+Z handles
///     regret. Opacity is disabled: the hint targets `Vec3` RGB, and alpha is a
///     separate field with its own `Range` hint (as in `StyleRule`).
///
/// **Quantization caveat:** pickers operate in 8-bit color, so a picked value
/// lands on an n/255 grid point. That is fine for visual choice; exact floats
/// are entered in the R/G/B fields, which the dialog never overwrites unless
/// the user actually picks.
class ColorFieldWidget extends StatelessWidget {
  /// Current value; components are the 0–1 RGB channels.
  final APIVec3 value;

  final ValueChanged<APIVec3> onChanged;

  /// Test-key prefix for the swatch and the three float fields.
  final String? keyPrefix;

  const ColorFieldWidget({
    super.key,
    required this.value,
    required this.onChanged,
    this.keyPrefix,
  });

  static double _clamp01(double v) => v.isNaN ? 0.0 : v.clamp(0.0, 1.0);

  /// Preview-only conversion — clamps so an out-of-range stored value still
  /// paints something rather than throwing.
  static Color _toColor(APIVec3 v) => Color.fromARGB(
        255,
        (_clamp01(v.x) * 255).round(),
        (_clamp01(v.y) * 255).round(),
        (_clamp01(v.z) * 255).round(),
      );

  static APIVec3 _fromColor(Color c) => APIVec3(
        x: (c.r * 255).round() / 255.0,
        y: (c.g * 255).round() / 255.0,
        z: (c.b * 255).round() / 255.0,
      );

  void _emit({double? r, double? g, double? b}) {
    onChanged(APIVec3(
      x: _clamp01(r ?? value.x),
      y: _clamp01(g ?? value.y),
      z: _clamp01(b ?? value.z),
    ));
  }

  Future<void> _openPicker(BuildContext context) async {
    await showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (context) => DraggableDialog(
        width: 400,
        dismissible: true,
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: _PickerBody(
            initial: _toColor(value),
            onPicked: (c) => onChanged(_fromColor(c)),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Tooltip(
          message: 'Pick a color',
          child: InkWell(
            key: keyPrefix == null ? null : Key('${keyPrefix}_swatch'),
            onTap: () => _openPicker(context),
            borderRadius: BorderRadius.circular(4),
            child: Container(
              width: 32,
              height: 32,
              decoration: BoxDecoration(
                color: _toColor(value),
                borderRadius: BorderRadius.circular(4),
                border: Border.all(color: Colors.grey.shade600),
              ),
            ),
          ),
        ),
        const SizedBox(width: 8),
        Expanded(child: _channel(context, 'R', value.x, (v) => _emit(r: v))),
        const SizedBox(width: 4),
        Expanded(child: _channel(context, 'G', value.y, (v) => _emit(g: v))),
        const SizedBox(width: 4),
        Expanded(child: _channel(context, 'B', value.z, (v) => _emit(b: v))),
      ],
    );
  }

  Widget _channel(
    BuildContext context,
    String label,
    double v,
    ValueChanged<double> onSet,
  ) {
    return FloatInput(
      label: label,
      inputKey:
          keyPrefix == null ? null : Key('${keyPrefix}_${label.toLowerCase()}'),
      value: v,
      onChanged: onSet,
    );
  }
}

/// The dialog body: `flex_color_picker`'s bare [ColorPicker] widget (not the
/// package's own dialog helper, which would not be draggable) plus a Close
/// button. Holds the in-flight color locally so the wheel tracks the drag,
/// and calls `onPicked` on every change — edits commit live.
class _PickerBody extends StatefulWidget {
  final Color initial;
  final ValueChanged<Color> onPicked;

  const _PickerBody({required this.initial, required this.onPicked});

  @override
  State<_PickerBody> createState() => _PickerBodyState();
}

class _PickerBodyState extends State<_PickerBody> {
  late Color _color = widget.initial;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Pick a color', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 12),
        ColorPicker(
          color: _color,
          onColorChanged: (c) {
            setState(() => _color = c);
            widget.onPicked(c);
          },
          width: 26,
          height: 26,
          borderRadius: 4,
          enableShadesSelection: true,
          // RGB is the whole payload — the hint targets a `Vec3`. Alpha is a
          // separate record field with its own `Range` hint.
          enableOpacity: false,
          showColorCode: true,
          colorCodeHasColor: true,
          pickersEnabled: const <ColorPickerType, bool>{
            ColorPickerType.both: false,
            ColorPickerType.primary: true,
            ColorPickerType.accent: false,
            ColorPickerType.wheel: true,
          },
          copyPasteBehavior: const ColorPickerCopyPasteBehavior(
            copyButton: true,
            pasteButton: true,
            editFieldCopyButton: true,
          ),
        ),
        const SizedBox(height: 12),
        Align(
          alignment: Alignment.centerRight,
          child: TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Close'),
          ),
        ),
      ],
    );
  }
}
