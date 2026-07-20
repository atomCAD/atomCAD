import 'package:flutter/material.dart';

/// The allowed passivant elements (atomic number → label), matching the Rust
/// `ALLOWED_PASSIVANTS` set `{1, 9, 17, 35, 53}` (H/F/Cl/Br/I). A restricted
/// list is better UX than the full ~100-element periodic table, since every
/// other element would be rejected at eval (design_halogen_passivation.md D1).
const List<(int, String)> kPassivantElements = [
  (1, 'H — Hydrogen'),
  (9, 'F — Fluorine'),
  (17, 'Cl — Chlorine'),
  (35, 'Br — Bromine'),
  (53, 'I — Iodine'),
];

/// A small fixed dropdown of the five allowed passivant elements. Shared by the
/// `passivate` and `materialize` editors. The [value] is the atomic number;
/// [onChanged] receives the newly selected atomic number.
class PassivantDropdown extends StatelessWidget {
  final int value;
  final ValueChanged<int> onChanged;
  final String? label;

  const PassivantDropdown({
    super.key,
    required this.value,
    required this.onChanged,
    this.label,
  });

  @override
  Widget build(BuildContext context) {
    // If the stored value is somehow outside the allowed set (e.g. authored via
    // the text format), fall back to hydrogen for display so the dropdown has a
    // valid selection; the eval-time check still surfaces the real error.
    final selected =
        kPassivantElements.any((e) => e.$1 == value) ? value : 1;
    return DropdownButtonFormField<int>(
      value: selected,
      decoration: InputDecoration(
        labelText: label ?? 'Passivant element',
        border: const OutlineInputBorder(),
        isDense: true,
      ),
      items: [
        for (final (z, text) in kPassivantElements)
          DropdownMenuItem<int>(value: z, child: Text(text)),
      ],
      onChanged: (newValue) {
        if (newValue != null) onChanged(newValue);
      },
    );
  }
}
