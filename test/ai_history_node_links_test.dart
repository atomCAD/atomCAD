/// The AI History panel's node links (`doc/design_node_names_in_ui.md` D12).
///
/// The two leaf widgets that own a path — the *Diff* tab's hunk title and the
/// *Layout* tab's moved-node row — take a plain `onJumpToNode` callback rather
/// than reaching for the model, so this needs no kernel. What is *not* asserted
/// here is whether a path resolves: that is `resolve_node_path`'s job and is
/// tested in Rust, which is the point of having exactly one path rule.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/ai_history_api.dart';
import 'package:flutter_cad/structure_designer/ai_history_panel.dart';
import 'package:flutter_cad/structure_designer/node_network/node_name_link.dart';

APIDiffHunk _hunk(String nodePath,
        {APIDiffHunkKind kind = APIDiffHunkKind.added}) =>
    APIDiffHunk(
      kind: kind,
      nodePath: nodePath,
      lines: const [
        APIDiffLine(tag: APIDiffLineTag.add, text: 'd = expr { a: \$element }'),
      ],
    );

APIMovedNode _moved(String path) => APIMovedNode(
      path: path,
      depth: path.contains('/') ? 1 : 0,
      before: const APIVec2(x: 0, y: 0),
      after: const APIVec2(x: 40, y: 0),
      displacement: 40,
    );

/// Pumps [child] with the Material ancestry a `Tooltip` needs.
Future<void> _pump(WidgetTester tester, Widget child) => tester.pumpWidget(
      MaterialApp(home: Scaffold(body: child)),
    );

void main() {
  group('diff hunk title', () {
    testWidgets('a node path is a link that reports the path and the network',
        (tester) async {
      final jumps = <(String, String)>[];
      await _pump(
        tester,
        AiHistoryDiffHunk(
          hunk: _hunk('m1/d'),
          network: '0_styling',
          onJumpToNode: (network, path) => jumps.add((network, path)),
        ),
      );

      await tester.tap(find.text('m1/d'));
      expect(jumps, [('0_styling', 'm1/d')]);
    });

    testWidgets('an empty path renders the (hunk) placeholder, not a link',
        (tester) async {
      final jumps = <(String, String)>[];
      await _pump(
        tester,
        AiHistoryDiffHunk(
          hunk: _hunk('', kind: APIDiffHunkKind.changed),
          network: 'main',
          onJumpToNode: (network, path) => jumps.add((network, path)),
        ),
      );

      expect(find.text('(hunk)'), findsOneWidget);
      await tester.tap(find.text('(hunk)'));
      expect(jumps, isEmpty);
    });

    testWidgets('a Text-diff `@@` range header is not a link', (tester) async {
      final jumps = <(String, String)>[];
      await _pump(
        tester,
        AiHistoryDiffHunk(
          hunk: _hunk('@@ -1,3 +1,4 @@', kind: APIDiffHunkKind.changed),
          network: 'main',
          onJumpToNode: (network, path) => jumps.add((network, path)),
        ),
      );

      await tester.tap(find.text('@@ -1,3 +1,4 @@'));
      expect(jumps, isEmpty);
    });

    testWidgets('a diff line is not a link', (tester) async {
      final jumps = <(String, String)>[];
      await _pump(
        tester,
        AiHistoryDiffHunk(
          hunk: _hunk('m1/d'),
          network: 'main',
          onJumpToNode: (network, path) => jumps.add((network, path)),
        ),
      );

      await tester.tap(find.textContaining('d = expr'));
      expect(jumps, isEmpty);
    });

    testWidgets('without a callback the title is plain text', (tester) async {
      await _pump(
        tester,
        AiHistoryDiffHunk(hunk: _hunk('m1/d'), network: 'main'),
      );

      expect(find.text('m1/d'), findsOneWidget);
      expect(find.byType(GestureDetector), findsNothing);
    });
  });

  group('moved node row', () {
    testWidgets('the path cell links, the coordinates do not', (tester) async {
      final jumps = <(String, String)>[];
      await _pump(
        tester,
        AiHistoryMovedNodeRow(
          moved: _moved('m1/e'),
          network: 'main',
          onJumpToNode: (network, path) => jumps.add((network, path)),
        ),
      );

      await tester.tap(find.textContaining('(0, 0) →'));
      expect(jumps, isEmpty);

      await tester.tap(find.text('m1/e'));
      expect(jumps, [('main', 'm1/e')]);
    });
  });

  group('isLinkable', () {
    test('separates the two non-paths from real paths', () {
      expect(NodeNameLink.isLinkable('e1'), isTrue);
      expect(NodeNameLink.isLinkable('m1/e1'), isTrue);
      // Interior whitespace is legal in a node name, so this is a real path.
      expect(NodeNameLink.isLinkable('my node/e1'), isTrue);
      expect(NodeNameLink.isLinkable(''), isFalse);
      expect(NodeNameLink.isLinkable('@@ -1,3 +1,4 @@'), isFalse);
    });
  });
}
