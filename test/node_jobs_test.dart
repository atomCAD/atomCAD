/// The Flutter side of node jobs (`doc/design_background_node_jobs.md`,
/// Phase 4): the poll loop, the job lookup, and the panel's run row.
///
/// Everything here takes plain callbacks and generated API data classes, so
/// no Rust library is needed. Time is `tester.pump(duration)` — fake time —
/// never the wall clock.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/chemisorb_editor.dart';
import 'package:flutter_cad/structure_designer/node_jobs.dart';

APINodeJobStatus status({
  int jobId = 1,
  int documentId = 1,
  String networkName = 'Main',
  List<int> scope = const [],
  int nodeId = 7,
  String phase = 'Relaxing',
  int done = 0,
  int? total,
  bool cancelling = false,
}) {
  final scopePath = Uint64List(scope.length);
  for (var i = 0; i < scope.length; i++) {
    scopePath[i] = BigInt.from(scope[i]);
  }
  return APINodeJobStatus(
    jobId: BigInt.from(jobId),
    documentId: BigInt.from(documentId),
    networkName: networkName,
    scopePath: scopePath,
    nodeId: BigInt.from(nodeId),
    label: 'Chemisorption search',
    phase: phase,
    done: BigInt.from(done),
    total: total == null ? null : BigInt.from(total),
    cancelling: cancelling,
  );
}

APINodeJobOutcome outcome(int jobId,
        [APINodeJobOutcomeKind kind = APINodeJobOutcomeKind.finished]) =>
    APINodeJobOutcome(
      jobId: BigInt.from(jobId),
      documentId: BigInt.one,
      networkName: 'Main',
      nodeId: BigInt.from(7),
      label: 'Chemisorption search',
      kind: kind,
      message: 'done',
    );

APINodeJobPoll pollResult({
  List<APINodeJobStatus> running = const [],
  List<APINodeJobOutcome> finished = const [],
  int pendingInstalls = 0,
}) =>
    APINodeJobPoll(
      running: running,
      finished: finished,
      pendingInstalls: pendingInstalls,
      activeChanged: finished.isNotEmpty,
    );

/// A scripted kernel: each poll takes the next result (the last one
/// repeats) and records the `defer` it was given.
class FakeKernel {
  FakeKernel(this.script);

  final List<APINodeJobPoll> script;
  final List<bool> defers = [];

  APINodeJobPoll poll(bool defer) {
    defers.add(defer);
    final i = defers.length - 1;
    return script[i < script.length ? i : script.length - 1];
  }
}

void main() {
  group('NodeJobPoller', () {
    testWidgets('does not poll before start, then every 100 ms',
        (tester) async {
      final kernel = FakeKernel([
        pollResult(running: [status()])
      ]);
      final poller = NodeJobPoller(
        poll: kernel.poll,
        interactionOpen: () => false,
        onOutcome: (_) {},
      );

      await tester.pump(const Duration(milliseconds: 500));
      expect(kernel.defers, isEmpty);

      poller.start();
      await tester.pump(const Duration(milliseconds: 99));
      expect(kernel.defers, isEmpty);
      await tester.pump(const Duration(milliseconds: 1));
      expect(kernel.defers.length, 1);
      await tester.pump(const Duration(milliseconds: 300));
      expect(kernel.defers.length, 4);

      // A second start while running does not add a second timer.
      poller.start();
      await tester.pump(const Duration(milliseconds: 100));
      expect(kernel.defers.length, 5);
      poller.stop();
    });

    testWidgets('passes interactionOpen() as defer', (tester) async {
      final kernel = FakeKernel([
        pollResult(running: [status()])
      ]);
      var open = true;
      final poller = NodeJobPoller(
        poll: kernel.poll,
        interactionOpen: () => open,
        onOutcome: (_) {},
      )..start();

      await tester.pump(const Duration(milliseconds: 100));
      open = false;
      await tester.pump(const Duration(milliseconds: 100));
      expect(kernel.defers, [true, false]);
      poller.stop();
    });

    testWidgets(
        'keeps polling while jobs run, outcomes arrive, or installs are '
        'pending; stops on the first idle poll', (tester) async {
      final kernel = FakeKernel([
        pollResult(running: [status()]),
        pollResult(finished: [outcome(1)], running: [status(jobId: 2)]),
        // Nothing running, but a result is held back (D11): the stranded-
        // result case. The loop must not stop here.
        pollResult(pendingInstalls: 1),
        pollResult(pendingInstalls: 1),
        pollResult(finished: [outcome(2)]),
        pollResult(),
      ]);
      final poller = NodeJobPoller(
        poll: kernel.poll,
        interactionOpen: () => false,
        onOutcome: (_) {},
      )..start();

      await tester.pump(const Duration(milliseconds: 500));
      expect(kernel.defers.length, 5);
      expect(poller.isActive, isTrue);
      await tester.pump(const Duration(milliseconds: 100));
      expect(kernel.defers.length, 6);
      expect(poller.isActive, isFalse);

      await tester.pump(const Duration(seconds: 1));
      expect(kernel.defers.length, 6, reason: 'stopped');

      // A new job resumes it.
      poller.start();
      await tester.pump(const Duration(milliseconds: 100));
      expect(kernel.defers.length, 7);
      poller.stop();
    });

    testWidgets('delivers every outcome to onOutcome exactly once',
        (tester) async {
      final kernel = FakeKernel([
        pollResult(running: [status()]),
        pollResult(
          finished: [
            outcome(1),
            outcome(2, APINodeJobOutcomeKind.cancelled),
          ],
          running: [status(jobId: 3)],
        ),
        pollResult(finished: [outcome(3, APINodeJobOutcomeKind.failed)]),
        pollResult(),
      ]);
      final seen = <BigInt>[];
      final poller = NodeJobPoller(
        poll: kernel.poll,
        interactionOpen: () => false,
        onOutcome: (o) => seen.add(o.jobId),
      )..start();

      await tester.pump(const Duration(seconds: 1));
      expect(seen, [BigInt.one, BigInt.two, BigInt.from(3)]);
      expect(poller.isActive, isFalse);
    });
  });

  group('findNodeJob', () {
    final jobs = [
      status(jobId: 1, documentId: 1, networkName: 'Main', nodeId: 7),
      status(jobId: 2, documentId: 1, networkName: 'Other', nodeId: 9),
      status(jobId: 3, documentId: 2, networkName: 'Main', nodeId: 11),
      status(jobId: 4, documentId: 1, networkName: 'Main', scope: [5]),
    ];

    BigInt? find(int? doc, String? net, List<int> scope, int node) =>
        findNodeJob(jobs, doc == null ? null : BigInt.from(doc), net,
                [for (final s in scope) BigInt.from(s)], BigInt.from(node))
            ?.jobId;

    test('matches document + network + scope + node id', () {
      expect(find(1, 'Main', [], 7), BigInt.one);
      expect(find(1, 'Other', [], 9), BigInt.two);
      expect(find(2, 'Main', [], 11), BigInt.from(3));
      expect(find(1, 'Main', [5], 7), BigInt.from(4));
    });

    test('a job on the same node id elsewhere is not found', () {
      // Same document, another network.
      expect(find(1, 'Other', [], 7), isNull);
      // Same network name and id, another document.
      expect(find(2, 'Main', [], 7), isNull);
      // Same network and id, another scope.
      expect(find(1, 'Main', [6], 7), isNull);
      expect(find(1, 'Main', [5, 1], 7), isNull);
      // No active document or network.
      expect(find(null, 'Main', [], 7), isNull);
      expect(find(1, null, [], 7), isNull);
    });
  });

  group('progress text', () {
    test('words and fraction', () {
      expect(nodeJobProgressText(status(done: 37, total: 121)),
          'Relaxing 37 / 121 (31 %)');
      expect(nodeJobFraction(status(done: 37, total: 121)),
          closeTo(37 / 121, 1e-9));
      expect(nodeJobProgressText(status(phase: 'Planning')), 'Planning…');
      expect(nodeJobFraction(status(phase: 'Planning')), isNull);
      expect(nodeJobFraction(status(total: 0)), isNull);
      expect(nodeJobProgressText(status(cancelling: true, total: 9)),
          'Cancelling…');
      expect(nodeJobTooltip(status(done: 37, total: 121)),
          'Chemisorption search — 31 %');
    });

    test('documents with jobs', () {
      expect(
          documentsWithNodeJobs(
              [status(documentId: 1), status(documentId: 3, jobId: 2)]),
          {BigInt.one, BigInt.from(3)});
    });
  });

  group('ChemisorbRunRow', () {
    Future<void> pumpRow(WidgetTester tester, APINodeJobStatus? job,
        {VoidCallback? onRun, VoidCallback? onCancel}) {
      return tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: ChemisorbRunRow(
            report: null,
            job: job,
            onRun: onRun ?? () {},
            onCancel: onCancel,
          ),
        ),
      ));
    }

    testWidgets('idle shows Run, and Run fires', (tester) async {
      var runs = 0;
      await pumpRow(tester, null, onRun: () => runs++);
      expect(find.text('Run'), findsOneWidget);
      expect(find.text('Cancel'), findsNothing);
      expect(find.byType(LinearProgressIndicator), findsNothing);
      await tester.tap(find.text('Run'));
      expect(runs, 1);
    });

    testWidgets('planning: an indeterminate bar', (tester) async {
      await pumpRow(tester, status(phase: 'Planning'), onCancel: () {});
      final bar = tester.widget<LinearProgressIndicator>(
          find.byType(LinearProgressIndicator));
      expect(bar.value, isNull);
      expect(find.text('Planning…'), findsOneWidget);
      expect(find.text('Run'), findsNothing);
    });

    testWidgets('with a total: progress in words, a bar, and Cancel',
        (tester) async {
      var cancels = 0;
      await pumpRow(tester, status(done: 37, total: 121),
          onCancel: () => cancels++);
      expect(find.text('Relaxing 37 / 121 (31 %)'), findsOneWidget);
      final bar = tester.widget<LinearProgressIndicator>(
          find.byType(LinearProgressIndicator));
      expect(bar.value, closeTo(37 / 121, 1e-9));
      await tester.tap(find.text('Cancel'));
      expect(cancels, 1);
    });

    testWidgets('cancelling: the text says so and the button is disabled',
        (tester) async {
      var cancels = 0;
      await pumpRow(tester, status(done: 3, total: 9, cancelling: true),
          onCancel: () => cancels++);
      expect(find.text('Cancelling…'), findsOneWidget);
      final button = tester.widget<ButtonStyleButton>(find.ancestor(
          of: find.text('Cancel'),
          matching: find.byWidgetPredicate((w) => w is ButtonStyleButton)));
      expect(button.onPressed, isNull);
      await tester.tap(find.text('Cancel'));
      expect(cancels, 0);
    });
  });
}
