import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/select_element_widget.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Index of the `rules` input pin on `atom_replace`. 0 = molecule, 1 = rules.
const int _RULES_PIN_INDEX = 1;

/// Editor widget for atom_replace nodes.
/// Displays a list of element replacement rules with add/remove controls.
/// When the `rules` input pin is wired, the editor renders in a disabled
/// state — the wired value entirely replaces the stored list at eval, but
/// the stored values are preserved so they become live again on disconnect.
class AtomReplaceEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIAtomReplaceData? data;
  final StructureDesignerModel model;

  const AtomReplaceEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  List<APIAtomReplaceRule> get _rules => data?.replacements ?? [];

  bool _isRulesPinConnected() {
    final view = model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == nodeId &&
          wire.destParamIndex == BigInt.from(_RULES_PIN_INDEX)) {
        return true;
      }
    }
    return false;
  }

  void _updateRules(List<APIAtomReplaceRule> rules) {
    model.setAtomReplaceData(
      nodeId,
      APIAtomReplaceData(replacements: rules),
    );
  }

  void _addRule() {
    final rules = List<APIAtomReplaceRule>.from(_rules);
    // Default: Carbon (6) → Carbon (6) — no-op until user changes the target
    rules.add(const APIAtomReplaceRule(fromAtomicNumber: 6, toAtomicNumber: 6));
    _updateRules(rules);
  }

  void _removeRule(int index) {
    final rules = List<APIAtomReplaceRule>.from(_rules);
    rules.removeAt(index);
    _updateRules(rules);
  }

  void _updateFrom(int index, int atomicNumber) {
    final rules = List<APIAtomReplaceRule>.from(_rules);
    rules[index] = APIAtomReplaceRule(
      fromAtomicNumber: atomicNumber,
      toAtomicNumber: rules[index].toAtomicNumber,
    );
    _updateRules(rules);
  }

  void _updateTo(int index, int atomicNumber) {
    final rules = List<APIAtomReplaceRule>.from(_rules);
    rules[index] = APIAtomReplaceRule(
      fromAtomicNumber: rules[index].fromAtomicNumber,
      toAtomicNumber: atomicNumber,
    );
    _updateRules(rules);
  }

  @override
  Widget build(BuildContext context) {
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final rulesConnected = _isRulesPinConnected();

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Atom Replace',
            nodeTypeName: 'atom_replace',
          ),
          const SizedBox(height: 16),

          if (rulesConnected)
            const Padding(
              padding: EdgeInsets.only(bottom: 8.0),
              child: Text(
                'Rules supplied by `rules` input. Disconnect to edit inline.',
                style: TextStyle(fontStyle: FontStyle.italic, fontSize: 12),
              ),
            ),

          // Replacement rules list
          if (_rules.isEmpty && !rulesConnected)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 8.0),
              child: Text(
                'No replacement rules. Structure passes through unchanged.',
                style: TextStyle(color: Colors.grey),
              ),
            ),

          for (int i = 0; i < _rules.length; i++)
            _ReplacementRuleRow(
              key: ValueKey('rule_$i'),
              rule: _rules[i],
              enabled: !rulesConnected,
              onFromChanged: (value) => _updateFrom(i, value),
              onToChanged: (value) => _updateTo(i, value),
              onDelete: () => _removeRule(i),
            ),

          const SizedBox(height: 8),

          // Add button
          SizedBox(
            width: double.infinity,
            child: OutlinedButton.icon(
              icon: const Icon(Icons.add, size: 18),
              label: const Text('Add Replacement'),
              onPressed: rulesConnected ? null : _addRule,
            ),
          ),
        ],
      ),
    );
  }
}

/// A single replacement rule row: [source dropdown] → [target dropdown] [delete]
class _ReplacementRuleRow extends StatelessWidget {
  final APIAtomReplaceRule rule;
  final bool enabled;
  final ValueChanged<int> onFromChanged;
  final ValueChanged<int> onToChanged;
  final VoidCallback onDelete;

  const _ReplacementRuleRow({
    super.key,
    required this.rule,
    required this.enabled,
    required this.onFromChanged,
    required this.onToChanged,
    required this.onDelete,
  });

  @override
  Widget build(BuildContext context) {
    final fromDropdown = SelectElementWidget(
      value: rule.fromAtomicNumber,
      required: true,
      onChanged: (value) {
        if (value != null) onFromChanged(value);
      },
    );
    final toDropdown = SelectElementWidget(
      value: rule.toAtomicNumber == 0 ? null : rule.toAtomicNumber,
      required: false,
      nullLabel: 'Delete',
      onChanged: (value) {
        onToChanged(value ?? 0);
      },
    );

    return Padding(
      padding: const EdgeInsets.only(bottom: 8.0),
      child: Opacity(
        opacity: enabled ? 1.0 : 0.5,
        child: Row(
          children: [
            // Source element dropdown
            Expanded(
              child: IgnorePointer(ignoring: !enabled, child: fromDropdown),
            ),
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 4.0),
              child: Text('→', style: TextStyle(fontSize: 16)),
            ),
            // Target element dropdown (with Delete option at atomic number 0)
            Expanded(
              child: IgnorePointer(ignoring: !enabled, child: toDropdown),
            ),
            // Delete button
            IconButton(
              icon: const Icon(Icons.close, size: 18),
              onPressed: enabled ? onDelete : null,
              padding: EdgeInsets.zero,
              constraints: const BoxConstraints(minWidth: 32, minHeight: 32),
              tooltip: 'Remove rule',
            ),
          ],
        ),
      ),
    );
  }
}
