/// Plain-text rendering of the design's problems, for pasting into a bug
/// report (issue #359).
///
/// The issue asked for selectable error text; the *goal* behind it was getting
/// errors out of the app and into a report. Hand-selecting a tooltip gives you
/// one message with no node id, no network name, no severity and no root-cause
/// chain. This module renders the same unified error list the panel badge
/// consumes (`doc/design_error_management.md` D1) into a complete, pasteable
/// report — which is both cheaper to produce and strictly more useful.
///
/// **No Rust involved.** `APIValidationError` already carries everything the
/// report needs (`errorText`, `blocking`, `source`, `stale`, `scopePath`,
/// `nodeId`, `nodeLabel`, `bodyQualifier`, `hostNetwork`, `rootCause`), so this
/// is a pure function over `StructureDesignerModel.nodeNetworkNames` — no API
/// change, no `flutter_rust_bridge_codegen generate`, and unit-testable in a
/// way none of the UI surfaces are.
///
/// This file is also the shared home of the **root-cause grouping** (D7) that
/// the panel badge and its picker use, so the copied report and the on-screen
/// list can never disagree about what counts as one problem.
library;

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// One underlying problem: the entry listed at top level plus the downstream
/// entries collapsed behind it (`doc/design_error_management.md` D7). Derived
/// entries are *presentation*-collapsed only — on the canvas every node still
/// badges its own error, because when looking at a node the user wants to know
/// why *it* is dark.
class ErrorGroup {
  ErrorGroup(this.root);
  final APIValidationError root;
  final List<APIValidationError> derived = [];
}

/// Whether an error entry can be navigated to (it is anchored to a node).
bool isNavigableError(APIValidationError e) => e.nodeId != null;

/// The address of the node an entry is anchored to, as a comparable key.
/// `hostNetwork` is set only on a cross-network root cause; every other entry
/// addresses a node of the network it is listed under.
String errorNodeKey(String networkName, APIValidationError e) =>
    '${e.hostNetwork ?? networkName}|${e.scopePath.join(',')}|${e.nodeId}';

/// The address of an entry's *root cause*, in the same key space as
/// [errorNodeKey] — so a derived entry finds the row representing its root
/// **node**, which may be that node's validation row rather than an eval row (a
/// derived chain routinely terminates at a cone-poisoned node whose synthesized
/// eval entry the D8 dedupe drops).
String? errorRootKey(APIValidationError e) {
  final root = e.rootCause;
  if (root == null) return null;
  return '${root.hostNetwork}|${root.scopePath.join(',')}|${root.nodeId}';
}

/// Groups [errors] into one entry per underlying problem: root causes (and
/// every validation row) stay top level; each derived eval entry is filed
/// behind the row representing its root-cause node.
///
/// A derived entry whose root is not represented in this list — its row was
/// dropped, or the root lives in a network whose rows are listed elsewhere — is
/// promoted to top level rather than hidden. Losing an error is worse than
/// showing it twice.
List<ErrorGroup> groupErrorsByRootCause(
    String networkName, List<APIValidationError> errors) {
  final groups = <ErrorGroup>[];
  final byNode = <String, ErrorGroup>{};
  for (final e in errors) {
    if (e.rootCause != null) continue;
    final group = ErrorGroup(e);
    groups.add(group);
    // Several rows can represent one node (e.g. two validation errors); the
    // first one becomes the collapse parent.
    byNode.putIfAbsent(errorNodeKey(networkName, e), () => group);
  }
  for (final e in errors) {
    if (e.rootCause == null) continue;
    final parent = byNode[errorRootKey(e)];
    if (parent != null) {
      parent.derived.add(e);
    } else {
      groups.add(ErrorGroup(e));
    }
  }
  return groups;
}

/// The bracketed tag list for one entry: severity, then which pipeline
/// produced it, then staleness. Mirrors what the picker encodes as colour and
/// icon — a plain-text report has neither channel available, so it spells them
/// out.
String _tags(APIValidationError e, {bool derived = false}) {
  final parts = <String>[
    if (derived) 'downstream',
    e.blocking ? 'blocking' : 'warning',
    e.source == APIErrorSource.evaluation ? 'runtime' : 'structural',
    if (e.stale) 'from last evaluation',
  ];
  return '[${parts.join(' · ')}]';
}

/// Where the entry lives: the node it is anchored to (id + label), its host
/// network when that differs from the listing one (a cross-network root
/// cause), and its HOF-body qualifier.
String _location(String networkName, APIValidationError e) {
  final buffer = StringBuffer();
  final nodeId = e.nodeId;
  if (nodeId == null) {
    buffer.write('network-level');
  } else {
    buffer.write('node #$nodeId');
    final label = e.nodeLabel;
    if (label != null && label.isNotEmpty) buffer.write(' `$label`');
  }
  final host = e.hostNetwork;
  if (host != null && host != networkName) buffer.write(' in $host');
  final qualifier = e.bodyQualifier;
  if (qualifier != null && qualifier.isNotEmpty) buffer.write(' ($qualifier)');
  return buffer.toString();
}

/// Keeps a multi-line message readable as one list item by indenting its
/// continuation lines under the bullet.
String _indentContinuations(String text, String indent) =>
    text.split('\n').join('\n$indent');

/// One entry as a Markdown list item. [derived] entries are nested one level
/// under their root cause, matching the picker's indentation.
String formatErrorEntry(
  String networkName,
  APIValidationError e, {
  bool derived = false,
}) {
  final indent = derived ? '  ' : '';
  final text = _indentContinuations(e.errorText, '$indent    ');
  return '$indent- ${_tags(e, derived: derived)} '
      '${_location(networkName, e)} — $text';
}

/// One network's problems as Markdown list items (no heading), root causes
/// with their downstream cone nested underneath. Returns an empty list when
/// the network is clean.
List<String> formatNetworkErrorLines(
    String networkName, List<APIValidationError> errors) {
  final lines = <String>[];
  for (final group in groupErrorsByRootCause(networkName, errors)) {
    lines.add(formatErrorEntry(networkName, group.root));
    for (final derived in group.derived) {
      lines.add(formatErrorEntry(networkName, derived, derived: true));
    }
  }
  return lines;
}

/// One network's problems as a standalone report, with a heading — what the
/// panel error picker's *Copy all* action puts on the clipboard.
String formatNetworkErrorReport(
    String networkName, List<APIValidationError> errors) {
  final lines = formatNetworkErrorLines(networkName, errors);
  if (lines.isEmpty) return 'atomCAD — no problems in `$networkName`.';
  final count = countProblems(networkName, errors);
  return [
    'atomCAD — $count ${_plural(count, 'problem')} in `$networkName`',
    '',
    ...lines,
  ].join('\n');
}

/// The whole design's problems, grouped by network — what *Edit > Copy all
/// problems* puts on the clipboard.
///
/// Networks are listed in the panel's own order so the report reads like the
/// screen. A clean design still produces text (rather than an empty clipboard),
/// because "I copied the problems and got nothing" is a useful thing to be able
/// to paste too.
String formatDesignErrorReport(List<APINetworkWithValidationErrors> networks) {
  final withErrors =
      networks.where((n) => n.validationErrors.isNotEmpty).toList();
  if (withErrors.isEmpty) return 'atomCAD — no problems reported.';

  var total = 0;
  final sections = <String>[];
  for (final network in withErrors) {
    total += countProblems(network.name, network.validationErrors);
    sections.add([
      '## ${network.name}',
      ...formatNetworkErrorLines(network.name, network.validationErrors),
    ].join('\n'));
  }

  return [
    'atomCAD — $total ${_plural(total, 'problem')} in '
        '${withErrors.length} ${_plural(withErrors.length, 'network')}',
    '',
    sections.join('\n\n'),
  ].join('\n');
}

/// How many *problems* a list of entries represents — one per root cause, with
/// its downstream cone collapsed behind it, exactly as the badge counts them.
///
/// [networkName] is **not** optional even though it only affects grouping keys:
/// with the wrong name, a derived entry fails to find its root and is promoted
/// to top level, inflating the count.
int countProblems(String networkName, List<APIValidationError> errors) =>
    groupErrorsByRootCause(networkName, errors).length;

String _plural(int n, String singular) => n == 1 ? singular : '${singular}s';
