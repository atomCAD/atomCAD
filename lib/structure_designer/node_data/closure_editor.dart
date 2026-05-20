import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Shared shape editor for the `closure` and `apply` nodes.
///
/// Both nodes store the same data — `{ kind, type_args }` — and differ only in
/// how the kind is expanded into pins (the `closure` node expands it inward to
/// zone pins + a `Function` output; `apply` expands it outward to a `Function`
/// input + per-param arg pins). The kind is the single source of truth, so one
/// editor serves both. See `doc/design_closures.md` §"Editor (Flutter) changes".
///
/// A **kind** is a shape template that fixes the arity and decides, per pin,
/// whether the type is **free** (user picks a `DataType`) or **fixed/derived**
/// (the system supplies it). The four v1 kinds are exactly the four HOF shapes:
///
/// | Kind            | free slots (user picks) | result pin       |
/// |-----------------|-------------------------|------------------|
/// | `(T) -> U`      | `T`, `U`                | free `U`         |
/// | `(T) -> Bool`   | `T`                     | fixed `Bool`     |
/// | `(A, T) -> A`   | `A`, `T`                | derived `= A`    |
/// | `(T) -> Unit`   | `T`                     | fixed `Unit`     |
class ClosureShapeEditor extends StatelessWidget {
  /// Panel title (e.g. `'Closure Properties'`).
  final String title;

  /// Node type name shown in the header (`'closure'` / `'apply'`).
  final String nodeTypeName;

  /// The currently stored shape kind.
  final APIClosureKind kind;

  /// The free type arguments filling the kind's slots (1 or 2 by kind).
  final List<APIDataType> typeArgs;

  /// `true` while the backing data is still loading (shows a spinner).
  final bool loading;

  /// Invoked with a fully-formed `(kind, typeArgs)` on any edit. The caller
  /// wraps it into the right node-data struct and pushes it through the model.
  final void Function(APIClosureKind kind, List<APIDataType> typeArgs) onChanged;

  const ClosureShapeEditor({
    super.key,
    required this.title,
    required this.nodeTypeName,
    required this.kind,
    required this.typeArgs,
    required this.loading,
    required this.onChanged,
  });

  /// Default for a freshly-revealed free slot (matches the Rust node default).
  static APIDataType _defaultArg() => APIDataType(
        dataTypeBase: APIDataTypeBase.float,
        customDataType: null,
        array: false,
      );

  /// Labels for the free type-argument slots, one per `type_args` entry.
  static List<String> _argLabels(APIClosureKind kind) {
    switch (kind) {
      case APIClosureKind.map:
        return ['Parameter Type (T)', 'Result Type (U)'];
      case APIClosureKind.filter:
        return ['Parameter Type (T)'];
      case APIClosureKind.fold:
        return ['Accumulator Type (A)', 'Element Type (T)'];
      case APIClosureKind.foreach:
        return ['Parameter Type (T)'];
    }
  }

  /// Read-only result-pin label, or `null` when the result is itself a free
  /// slot (the map-like kind, whose `U` is the second `DataTypeInput`).
  static String? _resultLabel(APIClosureKind kind) {
    switch (kind) {
      case APIClosureKind.map:
        return null; // result is the second free slot
      case APIClosureKind.filter:
        return 'Bool';
      case APIClosureKind.fold:
        return '= Accumulator (A)';
      case APIClosureKind.foreach:
        return 'Unit';
    }
  }

  /// Human-readable signature for the kind dropdown.
  static String _kindSignature(APIClosureKind kind) {
    switch (kind) {
      case APIClosureKind.map:
        return '(T) → U   · map-like';
      case APIClosureKind.filter:
        return '(T) → Bool   · filter-like';
      case APIClosureKind.fold:
        return '(A, T) → A   · fold-like';
      case APIClosureKind.foreach:
        return '(T) → Unit   · foreach-like';
    }
  }

  /// `type_args[i]`, defaulting to a fresh slot when the stored vector is
  /// shorter than the kind expects (a transient state while editing).
  APIDataType _argAt(int i) =>
      (i < typeArgs.length) ? typeArgs[i] : _defaultArg();

  /// Switch kinds, resizing `type_args` to the new kind's slot count and
  /// preserving overlapping entries. The Rust setter routes the structural
  /// pin change through the existing repair path.
  void _changeKind(APIClosureKind newKind) {
    if (newKind == kind) return;
    final newCount = _argLabels(newKind).length;
    final newArgs = <APIDataType>[
      for (int i = 0; i < newCount; i++)
        (i < typeArgs.length) ? typeArgs[i] : _defaultArg(),
    ];
    onChanged(newKind, newArgs);
  }

  /// Replace the `i`-th free type argument.
  void _changeArg(int i, APIDataType value) {
    final count = _argLabels(kind).length;
    final newArgs = <APIDataType>[
      for (int j = 0; j < count; j++) (j == i) ? value : _argAt(j),
    ];
    onChanged(kind, newArgs);
  }

  @override
  Widget build(BuildContext context) {
    if (loading) {
      return const Center(child: CircularProgressIndicator());
    }

    final argLabels = _argLabels(kind);
    final resultLabel = _resultLabel(kind);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          NodeEditorHeader(title: title, nodeTypeName: nodeTypeName),
          const SizedBox(height: 8),

          // Kind selector (the shape template).
          DropdownButtonFormField<APIClosureKind>(
            value: kind,
            decoration: const InputDecoration(
              labelText: 'Kind',
              border: OutlineInputBorder(),
              isDense: true,
            ),
            items: APIClosureKind.values
                .map((k) => DropdownMenuItem(
                      value: k,
                      child: Text(_kindSignature(k)),
                    ))
                .toList(),
            onChanged: (newKind) {
              if (newKind != null) _changeKind(newKind);
            },
          ),
          const SizedBox(height: 8),

          // One DataTypeInput per free slot.
          for (int i = 0; i < argLabels.length; i++) ...[
            DataTypeInput(
              label: argLabels[i],
              value: _argAt(i),
              onChanged: (newValue) => _changeArg(i, newValue),
            ),
            const SizedBox(height: 8),
          ],

          // Read-only result line for kinds whose result is fixed/derived.
          if (resultLabel != null)
            Padding(
              padding: const EdgeInsets.only(top: 4.0),
              child: Row(
                children: [
                  const Text(
                    'Result: ',
                    style: TextStyle(color: Colors.white70),
                  ),
                  Text(
                    resultLabel,
                    style: const TextStyle(
                      color: Colors.white,
                      fontStyle: FontStyle.italic,
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
