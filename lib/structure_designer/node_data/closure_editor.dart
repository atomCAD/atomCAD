import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Shared shape editor for the `closure` and `apply` nodes.
///
/// Both nodes store the same data — `{ kind, type_args, param_names }` — and
/// differ only in how the kind is expanded into pins (the `closure` node
/// expands it inward to zone pins + a `Function` output; `apply` expands it
/// outward to a `Function` input + per-param arg pins). The kind is the
/// single source of truth, so one editor serves both. See
/// `doc/design_closures.md` §"Editor (Flutter) changes" and
/// `doc/design_custom_closure_kind.md` §"Editor (Flutter)".
///
/// A **kind** is a shape template that fixes the arity and decides, per pin,
/// whether the type is **free** (user picks a `DataType`) or **fixed/derived**
/// (the system supplies it). The four preset kinds are exactly the four HOF
/// shapes; `Custom` is the escape hatch with author-able arity and types:
///
/// | Kind            | free slots (user picks) | result pin       |
/// |-----------------|-------------------------|------------------|
/// | `(T) -> U`      | `T`, `U`                | free `U`         |
/// | `(T) -> Bool`   | `T`                     | fixed `Bool`     |
/// | `(A, T) -> A`   | `A`, `T`                | derived `= A`    |
/// | `(T) -> Unit`   | `T`                     | fixed `Unit`     |
/// | `(args…) → R`   | every param + return    | free             |
class ClosureShapeEditor extends StatefulWidget {
  /// Panel title (e.g. `'Closure Properties'`).
  final String title;

  /// Node type name shown in the header (`'closure'` / `'apply'`).
  final String nodeTypeName;

  /// The currently stored shape kind.
  final APIClosureKind kind;

  /// The free type arguments filling the kind's slots.
  final List<APIDataType> typeArgs;

  /// Authored parameter names. Empty for preset kinds; length-N for `Custom`.
  final List<String> paramNames;

  /// `true` while the backing data is still loading (shows a spinner).
  final bool loading;

  /// Invoked with a fully-formed `(kind, typeArgs, paramNames)` on any edit.
  /// The caller wraps it into the right node-data struct and pushes it
  /// through the model.
  final void Function(
    APIClosureKind kind,
    List<APIDataType> typeArgs,
    List<String> paramNames,
  ) onChanged;

  const ClosureShapeEditor({
    super.key,
    required this.title,
    required this.nodeTypeName,
    required this.kind,
    required this.typeArgs,
    required this.paramNames,
    required this.loading,
    required this.onChanged,
  });

  @override
  State<ClosureShapeEditor> createState() => _ClosureShapeEditorState();
}

class _ClosureShapeEditorState extends State<ClosureShapeEditor> {
  /// Default for a freshly-revealed free slot (matches the Rust node default).
  static APIDataType _defaultArg() => const APIDataType(
        dataTypeBase: APIDataTypeBase.float,
        customDataType: null,
        array: false,
      );

  /// Labels for the free type-argument slots, one per `type_args` entry
  /// (preset kinds only — Custom drives its own per-row UI).
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
      case APIClosureKind.custom:
        return const [];
    }
  }

  /// Read-only result-pin label, or `null` when the result is itself a free
  /// slot (the map-like and custom kinds).
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
      case APIClosureKind.custom:
        return null; // result is the trailing free slot
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
      case APIClosureKind.custom:
        return '(args…) → R   · custom';
    }
  }

  /// Param-name validator: non-empty, starts with a letter or `_`, remaining
  /// chars `[A-Za-z0-9_]`. Returns `null` if valid, else an error string.
  static final RegExp _identifierRegex = RegExp(r'^[A-Za-z_][A-Za-z0-9_]*$');
  static String? _validateParamName(String name) {
    if (name.isEmpty) return 'Name cannot be empty';
    if (!_identifierRegex.hasMatch(name)) {
      return 'Use letters, digits, and underscores; start with a letter or _';
    }
    return null;
  }

  /// Pick a default name for a new Custom-kind parameter row, incrementing
  /// past the highest existing `argN` to avoid an immediate duplicate error.
  static String _defaultNewParamName(List<String> existing) {
    int maxIdx = -1;
    final argN = RegExp(r'^arg(\d+)$');
    for (final name in existing) {
      final m = argN.firstMatch(name);
      if (m != null) {
        final n = int.tryParse(m.group(1)!);
        if (n != null && n > maxIdx) maxIdx = n;
      }
    }
    return 'arg${maxIdx + 1}';
  }

  /// `type_args[i]`, defaulting to a fresh slot when the stored vector is
  /// shorter than the kind expects (a transient state while editing).
  APIDataType _argAt(int i) =>
      (i < widget.typeArgs.length) ? widget.typeArgs[i] : _defaultArg();

  /// Switch kinds, resizing `type_args` / `param_names` to the new kind.
  /// Preset ↔ preset preserves overlap; preset ↔ Custom synthesizes/strips
  /// `param_names` and re-encodes `type_args` as `[params..., return]`.
  void _changeKind(APIClosureKind newKind) {
    if (newKind == widget.kind) return;

    if (newKind == APIClosureKind.custom) {
      // Preset → Custom: synthesize param names + re-encode type_args.
      final names = _presetParamNames(widget.kind);
      final paramTypes = _presetParamTypes(widget.kind, widget.typeArgs);
      final ret = _presetReturnType(widget.kind, widget.typeArgs);
      final newArgs = [...paramTypes, ret];
      widget.onChanged(newKind, newArgs, names);
    } else if (widget.kind == APIClosureKind.custom) {
      // Custom → preset: take the first N free slots, drop param_names and
      // any extra type_args entries. Lossy by design — undo handles the
      // regret case (same as today's preset-to-preset behavior).
      final newCount = _argLabels(newKind).length;
      final newArgs = <APIDataType>[
        for (int i = 0; i < newCount; i++)
          (i < widget.typeArgs.length) ? widget.typeArgs[i] : _defaultArg(),
      ];
      widget.onChanged(newKind, newArgs, const <String>[]);
    } else {
      // Preset → preset: same overlap rule as before.
      final newCount = _argLabels(newKind).length;
      final newArgs = <APIDataType>[
        for (int i = 0; i < newCount; i++)
          (i < widget.typeArgs.length) ? widget.typeArgs[i] : _defaultArg(),
      ];
      widget.onChanged(newKind, newArgs, const <String>[]);
    }
  }

  /// Static param-name table for preset kinds (mirrors `ClosureKind::param_names`
  /// in Rust). Used when transitioning preset → Custom.
  static List<String> _presetParamNames(APIClosureKind kind) {
    switch (kind) {
      case APIClosureKind.fold:
        return ['acc', 'element'];
      case APIClosureKind.map:
      case APIClosureKind.filter:
      case APIClosureKind.foreach:
        return ['element'];
      case APIClosureKind.custom:
        return const [];
    }
  }

  static List<APIDataType> _presetParamTypes(
    APIClosureKind kind,
    List<APIDataType> typeArgs,
  ) {
    APIDataType at(int i) =>
        (i < typeArgs.length) ? typeArgs[i] : _defaultArg();
    switch (kind) {
      case APIClosureKind.map:
      case APIClosureKind.filter:
      case APIClosureKind.foreach:
        return [at(0)];
      case APIClosureKind.fold:
        return [at(0), at(1)];
      case APIClosureKind.custom:
        return const [];
    }
  }

  static APIDataType _presetReturnType(
    APIClosureKind kind,
    List<APIDataType> typeArgs,
  ) {
    APIDataType at(int i) =>
        (i < typeArgs.length) ? typeArgs[i] : _defaultArg();
    switch (kind) {
      case APIClosureKind.map:
        return at(1);
      case APIClosureKind.filter:
        return const APIDataType(
          dataTypeBase: APIDataTypeBase.bool,
          customDataType: null,
          array: false,
        );
      case APIClosureKind.fold:
        return at(0); // derived = A
      case APIClosureKind.foreach:
        return const APIDataType(
          dataTypeBase: APIDataTypeBase.unit,
          customDataType: null,
          array: false,
        );
      case APIClosureKind.custom:
        return _defaultArg();
    }
  }

  /// Replace the `i`-th free type argument (preset kinds).
  void _changeArg(int i, APIDataType value) {
    final count = _argLabels(widget.kind).length;
    final newArgs = <APIDataType>[
      for (int j = 0; j < count; j++) (j == i) ? value : _argAt(j),
    ];
    widget.onChanged(widget.kind, newArgs, widget.paramNames);
  }

  // ------- Custom-kind helpers -------

  /// Replace the type at param index `i` (0..N-1) or the return type (i == N).
  void _changeCustomTypeArg(int i, APIDataType value) {
    final n = widget.paramNames.length;
    final newArgs = <APIDataType>[
      for (int j = 0; j <= n; j++) (j == i) ? value : _argAt(j),
    ];
    widget.onChanged(widget.kind, newArgs, widget.paramNames);
  }

  /// Replace the name at param index `i`.
  void _changeCustomParamName(int i, String name) {
    final newNames = <String>[
      for (int j = 0; j < widget.paramNames.length; j++)
        (j == i) ? name : widget.paramNames[j],
    ];
    widget.onChanged(widget.kind, widget.typeArgs, newNames);
  }

  /// Append a new parameter row with a default name and `Float` type.
  void _addCustomParam() {
    final n = widget.paramNames.length;
    final newName = _defaultNewParamName(widget.paramNames);
    final returnSlot = _argAt(n); // preserve current return type
    final newArgs = <APIDataType>[
      for (int j = 0; j < n; j++) _argAt(j),
      _defaultArg(), // new param's type
      returnSlot,
    ];
    final newNames = <String>[...widget.paramNames, newName];
    widget.onChanged(widget.kind, newArgs, newNames);
  }

  /// Remove the parameter row at index `i`.
  void _removeCustomParam(int i) {
    final n = widget.paramNames.length;
    if (n <= 1) return; // v1: arity >= 1
    final returnSlot = _argAt(n);
    final newArgs = <APIDataType>[
      for (int j = 0; j < n; j++)
        if (j != i) _argAt(j),
      returnSlot,
    ];
    final newNames = <String>[
      for (int j = 0; j < n; j++)
        if (j != i) widget.paramNames[j],
    ];
    widget.onChanged(widget.kind, newArgs, newNames);
  }

  @override
  Widget build(BuildContext context) {
    if (widget.loading) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          NodeEditorHeader(
            title: widget.title,
            nodeTypeName: widget.nodeTypeName,
          ),
          const SizedBox(height: 8),

          // Kind selector (the shape template).
          DropdownButtonFormField<APIClosureKind>(
            value: widget.kind,
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

          if (widget.kind == APIClosureKind.custom)
            ..._buildCustomBranch(context)
          else
            ..._buildPresetBranch(context),
        ],
      ),
    );
  }

  List<Widget> _buildPresetBranch(BuildContext context) {
    final argLabels = _argLabels(widget.kind);
    final resultLabel = _resultLabel(widget.kind);

    return [
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
    ];
  }

  List<Widget> _buildCustomBranch(BuildContext context) {
    final n = widget.paramNames.length;
    // Per-row duplicate detection: a name is duplicated if it appears earlier
    // in the list (the *first* occurrence is fine; later ones are errored).
    final seenBefore = <String>{};
    final isDuplicate = <bool>[];
    for (int i = 0; i < n; i++) {
      final name = widget.paramNames[i];
      isDuplicate.add(seenBefore.contains(name));
      seenBefore.add(name);
    }

    return [
      const Padding(
        padding: EdgeInsets.only(bottom: 4.0),
        child: Text('Parameters', style: TextStyle(color: Colors.white70)),
      ),
      for (int i = 0; i < n; i++)
        Padding(
          padding: const EdgeInsets.only(bottom: 8.0),
          child: _CustomParamRow(
            name: widget.paramNames[i],
            type: _argAt(i),
            duplicate: isDuplicate[i],
            canDelete: n > 1,
            onNameChanged: (newName) => _changeCustomParamName(i, newName),
            onTypeChanged: (newType) => _changeCustomTypeArg(i, newType),
            onDelete: () => _removeCustomParam(i),
          ),
        ),
      Align(
        alignment: Alignment.centerLeft,
        child: TextButton.icon(
          icon: const Icon(Icons.add, size: 18),
          label: const Text('Add parameter'),
          onPressed: _addCustomParam,
        ),
      ),
      const Divider(height: 24),
      const Padding(
        padding: EdgeInsets.only(bottom: 4.0),
        child: Text('Return Type', style: TextStyle(color: Colors.white70)),
      ),
      DataTypeInput(
        label: 'Return Type',
        value: _argAt(n),
        onChanged: (newValue) => _changeCustomTypeArg(n, newValue),
      ),
    ];
  }
}

/// One row of the Custom-kind parameter list: a name `TextField`, a
/// `DataTypeInput`, and a delete button. Identifier validation runs locally;
/// invalid names are surfaced via a red border + helper text but the latest
/// text is still pushed to the controller — the *invalid string* is what gets
/// persisted to the parent, matching the lossy commit cadence of other
/// editor fields (`parameter_node`'s name) rather than blocking persistence.
class _CustomParamRow extends StatefulWidget {
  final String name;
  final APIDataType type;

  /// `true` when this name is a duplicate of an earlier row's name.
  final bool duplicate;

  /// `false` when the row is the last one (we don't allow zero-arg closures).
  final bool canDelete;

  final ValueChanged<String> onNameChanged;
  final ValueChanged<APIDataType> onTypeChanged;
  final VoidCallback onDelete;

  const _CustomParamRow({
    required this.name,
    required this.type,
    required this.duplicate,
    required this.canDelete,
    required this.onNameChanged,
    required this.onTypeChanged,
    required this.onDelete,
  });

  @override
  State<_CustomParamRow> createState() => _CustomParamRowState();
}

class _CustomParamRowState extends State<_CustomParamRow> {
  late TextEditingController _controller;
  late FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.name);
    _focusNode = FocusNode();
    _focusNode.addListener(() {
      if (!_focusNode.hasFocus) {
        // Commit on blur. We pass through the current text — validation
        // is purely UI feedback; the Rust side accepts whatever string.
        if (_controller.text != widget.name) {
          widget.onNameChanged(_controller.text);
        }
      }
    });
  }

  @override
  void didUpdateWidget(_CustomParamRow oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.name != widget.name && _controller.text != widget.name) {
      final selection = _controller.selection;
      _controller.text = widget.name;
      if (selection.isValid && selection.end <= widget.name.length) {
        _controller.selection = selection;
      } else {
        _controller.selection =
            TextSelection.collapsed(offset: widget.name.length);
      }
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final error = _ClosureShapeEditorState._validateParamName(widget.name) ??
        (widget.duplicate ? 'Duplicate parameter name' : null);

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 120,
          child: TextField(
            controller: _controller,
            focusNode: _focusNode,
            decoration: InputDecoration(
              labelText: 'Name',
              border: const OutlineInputBorder(),
              isDense: true,
              errorText: error,
            ),
            onSubmitted: (value) {
              if (value != widget.name) widget.onNameChanged(value);
            },
          ),
        ),
        const SizedBox(width: 8),
        Expanded(
          child: DataTypeInput(
            label: 'Type',
            value: widget.type,
            onChanged: widget.onTypeChanged,
          ),
        ),
        IconButton(
          icon: const Icon(Icons.delete_outline),
          tooltip: widget.canDelete ? 'Remove parameter' : 'At least one parameter is required',
          onPressed: widget.canDelete ? widget.onDelete : null,
        ),
      ],
    );
  }
}
