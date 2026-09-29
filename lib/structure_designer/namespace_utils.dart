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
  final allRecordDefs = sd_api.getAllRecordTypeDefNames() ?? const <String>[];
  if (allRecordDefs.contains(name)) return true;
  return false;
}

// --- Linked libraries (`doc/design_library_linking.md` D3, §5.3) ---

/// True iff [name] is [prefix] itself or lies under it (`prefix.` + more).
/// Never a bare `startsWith(prefix)`, which would take `demolibx` for part of
/// `demolib`. Twin of Rust `library_links::is_under`.
bool isUnderNamespace(String name, String prefix) =>
    name == prefix ||
    (name.length > prefix.length &&
        name.startsWith(prefix) &&
        name.codeUnitAt(prefix.length) == 0x2E /* . */);

/// The mount path owning [name] — the longest of [mountPaths] that [name] is
/// or lies under — or `null` for local content. Dart twin of Rust
/// `LibraryLinks::mount_containing`, for instant UI feedback only: Rust stays
/// authoritative. `test/library_links_test.dart` mirrors the Rust table
/// (`MOUNT_FOR_CASES`) case for case.
String? mountFor(String name, Iterable<String> mountPaths) {
  String? best;
  for (final p in mountPaths) {
    if (isUnderNamespace(name, p) && (best == null || p.length > best.length)) {
      best = p;
    }
  }
  return best;
}

final RegExp _aliasSegment = RegExp(r'^[A-Za-z_][A-Za-z0-9_]*$');

/// Why [alias] is not a syntactically valid library alias, or `null`. Every
/// dot-separated segment must be an identifier. Twin of Rust
/// `library_links::validate_alias`; whether the alias is *free* (no clash with
/// a local name or another mount) is asked of Rust (`checkLibraryAlias`).
String? validateLibraryAlias(String alias) {
  if (alias.isEmpty) return 'Alias must not be empty';
  for (final segment in alias.split('.')) {
    if (!_aliasSegment.hasMatch(segment)) {
      return "Invalid segment '$segment' (each segment must be an identifier)";
    }
  }
  return null;
}

/// The alias *Link library…* proposes for a file: its stem, minus a trailing
/// version suffix (`demolib_v3.cnnd` → `demolib`), made into an identifier.
String suggestLibraryAlias(String filePath) {
  var stem = filePath.split(RegExp(r'[\\/]')).last;
  final dot = stem.lastIndexOf('.');
  if (dot > 0) stem = stem.substring(0, dot);
  stem = stem.replaceFirst(RegExp(r'[_\-. ]?v\d+$', caseSensitive: false), '');
  stem = stem.replaceAll(RegExp(r'[^A-Za-z0-9_]'), '_');
  if (stem.isEmpty) return 'lib';
  if (RegExp(r'^[0-9]').hasMatch(stem)) stem = '_$stem';
  return stem;
}

/// The label of a mount folder in the user-types panel: the folder's own
/// segment, then the library's file name (`demolib — demolib_v3.cnnd`).
String mountFolderLabel(String segment, String fileName) =>
    '$segment — $fileName';

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
