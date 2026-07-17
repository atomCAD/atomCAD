import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// The generic **Function output** section of the property sidebar: one row per
/// input pin, letting the user override how that pin participates in the node's
/// `-1` function-pin view.
///
/// Unlike the per-node-type editors this section is not keyed on a node type —
/// it renders for every selected node with at least one input pin, below the
/// node's own editor. Roles only matter once something consumes the node's
/// function pin, so the section is collapsed by default until it does (or until
/// a non-Auto role is already stored).
///
/// The `effective` disposition shown under each selector is computed Rust-side
/// by the shared `function_pin_dispositions` helper and rendered verbatim: the
/// partition table lives in exactly one place, and this widget must not
/// re-derive it. See `doc/design_function_pin_roles.md`.
class FunctionOutputEditor extends StatelessWidget {
  final NodeView node;
  final StructureDesignerModel model;

  const FunctionOutputEditor({
    super.key,
    required this.node,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    final rows = _visibleRows();
    if (rows.isEmpty) return const SizedBox.shrink();

    final anyOverridden =
        rows.any((r) => r.view.role != APIFunctionPinRole.auto);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8.0),
      child: Theme(
        // The default divider lines read as panel separators here, which is
        // misleading for a section nested under a node editor.
        data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
        child: ExpansionTile(
          key: ValueKey('function_output_${node.id}'),
          initiallyExpanded: node.functionPinConsumed || anyOverridden,
          tilePadding: EdgeInsets.zero,
          childrenPadding: const EdgeInsets.only(bottom: 8.0),
          title: const Text(
            'Function output',
            style: TextStyle(fontWeight: FontWeight.w500),
          ),
          subtitle: Text(
            node.functionPinConsumed
                ? 'Used as a function: ${node.functionType}'
                : 'Inert until this node\'s ƒ pin is wired',
            style: const TextStyle(fontSize: 11, color: Colors.white54),
          ),
          children: [
            const _SectionHint(),
            const SizedBox(height: 8),
            for (final row in rows)
              _PinRoleRow(
                view: row.view,
                onChanged: (role) =>
                    model.setFunctionPinRole(node.id, row.pinIndex, role),
              ),
          ],
        ),
      ),
    );
  }

  /// Fetch the per-pin rows, or nothing for the node types where a role cannot
  /// mean anything (design doc, open question 1):
  ///
  /// - **Zone-owning nodes** (`map` / `filter` / `fold` / `foreach` /
  ///   `zip_with` / `closure`) don't render a function pin at all — see the
  ///   `if (!isHof)` gate in `node_network/node_widget.dart`. With nothing able
  ///   to consume their `-1` pin, roles are unreachable, so offering them here
  ///   would be a dead control.
  /// - **`apply`**'s pins are derived from its wired `f` rather than authored.
  List<({int pinIndex, APIFunctionPinRoleView view})> _visibleRows() {
    if (node.zone != null || node.nodeTypeName == 'apply') return const [];
    final views = sd_api.getFunctionPinRoles(
      scopePath: model.propertyEditorScopePath,
      nodeId: node.id,
    );
    return [
      for (var i = 0; i < views.length; i++) (pinIndex: i, view: views[i]),
    ];
  }
}

class _SectionHint extends StatelessWidget {
  const _SectionHint();

  @override
  Widget build(BuildContext context) {
    return const Text(
      'Choose which inputs stay as parameters of the function this node '
      'exposes on its ƒ pin, and which are baked in. A Delayed pin\'s wire '
      'becomes a preview: it drives this node\'s own output and types, but is '
      'ignored when the function is called.',
      style: TextStyle(fontSize: 11, color: Colors.white60),
    );
  }
}

class _PinRoleRow extends StatelessWidget {
  final APIFunctionPinRoleView view;
  final ValueChanged<APIFunctionPinRole> onChanged;

  const _PinRoleRow({required this.view, required this.onChanged});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 10.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  view.pinName,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontWeight: FontWeight.w500),
                ),
              ),
              if (view.wired)
                const Tooltip(
                  message: 'This pin has an incoming wire',
                  child: Icon(Icons.link, size: 14, color: Colors.white54),
                ),
            ],
          ),
          const SizedBox(height: 4),
          SizedBox(
            width: double.infinity,
            child: SegmentedButton<APIFunctionPinRole>(
              segments: const [
                ButtonSegment(
                  value: APIFunctionPinRole.auto,
                  label: Text('Auto'),
                  tooltip: 'Wiring decides: unwired = parameter, wired = '
                      'captured',
                ),
                ButtonSegment(
                  value: APIFunctionPinRole.delayed,
                  label: Text('Delayed'),
                  tooltip: 'Always a parameter. Any wire is preview-only.',
                ),
                ButtonSegment(
                  value: APIFunctionPinRole.supplied,
                  label: Text('Supplied'),
                  tooltip: 'Never a parameter. Unwired, the stored property '
                      'value is baked in.',
                ),
              ],
              selected: {view.role},
              showSelectedIcon: false,
              style: const ButtonStyle(
                visualDensity: VisualDensity.compact,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                textStyle: WidgetStatePropertyAll(TextStyle(fontSize: 11)),
              ),
              onSelectionChanged: (selection) => onChanged(selection.first),
            ),
          ),
          const SizedBox(height: 3),
          Text(
            _effectiveLabel(view.effective),
            style: const TextStyle(
              fontSize: 11,
              fontStyle: FontStyle.italic,
              color: Colors.white54,
            ),
          ),
        ],
      ),
    );
  }

  static String _effectiveLabel(APIFunctionPinDisposition effective) {
    switch (effective) {
      case APIFunctionPinDisposition.parameter:
        return 'parameter';
      case APIFunctionPinDisposition.captureWire:
        return 'captures wire';
      case APIFunctionPinDisposition.captureStored:
        return 'uses stored value';
    }
  }
}
