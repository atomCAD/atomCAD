/// Unit tests for the problem-report formatter (`lib/structure_designer/
/// error_report.dart`, issue #359).
///
/// This is a **pure Dart** test — `error_report.dart` only touches the
/// generated FRB *data classes*, never the Rust library, so nothing needs to be
/// loaded and `flutter test test/error_report_test.dart` runs in well under a
/// second. It is deliberately not an integration test (see `AGENTS.md`: the
/// Flutter smoke test is human-only).
///
/// What is worth testing here is the **root-cause grouping**, not the wording:
/// a derived entry must be filed behind its root (so the count reads as one
/// problem, not three), and a derived entry whose root is missing from the list
/// must be promoted rather than dropped.
library;

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/error_report.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:flutter_test/flutter_test.dart';

APIValidationError _error(
  String text, {
  int? nodeId,
  String? nodeLabel,
  bool blocking = true,
  APIErrorSource source = APIErrorSource.validation,
  bool stale = false,
  String? bodyQualifier,
  String? hostNetwork,
  APIErrorRootCause? rootCause,
}) {
  return APIValidationError(
    errorText: text,
    blocking: blocking,
    source: source,
    stale: stale,
    scopePath: Uint64List(0),
    nodeId: nodeId == null ? null : BigInt.from(nodeId),
    nodeLabel: nodeLabel,
    bodyQualifier: bodyQualifier,
    hostNetwork: hostNetwork,
    rootCause: rootCause,
  );
}

APIErrorRootCause _root(String hostNetwork, int nodeId, String label) {
  return APIErrorRootCause(
    hostNetwork: hostNetwork,
    scopePath: Uint64List(0),
    nodeId: BigInt.from(nodeId),
    nodeLabel: label,
    errorText: 'root failed',
  );
}

void main() {
  group('groupErrorsByRootCause', () {
    test('files a derived entry behind its root, counting one problem', () {
      final root =
          _error('radius is not connected', nodeId: 12, nodeLabel: 'sphere');
      final derived = _error(
        'error in a input (from sphere #12): radius is not connected',
        nodeId: 14,
        nodeLabel: 'union',
        source: APIErrorSource.evaluation,
        rootCause: _root('net', 12, 'sphere'),
      );

      final groups = groupErrorsByRootCause('net', [root, derived]);

      expect(groups, hasLength(1));
      expect(groups.single.root.nodeId, BigInt.from(12));
      expect(groups.single.derived, hasLength(1));
      expect(countProblems('net', [root, derived]), 1);
    });

    test('promotes a derived entry whose root is not in the list', () {
      // Losing an error is worse than showing it twice.
      final orphan = _error(
        'error in a input (from sphere #12): …',
        nodeId: 14,
        nodeLabel: 'union',
        source: APIErrorSource.evaluation,
        rootCause: _root('otherNet', 12, 'sphere'),
      );

      final groups = groupErrorsByRootCause('net', [orphan]);

      expect(groups, hasLength(1));
      expect(groups.single.root.nodeId, BigInt.from(14));
      expect(groups.single.derived, isEmpty);
    });

    test('counts two independent failures as two problems', () {
      final a = _error('a broke', nodeId: 1, nodeLabel: 'sphere');
      final b = _error('b broke', nodeId: 2, nodeLabel: 'relax');
      expect(countProblems('net', [a, b]), 2);
    });
  });

  group('formatErrorEntry', () {
    test('spells out severity, source and staleness as tags', () {
      final e = _error(
        'did not converge',
        nodeId: 3,
        nodeLabel: 'relax',
        source: APIErrorSource.evaluation,
        stale: true,
      );
      final line = formatErrorEntry('net', e);
      expect(line,
          '- [blocking · runtime · from last evaluation] node #3 `relax` — did not converge');
    });

    test('marks a warning and carries the body qualifier', () {
      final e = _error(
        'could not parse override',
        nodeId: 7,
        nodeLabel: 'materialize',
        blocking: false,
        bodyQualifier: 'in map1 body',
      );
      expect(
        formatErrorEntry('net', e),
        '- [warning · structural] node #7 `materialize` (in map1 body) — '
        'could not parse override',
      );
    });

    test('names the host network only when it differs from the listing one',
        () {
      final same = _error('x', nodeId: 1, nodeLabel: 'n', hostNetwork: 'net');
      final other =
          _error('x', nodeId: 1, nodeLabel: 'n', hostNetwork: 'myPart');
      expect(formatErrorEntry('net', same), isNot(contains(' in ')));
      expect(formatErrorEntry('net', other), contains('`n` in myPart'));
    });

    test('handles a network-level entry with no node to anchor to', () {
      final e = _error('network references an invalid node network');
      expect(formatErrorEntry('net', e), contains('network-level'));
    });

    test('indents a derived entry and tags it downstream', () {
      final e = _error('error in a input', nodeId: 14, nodeLabel: 'union');
      final line = formatErrorEntry('net', e, derived: true);
      expect(line, startsWith('  - [downstream · '));
    });
  });

  group('formatDesignErrorReport', () {
    test('renders a clean design as text rather than an empty clipboard', () {
      expect(
        formatDesignErrorReport([
          APINetworkWithValidationErrors(
              name: 'net', validationErrors: const [])
        ]),
        'atomCAD — no problems reported.',
      );
    });

    test('headlines the totals and sections by network', () {
      final report = formatDesignErrorReport([
        APINetworkWithValidationErrors(
          name: 'a.b.first',
          validationErrors: [
            _error('one broke', nodeId: 1, nodeLabel: 'sphere'),
            _error('two broke', nodeId: 2, nodeLabel: 'relax'),
          ],
        ),
        APINetworkWithValidationErrors(
            name: 'clean', validationErrors: const []),
        APINetworkWithValidationErrors(
          name: 'second',
          validationErrors: [_error('three broke', nodeId: 3, nodeLabel: 'x')],
        ),
      ]);

      expect(report, startsWith('atomCAD — 3 problems in 2 networks'));
      expect(report, contains('## a.b.first'));
      expect(report, contains('## second'));
      // A network with no problems contributes no section at all.
      expect(report, isNot(contains('## clean')));
    });

    test('singularises the headline', () {
      final report = formatDesignErrorReport([
        APINetworkWithValidationErrors(
          name: 'only',
          validationErrors: [_error('boom', nodeId: 1, nodeLabel: 'sphere')],
        ),
      ]);
      expect(report, startsWith('atomCAD — 1 problem in 1 network'));
    });
  });
}
