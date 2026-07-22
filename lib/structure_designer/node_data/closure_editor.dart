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
/// | `() → T`        | `T`                     | free `T`         |
/// | `(T) -> U`      | `T`, `U`                | free `U`         |
/// | `(T) -> Bool`   | `T`                     | fixed `Bool`     |
/// | `(A, T) -> A`   | `A`, `T`                | derived `= A`    |
/// | `(T) -> Unit`   | `T`                     | fixed `Unit`     |
/// | `(args…) → R`   | every param + return    | free             |
///
/// The first row — the **0-ary function** `() → T` (issue #418) — is not a
/// separate `APIClosureKind`: it *is* `Custom` with an empty parameter list,
/// promoted to its own top-of-list dropdown entry because it is by far the
/// most-used shape ("a named value evaluated in its captured context", and the
/// only shape whose body renders in the 3D viewport). Representing it as a
/// second enum variant would give one concept two storage forms; instead the
/// dropdown is keyed by [_KindOption], which folds `(kind, paramNames)` down
/// to the entry to show.
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

  /// Optional user-supplied free-form display label. Only meaningful for the
  /// `closure` node (the `apply` node passes `null` here and the editor hides
  /// the label row). No format restrictions — distinct from the
  /// identifier-only `Node.custom_name` used by the text format.
  final String? customLabel;

  /// `true` while the backing data is still loading (shows a spinner).
  final bool loading;

  /// Invoked with a fully-formed `(kind, typeArgs, paramNames, customLabel)`
  /// on any edit. The caller wraps it into the right node-data struct and
  /// pushes it through the model. `customLabel` is forwarded for `closure`
  /// nodes and ignored for `apply` (which passes `null` and never reads it).
  final void Function(
    APIClosureKind kind,
    List<APIDataType> typeArgs,
    List<String> paramNames,
    String? customLabel,
  ) onChanged;

  /// `true` to render the optional label TextField (closure only).
  final bool labelEnabled;

  const ClosureShapeEditor({
    super.key,
    required this.title,
    required this.nodeTypeName,
    required this.kind,
    required this.typeArgs,
    required this.paramNames,
    required this.loading,
    required this.onChanged,
    this.customLabel,
    this.labelEnabled = false,
  });

  @override
  State<ClosureShapeEditor> createState() => _ClosureShapeEditorState();
}

/// The entries of the Kind dropdown. One-to-one with `APIClosureKind` except
/// for [nullary], which is `Custom` with zero parameters surfaced as its own
/// first-class choice (issue #418).
enum _KindOption { nullary, map, filter, fold, foreach, custom }

class _ClosureShapeEditorState extends State<ClosureShapeEditor> {
  /// Default for a freshly-revealed free slot (matches the Rust node default).
  static APIDataType _defaultArg() => const APIDataType(
        dataTypeBase: APIDataTypeBase.float,
        customDataType: null,
        array: false,
        children: [],
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
  static String _optionSignature(_KindOption option) {
    switch (option) {
      case _KindOption.nullary:
        return '() → T   · 0-ary function';
      case _KindOption.map:
        return '(T) → U   · map-like';
      case _KindOption.filter:
        return '(T) → Bool   · filter-like';
      case _KindOption.fold:
        return '(A, T) → A   · fold-like';
      case _KindOption.foreach:
        return '(T) → Unit   · foreach-like';
      case _KindOption.custom:
        return '(args…) → R   · custom';
    }
  }

  /// Fold the stored `(kind, param_names)` pair down to the dropdown entry:
  /// a `Custom` closure with no parameters shows as the 0-ary entry.
  static _KindOption _optionFor(APIClosureKind kind, List<String> paramNames) {
    switch (kind) {
      case APIClosureKind.map:
        return _KindOption.map;
      case APIClosureKind.filter:
        return _KindOption.filter;
      case APIClosureKind.fold:
        return _KindOption.fold;
      case APIClosureKind.foreach:
        return _KindOption.foreach;
      case APIClosureKind.custom:
        return paramNames.isEmpty ? _KindOption.nullary : _KindOption.custom;
    }
  }

  /// The stored kind an entry maps to. Both [_KindOption.nullary] and
  /// [_KindOption.custom] store `Custom`; they differ only in arity.
  static APIClosureKind _kindOf(_KindOption option) {
    switch (option) {
      case _KindOption.nullary:
      case _KindOption.custom:
        return APIClosureKind.custom;
      case _KindOption.map:
        return APIClosureKind.map;
      case _KindOption.filter:
        return APIClosureKind.filter;
      case _KindOption.fold:
        return APIClosureKind.fold;
      case _KindOption.foreach:
        return APIClosureKind.foreach;
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

  /// Switch dropdown entries, resizing `type_args` / `param_names` to the new
  /// shape. Preset ↔ preset preserves overlap; preset ↔ Custom
  /// synthesizes/strips `param_names` and re-encodes `type_args` as
  /// `[params..., return]`; the 0-ary entry drops every parameter and keeps
  /// only the return type.
  void _changeOption(_KindOption newOption) {
    final current = _optionFor(widget.kind, widget.paramNames);
    if (newOption == current) return;

    final newKind = _kindOf(newOption);

    if (newOption == _KindOption.nullary) {
      // → 0-ary: keep the current return type, drop all parameters.
      final ret = widget.kind == APIClosureKind.custom
          ? _argAt(widget.paramNames.length)
          : _presetReturnType(widget.kind, widget.typeArgs);
      widget.onChanged(newKind, [ret], const <String>[], widget.customLabel);
    } else if (current == _KindOption.nullary) {
      // 0-ary → anything: the stored `type_args` is `[return]` only, so there
      // is nothing to carry into the new parameter slots. Seed them fresh and
      // keep the return type where the target shape has a free one.
      final ret = _argAt(0);
      if (newOption == _KindOption.custom) {
        // Seed one parameter so the entry visibly differs from 0-ary.
        widget.onChanged(
          newKind,
          [_defaultArg(), ret],
          <String>[_defaultNewParamName(const [])],
          widget.customLabel,
        );
      } else {
        final newCount = _argLabels(newKind).length;
        final newArgs = <APIDataType>[
          for (int i = 0; i < newCount; i++) _defaultArg(),
        ];
        // `map`'s second free slot *is* the return type — preserve it.
        if (newKind == APIClosureKind.map) newArgs[1] = ret;
        widget.onChanged(
            newKind, newArgs, const <String>[], widget.customLabel);
      }
    } else if (newOption == _KindOption.custom) {
      // Preset → Custom: synthesize param names + re-encode type_args.
      final names = _presetParamNames(widget.kind);
      final paramTypes = _presetParamTypes(widget.kind, widget.typeArgs);
      final ret = _presetReturnType(widget.kind, widget.typeArgs);
      final newArgs = [...paramTypes, ret];
      widget.onChanged(newKind, newArgs, names, widget.customLabel);
    } else if (widget.kind == APIClosureKind.custom) {
      // Custom → preset: take the first N free slots, drop param_names and
      // any extra type_args entries. Lossy by design — undo handles the
      // regret case (same as today's preset-to-preset behavior).
      final newCount = _argLabels(newKind).length;
      final newArgs = <APIDataType>[
        for (int i = 0; i < newCount; i++)
          (i < widget.typeArgs.length) ? widget.typeArgs[i] : _defaultArg(),
      ];
      widget.onChanged(newKind, newArgs, const <String>[], widget.customLabel);
    } else {
      // Preset → preset: same overlap rule as before.
      final newCount = _argLabels(newKind).length;
      final newArgs = <APIDataType>[
        for (int i = 0; i < newCount; i++)
          (i < widget.typeArgs.length) ? widget.typeArgs[i] : _defaultArg(),
      ];
      widget.onChanged(newKind, newArgs, const <String>[], widget.customLabel);
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
          children: [],
        );
      case APIClosureKind.fold:
        return at(0); // derived = A
      case APIClosureKind.foreach:
        return const APIDataType(
          dataTypeBase: APIDataTypeBase.unit,
          customDataType: null,
          array: false,
          children: [],
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
    widget.onChanged(
        widget.kind, newArgs, widget.paramNames, widget.customLabel);
  }

  // ------- Custom-kind helpers -------

  /// Replace the type at param index `i` (0..N-1) or the return type (i == N).
  void _changeCustomTypeArg(int i, APIDataType value) {
    final n = widget.paramNames.length;
    final newArgs = <APIDataType>[
      for (int j = 0; j <= n; j++) (j == i) ? value : _argAt(j),
    ];
    widget.onChanged(
        widget.kind, newArgs, widget.paramNames, widget.customLabel);
  }

  /// Replace the name at param index `i`.
  void _changeCustomParamName(int i, String name) {
    final newNames = <String>[
      for (int j = 0; j < widget.paramNames.length; j++)
        (j == i) ? name : widget.paramNames[j],
    ];
    widget.onChanged(
        widget.kind, widget.typeArgs, newNames, widget.customLabel);
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
    widget.onChanged(widget.kind, newArgs, newNames, widget.customLabel);
  }

  /// Remove the parameter row at index `i`. Arity 0 (a thunk `() -> R`) is
  /// legal — the substrate supports zero-arg closures and the function-type
  /// picker has always allowed `() -> R`.
  void _removeCustomParam(int i) {
    final n = widget.paramNames.length;
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
    widget.onChanged(widget.kind, newArgs, newNames, widget.customLabel);
  }

  /// Push a new label to the parent. Empty / whitespace-only strings are
  /// normalized to `null` so the title bar falls back to signature-only.
  void _changeCustomLabel(String raw) {
    final trimmed = raw.trim();
    final normalized = trimmed.isEmpty ? null : trimmed;
    widget.onChanged(
      widget.kind,
      widget.typeArgs,
      widget.paramNames,
      normalized,
    );
  }

  @override
  Widget build(BuildContext context) {
    if (widget.loading) {
      return const Center(child: CircularProgressIndicator());
    }

    final option = _optionFor(widget.kind, widget.paramNames);

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

          if (widget.labelEnabled) ...[
            _ClosureLabelField(
              value: widget.customLabel ?? '',
              onChanged: _changeCustomLabel,
            ),
            const SizedBox(height: 8),
          ],

          // Kind selector (the shape template).
          DropdownButtonFormField<_KindOption>(
            value: option,
            decoration: const InputDecoration(
              labelText: 'Kind',
              border: OutlineInputBorder(),
              isDense: true,
            ),
            items: _KindOption.values
                .map((o) => DropdownMenuItem(
                      value: o,
                      child: Text(_optionSignature(o)),
                    ))
                .toList(),
            onChanged: (newOption) {
              if (newOption != null) _changeOption(newOption);
            },
          ),
          const SizedBox(height: 8),

          if (option == _KindOption.nullary)
            ..._buildNullaryBranch(context)
          else if (option == _KindOption.custom)
            ..._buildCustomBranch(context)
          else
            ..._buildPresetBranch(context),
        ],
      ),
    );
  }

  /// The 0-ary branch: no parameter list at all, just the return type. Stored
  /// as `Custom` with `param_names == []`, so `type_args[0]` *is* the return
  /// type (`_changeCustomTypeArg(0, …)` writes exactly that one slot).
  List<Widget> _buildNullaryBranch(BuildContext context) {
    return [
      DataTypeInput(
        label: 'Return Type (T)',
        value: _argAt(0),
        onChanged: (newValue) => _changeCustomTypeArg(0, newValue),
      ),
      const SizedBox(height: 8),
      const Text(
        'No parameters — the body is a named value evaluated in this node\'s '
        'captured context. Its nodes get visibility eyes and render in the 3D '
        'viewport. Switch Kind to "custom" to add parameters.',
        style: TextStyle(color: Colors.white70, fontSize: 12),
      ),
    ];
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

  final ValueChanged<String> onNameChanged;
  final ValueChanged<APIDataType> onTypeChanged;
  final VoidCallback onDelete;

  const _CustomParamRow({
    required this.name,
    required this.type,
    required this.duplicate,
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
          tooltip: 'Remove parameter',
          onPressed: widget.onDelete,
        ),
      ],
    );
  }
}

/// Free-form display-label TextField for the `closure` node. Commits on blur
/// and on Enter (matches the lossy-commit cadence of the `_CustomParamRow`
/// name field). Empty/whitespace input clears the label so the title bar
/// falls back to signature-only.
class _ClosureLabelField extends StatefulWidget {
  final String value;
  final ValueChanged<String> onChanged;

  const _ClosureLabelField({
    required this.value,
    required this.onChanged,
  });

  @override
  State<_ClosureLabelField> createState() => _ClosureLabelFieldState();
}

class _ClosureLabelFieldState extends State<_ClosureLabelField> {
  late TextEditingController _controller;
  late FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value);
    _focusNode = FocusNode();
    _focusNode.addListener(() {
      if (!_focusNode.hasFocus && _controller.text != widget.value) {
        widget.onChanged(_controller.text);
      }
    });
  }

  @override
  void didUpdateWidget(_ClosureLabelField oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value && _controller.text != widget.value) {
      final selection = _controller.selection;
      _controller.text = widget.value;
      if (selection.isValid && selection.end <= widget.value.length) {
        _controller.selection = selection;
      } else {
        _controller.selection =
            TextSelection.collapsed(offset: widget.value.length);
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
    return TextField(
      controller: _controller,
      focusNode: _focusNode,
      decoration: const InputDecoration(
        labelText: 'Label (optional)',
        helperText: 'Shown in the title bar before the signature',
        border: OutlineInputBorder(),
        isDense: true,
      ),
      onSubmitted: (value) {
        if (value != widget.value) widget.onChanged(value);
      },
    );
  }
}
