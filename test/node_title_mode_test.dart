/// The one invariant of the node title mode that a test can hold onto:
/// **flipping it must not change a node's footprint**
/// (`doc/design_node_names_in_ui.md` D5).
///
/// Node size is derived from the *type* — pin counts, the stored body size —
/// and a footprint change triggers a reflow of the whole network
/// (`doc/design_reflow_on_footprint_change.md`). If the header grew to fit a
/// long name, flipping the mode would shuffle the canvas, and "toggle quickly
/// and it behaves as if both were shown" would stop being true.
///
/// This lives in Dart rather than Rust deliberately: `rendered_node_size` never
/// sees preferences, so a Rust assertion would pass vacuously. The rule that
/// actually paints is `NodeWidget`'s own layout, so that is what is pumped.
///
/// The other half of the mode — *which* string each node kind shows — is
/// asserted directly on `nodeTitleText` / `nodeTitleLabel`, which is where the
/// per-kind rule lives.
library;

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int32List;
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_title.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

/// 40 characters — the design's stated worst case, and far wider than
/// `BASE_NODE_WIDTH` can render at any zoom.
const String _LONG_NAME = 'a_deliberately_very_long_node_name_here_';

NodeView _node({
  String typeName = 'xray',
  String? customName,
  String? closureCustomLabel,
  String? commentLabel,
}) =>
    NodeView(
      id: BigInt.from(1),
      nodeTypeName: typeName,
      customName: customName,
      position: APIVec2(x: 0, y: 0),
      inputPins: const [
        InputPinView(
            name: 'in', dataType: 'Geometry', multi: false, connected: false),
      ],
      outputType: 'Geometry',
      outputPins: const [
        OutputPinView(
          name: 'out',
          dataType: 'Geometry',
          resolvedViaFallback: false,
          index: 0,
        ),
      ],
      displayedPins: Int32List(0),
      functionType: '',
      functionPinConsumed: false,
      selected: false,
      active: false,
      displayed: false,
      returnNode: false,
      outputPinStrings: const [],
      commentAnchors: const [],
      closureCustomLabel: closureCustomLabel,
      commentLabel: commentLabel,
    );

NodeNetworkView _network(NodeView node) => NodeNetworkView(
      name: 'main',
      nodes: {node.id: node},
      wires: const [],
    );

/// Pump one [NodeWidget] under [mode] and return the size its `Container`
/// actually took.
Future<Size> _pumpNodeSize(
    WidgetTester tester, NodeView node, NodeTitleMode mode) async {
  final view = _network(node);
  final resolver = ScopeResolver(
    root: view,
    panOffset: Offset.zero,
    scale: getZoomScale(ZoomLevel.normal),
    zoomLevel: ZoomLevel.normal,
  );
  await tester.pumpWidget(MaterialApp(
    home: Stack(children: [
      NodeWidget(
        node: node,
        panOffset: Offset.zero,
        zoomLevel: ZoomLevel.normal,
        rootView: view,
        resolver: resolver,
        titleMode: mode,
      ),
    ]),
  ));
  // The widget's root is a `Positioned`; the box under it is the node itself.
  return tester.getSize(find
      .descendant(
        of: find.byType(NodeWidget),
        matching: find.byType(Container),
      )
      .first);
}

void main() {
  group('the title mode never changes a node footprint (D5)', () {
    testWidgets('a 40-character name renders in the type name\'s box',
        (tester) async {
      final node = _node(customName: _LONG_NAME);

      final typeSize = await _pumpNodeSize(tester, node, NodeTitleMode.type);
      final nameSize = await _pumpNodeSize(tester, node, NodeTitleMode.name);

      expect(nameSize, typeSize);
      expect(tester.takeException(), isNull);
    });

    testWidgets('the long name ellipsizes rather than overflowing',
        (tester) async {
      final node = _node(customName: _LONG_NAME);
      await _pumpNodeSize(tester, node, NodeTitleMode.name);

      final header = tester.widget<Text>(find.text(_LONG_NAME));
      expect(header.overflow, TextOverflow.ellipsis);
      // An overflowing RenderFlex reports through the exception channel rather
      // than by failing layout, so the absence of one is the assertion.
      expect(tester.takeException(), isNull);
    });

    testWidgets('the size rule itself is mode-blind', (tester) async {
      // `ScopeResolver.effectiveNodeSizeLogical` takes no mode and must not
      // start taking one — this is the seam a future "make the header fit"
      // change would break, and `test/layout_size_parity_test.dart` (the
      // Rust↔Dart size contract) depends on it staying blind.
      final node = _node(customName: _LONG_NAME);
      final view = _network(node);
      final resolver = ScopeResolver(
        root: view,
        panOffset: Offset.zero,
        scale: getZoomScale(ZoomLevel.normal),
        zoomLevel: ZoomLevel.normal,
      );
      expect(resolver.effectiveNodeSizeLogical(node, const []),
          getNodeSize(node, ZoomLevel.normal));
    });
  });

  group('what each node kind shows (D4)', () {
    test('a builtin shows the simple type name, then the node name', () {
      final node = _node(typeName: 'builtin.xray', customName: 'xray1');
      expect(nodeTitleText(node, NodeTitleMode.type), 'xray');
      expect(nodeTitleText(node, NodeTitleMode.name), 'xray1');
    });

    test('a custom-network instance shows the instance name, not the type', () {
      final node = _node(typeName: '0_styling', customName: '0_styling1');
      expect(nodeTitleText(node, NodeTitleMode.type), '0_styling');
      expect(nodeTitleText(node, NodeTitleMode.name), '0_styling1');
    });

    test('a namespaced custom type keeps only its simple name in Type mode',
        () {
      final node = _node(typeName: 'lib.parts.bracket', customName: 'bracket3');
      expect(nodeTitleText(node, NodeTitleMode.type), 'bracket');
    });

    test("a closure's own label is replaced by the name", () {
      final node = _node(
          typeName: 'closure', customName: 'closure2', closureCustomLabel: 'f');
      expect(nodeTitleLabel(node, NodeTitleMode.type, 'f'), 'f');
      expect(nodeTitleLabel(node, NodeTitleMode.name, 'f'), 'closure2');
    });

    test('an unlabelled closure has no label segment in Type mode', () {
      final node = _node(typeName: 'closure', customName: 'closure2');
      expect(nodeTitleLabel(node, NodeTitleMode.type, ''), '');
      // ...but always has one in Name mode, which is what makes a comment's
      // normally-hidden header appear.
      expect(nodeTitleLabel(node, NodeTitleMode.name, ''), 'closure2');
    });

    test('an untitled comment gains a header in Name mode', () {
      final node = _node(typeName: 'Comment', customName: 'comment3');
      expect(nodeTitleLabel(node, NodeTitleMode.type, ''), '');
      expect(nodeTitleLabel(node, NodeTitleMode.name, ''), 'comment3');
    });

    test('a node with no stored name still says something', () {
      // Never reachable through `add_node` or the loader, both of which mint a
      // name — the fallback exists so a title bar is never blank.
      final node = _node(typeName: 'xray');
      expect(nodeTitleText(node, NodeTitleMode.name), '#1');
    });
  });
}
