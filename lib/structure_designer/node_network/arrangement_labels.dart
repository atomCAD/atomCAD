/// Row labels for the arrangement dialog (`doc/design_insert_node_on_wire.md`
/// D3). Kept free of widgets and of the generated API so the rule is
/// unit-tested on its own (`test/arrangement_labels_test.dart`).
library;

/// One arrangement as the dialog sees it: the names (and types) on each
/// side, `null` for a side the plan did not have.
class ArrangementSides {
  final String? inputName;
  final String? inputType;
  final String? outputName;
  final String? outputType;

  const ArrangementSides({
    this.inputName,
    this.inputType,
    this.outputName,
    this.outputType,
  });
}

/// What a row shows: its label and the type printed beside it (which also
/// picks the pin dot's colour).
class ArrangementRowLabel {
  final String label;
  final String typeLabel;

  const ArrangementRowLabel(this.label, this.typeLabel);

  @override
  bool operator ==(Object other) =>
      other is ArrangementRowLabel &&
      other.label == label &&
      other.typeLabel == typeLabel;

  @override
  int get hashCode => Object.hash(label, typeLabel);

  @override
  String toString() => 'ArrangementRowLabel($label, $typeLabel)';
}

/// Each row names only the side that varies across the rows (`base` / `sub`;
/// `result` / `diff`); when both vary, `input → output`. The type shown is
/// the varying side's — the output's when both vary, since that is what
/// reaches the destination.
List<ArrangementRowLabel> arrangementRowLabels(List<ArrangementSides> rows) {
  bool varies(String? Function(ArrangementSides) side) =>
      rows.map(side).toSet().length > 1;
  final inputsVary = varies((r) => r.inputName);
  final outputsVary = varies((r) => r.outputName);

  return rows.map((r) {
    if (inputsVary && outputsVary) {
      return ArrangementRowLabel(
        '${r.inputName ?? ''} → ${r.outputName ?? ''}',
        r.outputType ?? r.inputType ?? '',
      );
    }
    if (outputsVary || (r.inputName == null && r.outputName != null)) {
      return ArrangementRowLabel(r.outputName ?? '', r.outputType ?? '');
    }
    return ArrangementRowLabel(r.inputName ?? '', r.inputType ?? '');
  }).toList();
}
