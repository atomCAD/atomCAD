import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Property panel for the `apply` node.
///
/// `apply` has no editable shape state of its own: when `f` is wired the arg
/// pins and output type are derived from the wired source's flat function
/// type; when `f` is disconnected only the `f` pin renders and no arg pins
/// exist to call against. So this panel is *informational only* — a header
/// plus a single placard summarising the current state. See
/// `doc/design_function_pin_unification.md` (Phase D).
class ApplyEditor extends StatelessWidget {
  final NodeView node;

  const ApplyEditor({super.key, required this.node});

  @override
  Widget build(BuildContext context) {
    final wired = node.derivedShape?.derivedFromInputPin != null;
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Apply Properties',
            nodeTypeName: 'apply',
          ),
          const SizedBox(height: 12),
          _ApplyPlacard(
            wired: wired,
            derivedSummary: wired ? _formatDerivedSummary(node) : null,
          ),
        ],
      ),
    );
  }

  /// Build a short read-only summary of the derived shape from the resolved
  /// pins: `f: (T0, T1, …) → R`. Reads the wired-f-derived arg pins (skip the
  /// leading `f` pin) and the output pin's effective type. Falls back to a
  /// minimal hint if the per-pin info isn't populated yet.
  static String _formatDerivedSummary(NodeView node) {
    final argPins = node.inputPins.skip(1).toList(growable: false);
    final params = argPins.map((p) => p.dataType).join(', ');
    final returnType = node.outputPins.isNotEmpty
        ? (node.outputPins.first.resolvedDataType ??
            node.outputPins.first.dataType)
        : '?';
    return '($params) → $returnType';
  }
}

class _ApplyPlacard extends StatelessWidget {
  final bool wired;
  final String? derivedSummary;

  const _ApplyPlacard({required this.wired, this.derivedSummary});

  @override
  Widget build(BuildContext context) {
    final icon = wired ? Icons.check_circle_outline : Icons.info_outline;
    final iconColor = wired ? Colors.white70 : Colors.white60;
    final title = wired
        ? 'Shape derived from wired `f`'
        : 'Wire a function value into `f`';
    final body = wired
        ? (derivedSummary ?? 'Arg pins materialised from the wired source.')
        : 'Arg pins materialise once `f` is wired — '
            'connect a `closure` output, a node\'s function pin, or a '
            'subnetwork\'s `Function` output.';

    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: Colors.white12,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: Colors.white24),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 20, color: iconColor),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: const TextStyle(
                    color: Colors.white,
                    fontWeight: FontWeight.w500,
                  ),
                ),
                const SizedBox(height: 6),
                Text(
                  body,
                  style: TextStyle(
                    color: Colors.white70,
                    fontFamily: wired ? 'monospace' : null,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
