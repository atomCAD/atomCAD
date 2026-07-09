import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for the `switch` node (select a value by matching a selector
/// against literal cases; `doc/design_switch_node.md`).
///
/// Stored state (all via the whole-data `model.setSwitchData`, which runs the
/// value-keyed id merge Rust-side so external wires follow their case):
///   - **Selector Type** — a dropdown restricted to Int / String (the only two
///     selector domains). Flipping it re-sends the current case strings; Rust
///     converts the stored cases into the new domain (ids preserved).
///   - **Value Type** — a `DataTypeInput` driving the case / `default` / output
///     pins (any concrete type, including structural).
///   - **Cases** — one text field per case (the hidden per-case stable id is
///     managed Rust-side and never surfaces here) plus a delete button
///     (disabled at one case — the minimum) and an "Add Case" button.
///
/// Case values cross the API as strings; the setter parses them per selector
/// type and returns an [APIResult] whose error (bad integer parse, duplicate
/// value, unparseable selector flip) is shown inline — nothing is mutated on
/// failure.
class SwitchEditor extends StatefulWidget {
  final BigInt nodeId;
  final APISwitchData? data;
  final StructureDesignerModel model;

  const SwitchEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<SwitchEditor> createState() => _SwitchEditorState();
}

class _SwitchEditorState extends State<SwitchEditor> {
  /// One controller/focus-node per case, kept positionally in sync with
  /// `data.caseValues`. A case field commits on focus loss / Enter.
  final List<TextEditingController> _controllers = [];
  final List<FocusNode> _focusNodes = [];

  /// Last inline error from `setSwitchData` (`null` when the edit succeeded).
  String? _error;

  bool get _isIntSelector =>
      widget.data?.selectorType.dataTypeBase == APIDataTypeBase.int;

  @override
  void initState() {
    super.initState();
    _syncControllers(widget.data?.caseValues ?? const []);
  }

  @override
  void didUpdateWidget(SwitchEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncControllers(widget.data?.caseValues ?? const []);
  }

  @override
  void dispose() {
    for (final c in _controllers) {
      c.dispose();
    }
    for (final f in _focusNodes) {
      f.dispose();
    }
    super.dispose();
  }

  /// Resize the controller/focus lists to match `values` and refresh the text
  /// of any field that is not currently being edited. Controllers are kept
  /// positionally (surviving cases keep their controller), so a field the user
  /// is typing in is never clobbered mid-edit.
  void _syncControllers(List<String> values) {
    while (_controllers.length < values.length) {
      final index = _controllers.length;
      final controller = TextEditingController();
      final focusNode = FocusNode();
      focusNode.addListener(() {
        if (!focusNode.hasFocus) {
          _commitCaseFromField(index);
        }
      });
      _controllers.add(controller);
      _focusNodes.add(focusNode);
    }
    while (_controllers.length > values.length) {
      _controllers.removeLast().dispose();
      _focusNodes.removeLast().dispose();
    }
    for (int i = 0; i < values.length; i++) {
      if (!_focusNodes[i].hasFocus && _controllers[i].text != values[i]) {
        _controllers[i].text = values[i];
      }
    }
  }

  APIDataType _selectorTypeFor(APIDataTypeBase base) => APIDataType(
        dataTypeBase: base,
        customDataType: null,
        array: false,
        children: const [],
      );

  /// Push a whole-data edit; capture the inline error (if any) for display.
  void _commit({
    APIDataType? selectorType,
    APIDataType? valueType,
    List<String>? caseValues,
  }) {
    final data = widget.data;
    if (data == null) return;
    final result = widget.model.setSwitchData(
      widget.nodeId,
      APISwitchData(
        selectorType: selectorType ?? data.selectorType,
        valueType: valueType ?? data.valueType,
        caseValues: caseValues ?? data.caseValues,
      ),
    );
    if (!mounted) return;
    setState(() {
      _error = result.success ? null : result.errorMessage;
    });
  }

  void _commitCaseFromField(int index) {
    if (!mounted) return;
    final data = widget.data;
    if (data == null || index >= data.caseValues.length) return;
    final text = _controllers[index].text;
    if (text == data.caseValues[index]) return; // no change — skip the churn
    final values = [...data.caseValues];
    values[index] = text;
    _commit(caseValues: values);
  }

  /// Default value for a freshly added case: the smallest unused non-negative
  /// integer for an Int selector, or an unused non-empty placeholder string.
  String _nextCaseValue() {
    final existing = widget.data!.caseValues.toSet();
    if (_isIntSelector) {
      var i = 0;
      while (existing.contains(i.toString())) {
        i++;
      }
      return i.toString();
    }
    var candidate = 'case';
    var i = 1;
    while (existing.contains(candidate)) {
      candidate = 'case$i';
      i++;
    }
    return candidate;
  }

  void _addCase() {
    _commit(caseValues: [...widget.data!.caseValues, _nextCaseValue()]);
  }

  void _removeCase(int index) {
    final values = [...widget.data!.caseValues]..removeAt(index);
    _commit(caseValues: values);
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final data = widget.data!;
    final caseValues = data.caseValues;
    final canDelete = caseValues.length > 1;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Switch Properties',
            nodeTypeName: 'switch',
          ),
          const SizedBox(height: 8),
          const Text(
            'Selects the case whose case literal matches the selector; the '
            '`default` pin is used when nothing matches. Only the matched '
            'branch is evaluated.',
            style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
          ),
          const SizedBox(height: 12),

          // Selector type — restricted to Int / String. An outlined,
          // floating-label field to match the Value Type field below (and stay
          // legible in both light and dark themes).
          DropdownButtonFormField<APIDataTypeBase>(
            value:
                _isIntSelector ? APIDataTypeBase.int : APIDataTypeBase.string,
            isDense: true,
            decoration: const InputDecoration(
              labelText: 'Selector Type',
              border: OutlineInputBorder(),
              contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            ),
            items: const [
              DropdownMenuItem(
                value: APIDataTypeBase.int,
                child: Text('Int'),
              ),
              DropdownMenuItem(
                value: APIDataTypeBase.string,
                child: Text('String'),
              ),
            ],
            onChanged: (base) {
              if (base == null) return;
              _commit(selectorType: _selectorTypeFor(base));
            },
          ),
          const SizedBox(height: 8),

          // Value type — any concrete type.
          DataTypeInput(
            label: 'Value Type',
            value: data.valueType,
            onChanged: (newValue) => _commit(valueType: newValue),
          ),

          const Divider(height: 24),

          const Padding(
            padding: EdgeInsets.only(bottom: 4.0),
            child: Text('Cases', style: TextStyle(color: Colors.white70)),
          ),

          // One row per case: literal field + delete button.
          for (int i = 0; i < caseValues.length; i++)
            Padding(
              padding: const EdgeInsets.only(bottom: 8.0),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  Expanded(
                    child: TextField(
                      controller: _controllers[i],
                      focusNode: _focusNodes[i],
                      keyboardType: _isIntSelector
                          ? TextInputType.number
                          : TextInputType.text,
                      inputFormatters: _isIntSelector
                          ? [
                              FilteringTextInputFormatter.allow(
                                  RegExp(r'[0-9-]'))
                            ]
                          : null,
                      decoration: const InputDecoration(
                        labelText: 'Case',
                        isDense: true,
                        border: OutlineInputBorder(),
                        contentPadding:
                            EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                      ),
                      onSubmitted: (_) => _commitCaseFromField(i),
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.delete_outline),
                    tooltip: canDelete
                        ? 'Remove case'
                        : 'switch needs at least one case',
                    onPressed: canDelete ? () => _removeCase(i) : null,
                  ),
                ],
              ),
            ),

          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              icon: const Icon(Icons.add, size: 18),
              label: const Text('Add Case'),
              onPressed: _addCase,
            ),
          ),

          // Inline error from the setter (bad Int parse, duplicate value,
          // unparseable selector flip).
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8.0),
              child: Container(
                width: double.infinity,
                padding: const EdgeInsets.all(8.0),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.errorContainer,
                  borderRadius: BorderRadius.circular(4.0),
                  border: Border.all(
                    color: Theme.of(context).colorScheme.error,
                    width: 1.0,
                  ),
                ),
                child: Text(
                  _error!,
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.onErrorContainer,
                    fontSize: 12.0,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
