import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Read-only display of a node's output type when it is **derived from a wired
/// `f` function value** rather than editable inline. Shared by the `map` and
/// `zip_with` editors (both switch their editable "Output Type" field to this
/// display whenever `node.derivedShape?.derivedFromInputPin == 'f'`).
///
/// The output pin's resolved type is `Iter[derived]` (or, for a partial
/// application, `Iter[Function(... → R)]`); the stored fallback field is the
/// bare inner element, so we strip the wrapping `Iter[...]` for a label that
/// lines up with the editable-mode field. See
/// `doc/design_function_pin_unification.md` (Phase D) and
/// `doc/design_zip_with.md` (Phase 5).
class DerivedOutputTypeDisplay extends StatelessWidget {
  final NodeView node;

  const DerivedOutputTypeDisplay({super.key, required this.node});

  String _displayedType() {
    if (node.outputPins.isEmpty) return '?';
    final pin = node.outputPins.first;
    final t = pin.resolvedDataType ?? pin.dataType;
    const prefix = 'Iter[';
    if (t.startsWith(prefix) && t.endsWith(']')) {
      return t.substring(prefix.length, t.length - 1);
    }
    return t;
  }

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: 'Derived from `f`. '
          'Disconnect `f` to edit the stored fallback inline.',
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        decoration: BoxDecoration(
          color: Colors.white10,
          borderRadius: BorderRadius.circular(4),
          border: Border.all(color: Colors.white24),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Icon(Icons.link, size: 16, color: Colors.white54),
            const SizedBox(width: 8),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    'Output Type',
                    style: TextStyle(color: Colors.white70, fontSize: 12),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    _displayedType(),
                    style: const TextStyle(
                      color: Colors.white,
                      fontFamily: 'monospace',
                    ),
                  ),
                  const SizedBox(height: 4),
                  const Text(
                    'derived from f',
                    style: TextStyle(
                      color: Colors.white54,
                      fontStyle: FontStyle.italic,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
