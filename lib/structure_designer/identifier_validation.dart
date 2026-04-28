// Validation rules for user-provided node-network names and per-node custom
// names. Mirrors `rust/src/structure_designer/identifier.rs`. See
// `doc/design_relaxed_node_names.md` for the design rationale.

/// Validates a user-provided network or per-node name.
///
/// Returns `null` if the name is valid, otherwise a human-readable error
/// message describing why it was rejected.
String? validateUserName(String name) {
  if (name.isEmpty) {
    return 'Name cannot be empty';
  }
  for (final code in name.runes) {
    if (code == 0x60 /* backtick */) {
      return 'Name cannot contain backticks (reserved as quoting delimiter)';
    }
    if (_isControl(code)) {
      return 'Name cannot contain control characters';
    }
  }
  if (_isWhitespace(name.codeUnitAt(0)) ||
      _isWhitespace(name.codeUnitAt(name.length - 1))) {
    return 'Name cannot start or end with whitespace';
  }
  return null;
}

bool _isControl(int code) {
  // C0 controls + DEL + C1 controls. Matches Rust `char::is_control`.
  return (code <= 0x1F) || (code >= 0x7F && code <= 0x9F);
}

bool _isWhitespace(int code) {
  // Common whitespace characters. Stricter than Unicode but matches the
  // typical paste-accident case the validator targets (space, tab, newlines,
  // non-breaking space).
  return code == 0x20 || // space
      code == 0x09 || // tab
      code == 0x0A || // LF
      code == 0x0D || // CR
      code == 0x0B || // VT
      code == 0x0C || // FF
      code == 0xA0; // NBSP
}
