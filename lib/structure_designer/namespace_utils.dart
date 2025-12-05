// Utilities for handling qualified names and namespaces in node networks.
//
// Node networks use dot-delimited hierarchical names similar to Java packages.
// For example: "Physics.Mechanics.Spring"
// - Qualified Name: "Physics.Mechanics.Spring" (full name)
// - Namespace: "Physics.Mechanics" (organizational prefix)
// - Simple Name: "Spring" (leaf name, the actual network name)
// - Segments: ["Physics", "Mechanics", "Spring"] (individual parts)

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
