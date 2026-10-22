/// Tests for the open-port `+` tab placement
/// (`lib/structure_designer/node_network/open_port_tabs.dart`,
/// `doc/design_wire_declared_inputs.md` Phase 4).
///
/// Which nodes get a tab and where it sits depend only on the network view,
/// so they are pinned here; the drag feedback and the drop itself need a
/// running app and belong to the manual walkthrough.
library;

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int32List;
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/open_port_tabs.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

InputPinView _inputPin(int i) => InputPinView(
      name: 'in$i',
      dataType: 'Float',
      multi: false,
      connected: false,
    );

NodeView _node(
  int id,
  double x,
  double y, {
  bool openPort = false,
  int inputs = 0,
  String type = 'expr',
  ZoneView? zone,
}) =>
    NodeView(
      id: BigInt.from(id),
      nodeTypeName: type,
      position: APIVec2(x: x, y: y),
      inputPins: List.generate(inputs, _inputPin),
      outputType: 'Float',
      outputPins: const [
        OutputPinView(
          name: 'out',
          dataType: 'Float',
          resolvedViaFallback: false,
          index: 0,
        ),
      ],
      displayedPins: Int32List(0),
      commentAnchors: const [],
      functionType: '',
      functionPinConsumed: false,
      hasOpenInputPort: openPort,
      selected: false,
      active: false,
      displayed: false,
      returnNode: false,
      outputPinStrings: const [],
      zone: zone,
    );

/// A `map`-like HOF at (0, 0) whose body holds [bodyNodes].
NodeView _hof(int id, List<NodeView> bodyNodes) => _node(
      id,
      0,
      0,
      type: 'map',
      inputs: 1,
      zone: ZoneView(
        zoneInputPins: const [],
        zoneOutputPins: const [],
        nodes: {for (final n in bodyNodes) n.id: n},
        wires: const [],
        storedWidth: 300,
        storedHeight: 150,
        collapseMode: APICollapseMode.auto,
        collapsed: false,
        collapsable: true,
        bodySceneEvaluable: false,
      ),
    );

ScopeResolver _resolver(List<NodeView> nodes,
    {ZoomLevel zoom = ZoomLevel.normal, Offset pan = Offset.zero}) {
  return ScopeResolver(
    root: NodeNetworkView(
      name: 'test_network',
      nodes: {for (final n in nodes) n.id: n},
      wires: const [],
      readOnly: false,
    ),
    panOffset: pan,
    scale: getZoomScale(zoom),
    zoomLevel: zoom,
  );
}

void main() {
  test('only nodes with an open port get a tab', () {
    final tabs = openPortTabs(_resolver([
      _node(1, 0, 0, openPort: true, inputs: 2),
      _node(2, 400, 0, type: 'sphere', inputs: 2),
      _node(3, 0, 300, openPort: true),
    ]));

    expect(tabs.map((t) => t.nodeId).toSet(), {BigInt.from(1), BigInt.from(3)});
    expect(tabs.every((t) => t.scopeChain.isEmpty), isTrue);
  });

  test('a tab sits directly below its node, at the input-pin column', () {
    final node = _node(1, 100, 50, openPort: true, inputs: 3);
    final resolver = _resolver([node], pan: const Offset(20, 10));
    final tab = openPortTabs(resolver).single;

    final topLeft = resolver.scopedToScreen(const [], const Offset(100, 50));
    final size = getNodeSize(node, ZoomLevel.normal);

    expect(tab.rect.top, closeTo(topLeft.dy + size.height, 1e-9));
    expect(tab.rect.left, closeTo(topLeft.dx + OPEN_PORT_TAB_LEFT_INSET, 1e-9));
    expect(tab.rect.left, lessThan(topLeft.dx + size.width / 2));
    expect(
        tab.rect.size, const Size(OPEN_PORT_TAB_WIDTH, OPEN_PORT_TAB_HEIGHT));
  });

  test('the tab scales with the zoom level', () {
    final node = _node(1, 100, 50, openPort: true, inputs: 1);
    final normal = openPortTabs(_resolver([node])).single;
    final medium =
        openPortTabs(_resolver([node], zoom: ZoomLevel.zoomedOutMedium)).single;
    final scale = getZoomScale(ZoomLevel.zoomedOutMedium);

    expect(medium.rect.width, closeTo(normal.rect.width * scale, 1e-9));
    expect(medium.rect.height, closeTo(normal.rect.height * scale, 1e-9));
    final mediumSize = getNodeSize(node, ZoomLevel.zoomedOutMedium);
    expect(medium.rect.top, closeTo(50 * scale + mediumSize.height, 1e-9));
  });

  test('a node in a visible body gets a tab in the body scope', () {
    final hof = _hof(10, [_node(1, 20, 20, openPort: true)]);
    final tabs = openPortTabs(_resolver([hof]));

    expect(tabs, hasLength(1));
    expect(tabs.single.nodeId, BigInt.from(1));
    expect(tabs.single.scopeChain, [BigInt.from(10)]);
  });

  test('a node inside a collapsed body gets no tab', () {
    // At the far zoom level the body renders below the collapse threshold, so
    // its content is not drawn — and its nodes must not offer a drop target.
    final hof = _hof(10, [_node(1, 20, 20, openPort: true)]);
    final resolver = _resolver([hof], zoom: ZoomLevel.zoomedOutFar);
    expect(resolver.isBodyCollapsed([BigInt.from(10)]), isTrue);

    expect(openPortTabs(resolver), isEmpty);
  });

  test('the tab does not change the node footprint', () {
    for (final zoom in ZoomLevel.values) {
      expect(
        getNodeSize(_node(1, 0, 0, openPort: true, inputs: 2), zoom),
        getNodeSize(_node(1, 0, 0, inputs: 2), zoom),
      );
    }
  });
}
