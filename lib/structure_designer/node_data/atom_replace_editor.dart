import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/select_element_widget.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for atom_replace nodes.
/// Displays a list of element replacement rules with add/remove controls.
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

          // Replacement rules list
          if (_rules.isEmpty)
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
              onPressed: _addRule,
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
  final ValueChanged<int> onFromChanged;
  final ValueChanged<int> onToChanged;
  final VoidCallback onDelete;

  const _ReplacementRuleRow({
    super.key,
    required this.rule,
    required this.onFromChanged,
    required this.onToChanged,
    required this.onDelete,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8.0),
      child: Row(
        children: [
          // Source element dropdown
          Expanded(
            child: SelectElementWidget(
              value: rule.fromAtomicNumber,
              required: true,
              onChanged: (value) {
                if (value != null) onFromChanged(value);
              },
            ),
          ),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 4.0),
            child: Text('→', style: TextStyle(fontSize: 16)),
          ),
          // Target element dropdown (with Delete option at atomic number 0)
          Expanded(
            child: SelectElementWidget(
              value: rule.toAtomicNumber == 0 ? null : rule.toAtomicNumber,
              required: false,
              nullLabel: 'Delete',
              onChanged: (value) {
                onToChanged(value ?? 0);
              },
            ),
          ),
          // Delete button
          IconButton(
            icon: const Icon(Icons.close, size: 18),
            onPressed: onDelete,
            padding: EdgeInsets.zero,
            constraints: const BoxConstraints(minWidth: 32, minHeight: 32),
            tooltip: 'Remove rule',
          ),
        ],
      ),
    );
  }
}
