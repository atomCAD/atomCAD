// Utilities for handling qualified names and namespaces in node networks.
//
// Node networks use dot-delimited hierarchical names similar to Java packages.
// For example: "Physics.Mechanics.Spring"
// - Qualified Name: "Physics.Mechanics.Spring" (full name)
// - Namespace: "Physics.Mechanics" (organizational prefix)
// - Simple Name: "Spring" (leaf name, the actual network name)
// - Segments: ["Physics", "Mechanics", "Spring"] (individual parts)

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;

/// Returns `true` when `name` is taken anywhere in the user-type namespace —
/// by an existing node network, a user-declared record type def, or a
/// built-in record type def (e.g. `ElementMapping`). Mirrors the Rust-side
/// `NodeTypeRegistry::name_is_taken` for Flutter UI pre-validation. The Rust
/// API still enforces the invariant authoritatively; this helper is for
/// instant feedback in dialogs.
bool nameIsTaken(String name) {
  final networks = sd_api.getNodeNetworkNames() ?? const <String>[];
  if (networks.contains(name)) return true;
  final allRecordDefs =
      sd_api.getAllRecordTypeDefNames() ?? const <String>[];
  if (allRecordDefs.contains(name)) return true;
  return false;
}

/// Extracts the simple name (leaf name) from a qualified name.
///
/// Examples:
/// - "Physics.Mechanics.Spring" → "Spring"
/// - "Math.Vector" → "Vector"
/// - "SimpleNode" → "SimpleNode"
String getSimpleName(String qualifiedName) {
  final lastDotIndex = qualifiedName.lastIndexOf('.');
  if (lastDotIndex == -1) {
    // No dot found, the whole name is the simple name
    return qualifiedName;
  }
  return qualifiedName.substring(lastDotIndex + 1);
}

/// Extracts the namespace from a qualified name.
/// Returns an empty string if there is no namespace.
///
/// Examples:
/// - "Physics.Mechanics.Spring" → "Physics.Mechanics"
/// - "Math.Vector" → "Math"
/// - "SimpleNode" → ""
String getNamespace(String qualifiedName) {
  final lastDotIndex = qualifiedName.lastIndexOf('.');
  if (lastDotIndex == -1) {
    // No dot found, no namespace
    return '';
  }
  return qualifiedName.substring(0, lastDotIndex);
}

/// Splits a qualified name into segments.
///
/// Examples:
/// - "Physics.Mechanics.Spring" → ["Physics", "Mechanics", "Spring"]
/// - "Math.Vector" → ["Math", "Vector"]
/// - "SimpleNode" → ["SimpleNode"]
List<String> getSegments(String qualifiedName) {
  return qualifiedName.split('.');
}

/// Checks if a name is qualified (contains at least one dot).
///
/// Examples:
/// - "Physics.Mechanics.Spring" → true
/// - "Math.Vector" → true
/// - "SimpleNode" → false
bool isQualifiedName(String name) {
  return name.contains('.');
}

/// Combines namespace and simple name into a qualified name.
///
/// Examples:
/// - ("Physics.Mechanics", "Spring") → "Physics.Mechanics.Spring"
/// - ("Math", "Vector") → "Math.Vector"
/// - ("", "SimpleNode") → "SimpleNode"
String combineQualifiedName(String namespace, String simpleName) {
  if (namespace.isEmpty) {
    return simpleName;
  }
  return '$namespace.$simpleName';
}
