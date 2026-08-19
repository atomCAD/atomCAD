/// Tests for the canvas content bounding box that frames an image export
/// (`lib/structure_designer/node_network/node_network_content.dart`).
///
/// The framing is the whole difference between "export the network" and "export
/// the region I selected", and it is pure geometry over the network view — no
/// Rust, no rendering — so it is worth pinning here rather than discovering in a
/// 6000-pixel PNG.
///
/// Sizes are deliberately never asserted exactly: node heights come from
/// `getNodeSize`, whose constants change with layout work. What the export
/// depends on is the *relationships* — a selection frames its selected nodes,
/// spans them however far apart they are, and ignores everything else.
library;

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int32List;
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_content.dart';

NodeView _node(int id, double x, double y, {bool selected = false}) => NodeView(
      id: BigInt.from(id),
      nodeTypeName: 'sphere',
      position: APIVec2(x: x, y: y),
      inputPins: const [],
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
      selected: selected,
      active: false,
      displayed: false,
      returnNode: false,
      outputPinStrings: const [],
    );

NodeNetworkView _network(List<NodeView> nodes) => NodeNetworkView(
      name: 'test_network',
      nodes: {for (final node in nodes) node.id: node},
      wires: const [],
    );

void main() {
  const zoom = ZoomLevel.normal;

  test('an empty network has no bounds to frame', () {
    expect(nodeNetworkContentBounds(view: _network([]), zoomLevel: zoom),
        Rect.zero);
  });

  test('the whole-network box contains every node', () {
    final view = _network([
      _node(1, 0, 0),
      _node(2, 4000, 2500),
      _node(3, -600, -300),
    ]);

    final bounds = nodeNetworkContentBounds(view: view, zoomLevel: zoom);

    expect(bounds.left, lessThan(-600));
    expect(bounds.top, lessThan(-300));
    expect(bounds.right, greaterThan(4000));
    expect(bounds.bottom, greaterThan(2500));
  });

  test('the selection box spans far-apart selected nodes', () {
    // The point of the feature: a shift-drag selection need not fit on one
    // screen, and the frame is the selection's common bounding box.
    final view = _network([
      _node(1, 0, 0, selected: true),
      _node(2, 5000, 3000, selected: true),
      _node(3, 2000, 1000),
    ]);

    final bounds = nodeNetworkContentBounds(
        view: view, zoomLevel: zoom, selectedOnly: true);

    expect(bounds.left, lessThanOrEqualTo(0));
    expect(bounds.top, lessThanOrEqualTo(0));
    expect(bounds.right, greaterThan(5000));
    expect(bounds.bottom, greaterThan(3000));
  });

  test('an unselected outlier does not stretch the selection box', () {
    final selectedOnly = _network([
      _node(1, 0, 0, selected: true),
      _node(2, 300, 200, selected: true),
      _node(3, 9000, 7000),
    ]);

    final selectionBounds = nodeNetworkContentBounds(
        view: selectedOnly, zoomLevel: zoom, selectedOnly: true);
    final wholeBounds =
        nodeNetworkContentBounds(view: selectedOnly, zoomLevel: zoom);

    expect(selectionBounds.right, lessThan(2000));
    expect(selectionBounds.bottom, lessThan(2000));
    // …and this is what lets a selection export at a zoom level the whole
    // network is too large for.
    expect(selectionBounds.width, lessThan(wholeBounds.width));
    expect(selectionBounds.height, lessThan(wholeBounds.height));
  });

  test('selecting everything frames the same region as the whole network', () {
    final view = _network([
      _node(1, 0, 0, selected: true),
      _node(2, 800, 400, selected: true),
    ]);

    expect(
      nodeNetworkContentBounds(view: view, zoomLevel: zoom, selectedOnly: true),
      nodeNetworkContentBounds(view: view, zoomLevel: zoom),
    );
  });

  test('no selection frames nothing, so the dialog can gate on the count', () {
    final view = _network([_node(1, 0, 0), _node(2, 500, 500)]);

    expect(countSelectedTopLevelNodes(view), 0);
    expect(
        nodeNetworkContentBounds(
            view: view, zoomLevel: zoom, selectedOnly: true),
        Rect.zero);
  });

  test('the margin widens the box evenly on all four sides', () {
    // The export dialog's Margin field lands here, so a margin that only
    // widened (say) the right edge would put the content off-centre in every
    // exported image.
    final view = _network([_node(1, 100, 100)]);

    final tight =
        nodeNetworkContentBounds(view: view, zoomLevel: zoom, margin: 0);
    final loose =
        nodeNetworkContentBounds(view: view, zoomLevel: zoom, margin: 40);

    expect(loose.left, tight.left - 40);
    expect(loose.top, tight.top - 40);
    expect(loose.right, tight.right + 40);
    expect(loose.bottom, tight.bottom + 40);
  });

  test('a zero margin frames the nodes exactly', () {
    final view = _network([_node(1, 250, 175)]);

    final bounds =
        nodeNetworkContentBounds(view: view, zoomLevel: zoom, margin: 0);

    expect(bounds.left, 250);
    expect(bounds.top, 175);
    expect(bounds.width, greaterThan(0));
    expect(bounds.height, greaterThan(0));
  });

  test('the selected-node count drives the checkbox label', () {
    expect(countSelectedTopLevelNodes(_network([])), 0);
    expect(
        countSelectedTopLevelNodes(_network([
          _node(1, 0, 0, selected: true),
          _node(2, 100, 0),
          _node(3, 200, 0, selected: true),
        ])),
        2);
  });
}
