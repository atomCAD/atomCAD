/// Node jobs on the Flutter side (`doc/design_background_node_jobs.md`).
///
/// An explicit, expensive action on one node — `chemisorb`'s Run — runs on a
/// Rust worker pool. Flutter starts it (`model.startNodeJob`), polls it, and
/// shows it in three places: the property panel, the node's title bar and the
/// document tab. Rust owns every rule (routing a result to its document,
/// deferring an install, classifying an outcome); this file holds only the
/// Flutter-side logic that is worth testing, and none of it is a widget:
///
/// - [NodeJobPoller] — the 100 ms poll loop and its stop rule (D5);
/// - [findNodeJob] — which job, if any, belongs to a node on screen;
/// - [nodeJobProgressText] / [nodeJobFraction] — the progress in words and
///   as a bar value, shared by the panel and the node badge.
///
/// It takes plain callbacks and the generated API data classes, so
/// `test/node_jobs_test.dart` runs it without the Rust library.
library;

import 'dart:async';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// How often [NodeJobPoller] polls while it runs (D5).
const Duration NODE_JOB_POLL_PERIOD = Duration(milliseconds: 100);

/// The poll loop of the node jobs.
///
/// Polls only while there is something to poll for: [start] begins a
/// `Timer.periodic`, and the loop stops itself on the first poll that reports
/// no running job, no outcome **and no pending install**. The last condition
/// matters: a job that finished while an interaction was open stays in its
/// Rust slot (D11) with nothing running, and a loop that stopped on "nothing
/// running" would strand it there until the next Run.
class NodeJobPoller {
  NodeJobPoller({
    required this.poll,
    required this.interactionOpen,
    required this.onOutcome,
    this.period = NODE_JOB_POLL_PERIOD,
  });

  /// One poll; `defer` holds successful results back (D11, Flutter's half).
  final APINodeJobPoll Function(bool defer) poll;

  /// Whether an interaction only Flutter knows about is in progress — a
  /// pointer held down, a focused text field, a dialog or menu, a wire drag.
  final bool Function() interactionOpen;

  /// Called once for every job that ended, in the order the poll lists them.
  final void Function(APINodeJobOutcome outcome) onOutcome;

  final Duration period;

  Timer? _timer;

  /// Whether the loop is running.
  bool get isActive => _timer != null;

  /// Starts the loop; a no-op while it already runs. Called when a job is
  /// started.
  void start() {
    _timer ??= Timer.periodic(period, (_) => _tick());
  }

  /// Stops the loop (the host's `dispose`).
  void stop() {
    _timer?.cancel();
    _timer = null;
  }

  void _tick() {
    final result = poll(interactionOpen());
    final idle = result.running.isEmpty &&
        result.finished.isEmpty &&
        result.pendingInstalls == 0;
    if (idle) stop();
    for (final outcome in result.finished) {
      onOutcome(outcome);
    }
  }
}

/// The job running on node [nodeId] in [scopeChain] of network
/// [networkName] in document [documentId], or `null`.
///
/// All four must match. Node ids are unique only within one network (and one
/// scope of it), and a document holds many networks: matching on the
/// document and the id alone would badge the same-id node of whatever other
/// network is shown.
APINodeJobStatus? findNodeJob(
  List<APINodeJobStatus> statuses,
  BigInt? documentId,
  String? networkName,
  List<BigInt> scopeChain,
  BigInt nodeId,
) {
  if (documentId == null || networkName == null) return null;
  for (final status in statuses) {
    if (status.documentId == documentId &&
        status.networkName == networkName &&
        status.nodeId == nodeId &&
        _sameScope(status, scopeChain)) {
      return status;
    }
  }
  return null;
}

bool _sameScope(APINodeJobStatus status, List<BigInt> scopeChain) {
  final path = status.scopePath;
  if (path.length != scopeChain.length) return false;
  for (var i = 0; i < path.length; i++) {
    if (path[i] != scopeChain[i]) return false;
  }
  return true;
}

/// The documents that have a job running, for the tab spinner.
Set<BigInt> documentsWithNodeJobs(List<APINodeJobStatus> statuses) =>
    {for (final status in statuses) status.documentId};

/// The job's progress as a bar value in `[0, 1]`, or `null` while the
/// amount of work is not known yet (indeterminate).
double? nodeJobFraction(APINodeJobStatus status) {
  final total = status.total;
  if (total == null || total == BigInt.zero) return null;
  final fraction = status.done.toDouble() / total.toDouble();
  return fraction.clamp(0.0, 1.0);
}

/// The job's progress in words: "Relaxing 37 / 121 (31 %)", the bare phase
/// ("Planning") while the total is unknown, "Cancelling…" once cancel was
/// asked for.
String nodeJobProgressText(APINodeJobStatus status) {
  if (status.cancelling) return 'Cancelling…';
  final phase = status.phase.isEmpty ? 'Starting' : status.phase;
  final total = status.total;
  final fraction = nodeJobFraction(status);
  if (total == null || fraction == null) return '$phase…';
  return '$phase ${status.done} / $total (${(fraction * 100).round()} %)';
}

/// The node badge's tooltip: "Chemisorption search — 31 %".
String nodeJobTooltip(APINodeJobStatus status) {
  // Just started: the first poll has not told the label yet.
  if (status.label.isEmpty) return 'Starting…';
  if (status.cancelling) return '${status.label} — cancelling…';
  final fraction = nodeJobFraction(status);
  if (fraction == null) {
    return status.phase.isEmpty
        ? status.label
        : '${status.label} — ${status.phase.toLowerCase()}…';
  }
  return '${status.label} — ${(fraction * 100).round()} %';
}
