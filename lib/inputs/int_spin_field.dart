/// The one integer text box behind every integer input in the app.
///
/// `IntInput`, `IVec2Input` and `IVec3Input` are thin layouts over this widget;
/// the parsing, range clamping, mouse-wheel stepping, arrow-key stepping and
/// the optional `−` / `+` buttons all live here so they cannot drift apart.
/// Do not add a fourth copy of the increment logic — compose this instead.
///
/// **Stepping reads the text box, not the committed value.** A user who has
/// typed `12` but not yet pressed Enter and then clicks `+` expects `13`, so
/// every step parses the current text first and falls back to the committed
/// `value` only when the text is not a number. A step that lands on the value
/// already shown (at a range bound) is a no-op: it emits nothing, so a held
/// button at the bound does not spam the kernel with identical writes.
///
/// **The buttons sit beside the box, not inside it.** The dense field is only
/// about 24 px tall, so a stacked up/down spinner inside it would give each
/// half roughly 12 px of hit area — well under a comfortable click target and
/// an invitation to hit decrement when increment was meant. Side-by-side
/// buttons are the field's full height and are separated by the whole text
/// box. They exist because mouse-wheel stepping is silently dead on some
/// hardware (a user report, 2026-09); keyboard arrow keys are the third route
/// for the same reason.
library;

import 'dart:async';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:provider/provider.dart';

/// Multiplier applied to a step while SHIFT is held (wheel, keys or buttons).
const int INT_SPIN_SHIFT_MULTIPLIER = 10;

/// The standard box decoration with a bold, axis-coloured floating label —
/// what the per-axis boxes of the vector inputs use.
InputDecoration axisInputDecoration(String axisLabel, Color axisColor) {
  return AppInputDecorations.standard.copyWith(
    labelText: axisLabel,
    labelStyle: TextStyle(
      fontSize: 13,
      color: axisColor,
      fontWeight: FontWeight.bold,
    ),
  );
}

/// How long a `−` / `+` button must be held before it starts repeating, and
/// how often it repeats after that.
const Duration INT_SPIN_REPEAT_DELAY = Duration(milliseconds: 400);
const Duration INT_SPIN_REPEAT_INTERVAL = Duration(milliseconds: 60);

class IntSpinField extends StatefulWidget {
  /// The committed value. The text box tracks it (preserving the caret) when
  /// it changes from outside, and it is the fallback base for a step while the
  /// text is not a number.
  final int value;
  final ValueChanged<int> onChanged;
  final int? minimumValue;
  final int? maximumValue;

  /// Whether to render the `−` / `+` buttons on either side of the box. Off in
  /// the vector inputs, where three boxes share a 300 px panel and the 52 px
  /// per box the buttons cost would leave no room for the digits.
  final bool showStepButtons;

  /// Width limits for the text box alone (the buttons are extra).
  final BoxConstraints fieldConstraints;

  /// Decoration for the text box; callers add a floating axis label to it.
  final InputDecoration decoration;

  /// Key applied to the `TextField` itself (used by the integration tests).
  final Key? inputKey;

  const IntSpinField({
    super.key,
    required this.value,
    required this.onChanged,
    this.minimumValue,
    this.maximumValue,
    this.showStepButtons = true,
    this.fieldConstraints = AppSpacing.intSpinFieldConstraints,
    this.decoration = AppInputDecorations.standard,
    this.inputKey,
  });

  @override
  State<IntSpinField> createState() => _IntSpinFieldState();
}

class _IntSpinFieldState extends State<IntSpinField> {
  late final TextEditingController _controller;
  late final FocusNode _focusNode;

  /// The value most recently committed from this box or handed in by the
  /// parent. Enter both submits and drops focus, so two commits run per Enter
  /// before the parent has rebuilt with the new `widget.value`; comparing
  /// against this (not `widget.value`) is what makes the second one a no-op.
  late int _lastCommitted;

  @override
  void initState() {
    super.initState();
    _lastCommitted = widget.value;
    _controller = TextEditingController(text: widget.value.toString());
    _focusNode = FocusNode(onKeyEvent: _handleKey);
    _focusNode.addListener(_onFocusChanged);
  }

  @override
  void didUpdateWidget(IntSpinField oldWidget) {
    super.didUpdateWidget(oldWidget);
    _lastCommitted = widget.value;
    if (oldWidget.value != widget.value) {
      updateTextControllerWithSelection(_controller, widget.value.toString());
    }
  }

  @override
  void dispose() {
    _focusNode.removeListener(_onFocusChanged);
    _focusNode.dispose();
    _controller.dispose();
    super.dispose();
  }

  void _onFocusChanged() {
    if (!_focusNode.hasFocus) _commitText(_controller.text);
  }

  void _emit(int value) {
    _lastCommitted = value;
    widget.onChanged(value);
  }

  /// Parses `text` and checks it against the range; null means "reject".
  int? _validate(String text) {
    final value = int.tryParse(text);
    if (value == null) return null;
    final min = widget.minimumValue;
    final max = widget.maximumValue;
    if (min != null && value < min) return null;
    if (max != null && value > max) return null;
    return value;
  }

  /// Commits typed text (Enter / focus loss); invalid text snaps back to the
  /// committed value with the caret at the end.
  ///
  /// Enter both submits and drops focus, so this runs twice per Enter; a
  /// value equal to the committed one only normalises the text (`007` → `7`)
  /// and emits nothing, which keeps that second run from writing the kernel.
  void _commitText(String text) {
    final valid = _validate(text);
    if (valid != null) {
      if (valid != _lastCommitted) {
        _emit(valid);
      } else if (_controller.text != valid.toString()) {
        _controller.text = valid.toString();
      }
      return;
    }
    _controller.text = widget.value.toString();
    _controller.selection = TextSelection.collapsed(
      offset: _controller.text.length,
    );
  }

  /// Steps by `direction` (±1) times the SHIFT multiplier when SHIFT is held,
  /// clamped to the range.
  void _step(int direction) {
    final shift = HardwareKeyboard.instance.isShiftPressed;
    final magnitude = shift ? INT_SPIN_SHIFT_MULTIPLIER : 1;
    final current = int.tryParse(_controller.text) ?? widget.value;
    var next = current + direction * magnitude;
    final min = widget.minimumValue;
    final max = widget.maximumValue;
    if (min != null && next < min) next = min;
    if (max != null && next > max) next = max;
    if (next == current) return;
    // The clamp can still leave `next` outside the range when `current` was
    // typed out of range; `_validate` is the single gate for that.
    if (_validate(next.toString()) == null) return;
    _controller.text = next.toString();
    _emit(next);
  }

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is KeyUpEvent) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.arrowUp) {
      _step(1);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.arrowDown) {
      _step(-1);
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  void _handleScroll(PointerSignalEvent event) {
    if (event is! PointerScrollEvent) return;
    // Scrolling down (positive delta) decreases; scrolling up increases.
    if (event.scrollDelta.dy > 0) {
      _step(-1);
    } else if (event.scrollDelta.dy < 0) {
      _step(1);
    }
  }

  String _tooltipMessage() {
    final lines = <String>[
      widget.showStepButtons
          ? 'Click − / +, press ↑ / ↓, or use the mouse wheel to step the value'
          : 'Press ↑ / ↓ or use the mouse wheel to step the value',
      'Hold SHIFT for ${INT_SPIN_SHIFT_MULTIPLIER}x steps',
      if (widget.minimumValue != null) 'Minimum value: ${widget.minimumValue}',
      if (widget.maximumValue != null) 'Maximum value: ${widget.maximumValue}',
    ];
    return lines.join('\n');
  }

  void _setWheelBlocked(bool blocked) {
    // The service is absent in widget tests and in dialogs outside the main
    // provider tree; stepping must keep working there.
    try {
      final service = context.read<MouseWheelBlockService>();
      if (blocked) {
        service.block();
      } else {
        service.unblock();
      }
    } catch (_) {
      // Provider not available, do nothing.
    }
  }

  @override
  Widget build(BuildContext context) {
    final field = ConstrainedBox(
      constraints: widget.fieldConstraints,
      child: TextField(
        key: widget.inputKey,
        decoration: widget.decoration,
        controller: _controller,
        focusNode: _focusNode,
        keyboardType: TextInputType.number,
        style: AppTextStyles.inputField,
        onSubmitted: _commitText,
      ),
    );

    final Widget body;
    if (widget.showStepButtons) {
      // IntrinsicHeight + stretch makes the buttons exactly as tall as the
      // dense text box, whatever the theme's font metrics make that.
      body = IntrinsicHeight(
        child: Row(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _StepButton(
              icon: Icons.remove,
              semanticLabel: 'Decrease',
              onStep: () => _step(-1),
            ),
            const SizedBox(width: AppSpacing.intSpinButtonGap),
            Flexible(child: field),
            const SizedBox(width: AppSpacing.intSpinButtonGap),
            _StepButton(
              icon: Icons.add,
              semanticLabel: 'Increase',
              onStep: () => _step(1),
            ),
          ],
        ),
      );
    } else {
      body = field;
    }

    return Tooltip(
      message: _tooltipMessage(),
      preferBelow: true,
      child: MouseRegion(
        onEnter: (_) => _setWheelBlocked(true),
        onExit: (_) => _setWheelBlocked(false),
        child: Listener(
          onPointerSignal: _handleScroll,
          child: body,
        ),
      ),
    );
  }
}

/// A `−` or `+` button that fires once on press and then auto-repeats while
/// held. Built on a raw `Listener` rather than `InkWell` so the press is seen
/// on pointer-down (an `onTap` would only fire on release, which is the wrong
/// moment for a spinner) and so the repeat timer is tied to the same event.
class _StepButton extends StatefulWidget {
  final IconData icon;
  final String semanticLabel;
  final VoidCallback onStep;

  const _StepButton({
    required this.icon,
    required this.semanticLabel,
    required this.onStep,
  });

  @override
  State<_StepButton> createState() => _StepButtonState();
}

class _StepButtonState extends State<_StepButton> {
  Timer? _repeat;
  bool _hovered = false;
  bool _pressed = false;

  void _startRepeat() {
    widget.onStep();
    _repeat?.cancel();
    _repeat = Timer(INT_SPIN_REPEAT_DELAY, () {
      _repeat = Timer.periodic(
        INT_SPIN_REPEAT_INTERVAL,
        (_) => widget.onStep(),
      );
    });
  }

  void _stopRepeat() {
    _repeat?.cancel();
    _repeat = null;
  }

  @override
  void dispose() {
    _stopRepeat();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    // Matches the enabled OutlineInputBorder colour of the neighbouring box.
    final borderColor = theme.colorScheme.onSurface.withValues(alpha: 0.38);
    final Color? fill = _pressed
        ? theme.colorScheme.onSurface.withValues(alpha: 0.12)
        : _hovered
            ? theme.hoverColor
            : null;

    return Semantics(
      button: true,
      label: widget.semanticLabel,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: Listener(
          onPointerDown: (_) {
            setState(() => _pressed = true);
            _startRepeat();
          },
          onPointerUp: (_) {
            setState(() => _pressed = false);
            _stopRepeat();
          },
          onPointerCancel: (_) {
            setState(() => _pressed = false);
            _stopRepeat();
          },
          child: Container(
            width: AppSpacing.intSpinButtonWidth,
            decoration: BoxDecoration(
              color: fill,
              border: Border.all(color: borderColor),
              borderRadius: BorderRadius.circular(4),
            ),
            child: Icon(widget.icon, size: 16),
          ),
        ),
      ),
    );
  }
}
