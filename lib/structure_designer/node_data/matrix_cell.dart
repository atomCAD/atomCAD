import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:provider/provider.dart';

const double MATRIX_CELL_WIDTH = 56.0;
const double MATRIX_CELL_HEIGHT = 28.0;

/// Compact integer cell used inside a matrix-grid editor. Matches the inline
/// cell used by `SupercellEditor`: numeric input + scroll-to-step + shift-to-step-by-10.
class IntMatrixCell extends StatefulWidget {
  final int value;
  final bool enabled;
  final ValueChanged<int> onChanged;
  final Key? inputKey;

  const IntMatrixCell({
    super.key,
    required this.value,
    required this.enabled,
    required this.onChanged,
    this.inputKey,
  });

  @override
  State<IntMatrixCell> createState() => _IntMatrixCellState();
}

class _IntMatrixCellState extends State<IntMatrixCell> {
  late TextEditingController _controller;
  late FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value.toString());
    _focusNode = FocusNode();
    _focusNode.addListener(() {
      if (!_focusNode.hasFocus) {
        _commit(_controller.text);
      }
    });
  }

  @override
  void didUpdateWidget(IntMatrixCell oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value) {
      final selection = _controller.selection;
      _controller.text = widget.value.toString();
      final newLen = _controller.text.length;
      if (selection.isValid && selection.end <= newLen) {
        _controller.selection = selection;
      } else {
        _controller.selection = TextSelection.collapsed(offset: newLen);
      }
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _commit(String text) {
    final parsed = int.tryParse(text);
    if (parsed == null) {
      _controller.text = widget.value.toString();
      _controller.selection = TextSelection.fromPosition(
          TextPosition(offset: _controller.text.length));
      return;
    }
    if (parsed != widget.value) {
      widget.onChanged(parsed);
    }
  }

  void _step(int delta) {
    if (!widget.enabled) return;
    final current = int.tryParse(_controller.text) ?? widget.value;
    final next = current + delta;
    _controller.text = next.toString();
    widget.onChanged(next);
  }

  @override
  Widget build(BuildContext context) {
    final field = SizedBox(
      width: MATRIX_CELL_WIDTH,
      height: MATRIX_CELL_HEIGHT,
      child: TextField(
        key: widget.inputKey,
        controller: _controller,
        focusNode: _focusNode,
        enabled: widget.enabled,
        textAlign: TextAlign.center,
        style: AppTextStyles.inputField,
        keyboardType: TextInputType.number,
        decoration: AppInputDecorations.standard,
        onSubmitted: _commit,
      ),
    );

    if (!widget.enabled) {
      return Opacity(opacity: 0.5, child: field);
    }

    return MouseRegion(
      onEnter: (PointerEnterEvent event) {
        try {
          context.read<MouseWheelBlockService>().block();
        } catch (_) {}
      },
      onExit: (PointerExitEvent event) {
        try {
          context.read<MouseWheelBlockService>().unblock();
        } catch (_) {}
      },
      child: Listener(
        onPointerSignal: (event) {
          if (event is PointerScrollEvent) {
            final step = HardwareKeyboard.instance.isShiftPressed ? 10 : 1;
            if (event.scrollDelta.dy > 0) {
              _step(-step);
            } else if (event.scrollDelta.dy < 0) {
              _step(step);
            }
          }
        },
        child: field,
      ),
    );
  }
}

/// Compact float cell used inside a matrix-grid editor. Same layout as
/// `IntMatrixCell` but accepts decimal input.
class FloatMatrixCell extends StatefulWidget {
  final double value;
  final bool enabled;
  final ValueChanged<double> onChanged;
  final Key? inputKey;

  const FloatMatrixCell({
    super.key,
    required this.value,
    required this.enabled,
    required this.onChanged,
    this.inputKey,
  });

  @override
  State<FloatMatrixCell> createState() => _FloatMatrixCellState();
}

class _FloatMatrixCellState extends State<FloatMatrixCell> {
  late TextEditingController _controller;
  late FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: _format(widget.value));
    _focusNode = FocusNode();
    _focusNode.addListener(() {
      if (!_focusNode.hasFocus) {
        _commit(_controller.text);
      }
    });
  }

  @override
  void didUpdateWidget(FloatMatrixCell oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value) {
      final selection = _controller.selection;
      _controller.text = _format(widget.value);
      final newLen = _controller.text.length;
      if (selection.isValid && selection.end <= newLen) {
        _controller.selection = selection;
      } else {
        _controller.selection = TextSelection.collapsed(offset: newLen);
      }
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  // Match the user's typed precision when round-tripping through Dart's
  // default `toString` — `1.0.toString()` returns "1.0", which is fine.
  String _format(double v) => v.toString();

  void _commit(String text) {
    final parsed = double.tryParse(text);
    if (parsed == null) {
      _controller.text = _format(widget.value);
      _controller.selection = TextSelection.fromPosition(
          TextPosition(offset: _controller.text.length));
      return;
    }
    if (parsed != widget.value) {
      widget.onChanged(parsed);
    }
  }

  void _step(double delta) {
    if (!widget.enabled) return;
    final current = double.tryParse(_controller.text) ?? widget.value;
    final next = current + delta;
    _controller.text = _format(next);
    widget.onChanged(next);
  }

  @override
  Widget build(BuildContext context) {
    final field = SizedBox(
      width: MATRIX_CELL_WIDTH,
      height: MATRIX_CELL_HEIGHT,
      child: TextField(
        key: widget.inputKey,
        controller: _controller,
        focusNode: _focusNode,
        enabled: widget.enabled,
        textAlign: TextAlign.center,
        style: AppTextStyles.inputField,
        keyboardType: const TextInputType.numberWithOptions(decimal: true),
        decoration: AppInputDecorations.standard,
        onSubmitted: _commit,
      ),
    );

    if (!widget.enabled) {
      return Opacity(opacity: 0.5, child: field);
    }

    return MouseRegion(
      onEnter: (PointerEnterEvent event) {
        try {
          context.read<MouseWheelBlockService>().block();
        } catch (_) {}
      },
      onExit: (PointerExitEvent event) {
        try {
          context.read<MouseWheelBlockService>().unblock();
        } catch (_) {}
      },
      child: Listener(
        onPointerSignal: (event) {
          if (event is PointerScrollEvent) {
            final step = HardwareKeyboard.instance.isShiftPressed ? 1.0 : 0.1;
            if (event.scrollDelta.dy > 0) {
              _step(-step);
            } else if (event.scrollDelta.dy < 0) {
              _step(step);
            }
          }
        },
        child: field,
      ),
    );
  }
}
