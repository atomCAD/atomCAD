/// Tests for the snapping guides of a node drag
/// (`lib/structure_designer/node_network/snap_guides.dart`,
/// `doc/design_deliberate_node_placement.md` B1).
///
/// Snap resolution is pure geometry, so most of this drives [SnapSession]
/// directly with hand-made boxes. The model-level group checks the glue that
/// a session cannot see: the grabbed node alone snaps and the selection
/// follows it, a snap never sticks, Alt bypasses it, and a drag tick never
/// notifies listeners. The snapping *feel* is a manual walkthrough step.
library;

import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int32List;
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';
import 'package:flutter_cad/structure_designer/node_network/snap_guides.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

const _grabbed = Size(160, 60);

SnapSession _session(List<Rect> candidates,
        {List<SnapWire> wires = const [], double scale = 1.0}) =>
    SnapSession(
        grabbedSize: _grabbed,
        candidates: candidates,
        wires: wires,
        scale: scale);

NodeView _node(int id, double x, double y,
        {bool selected = false,
        String type = 'sphere',
        int inputs = 0,
        ZoneView? zone}) =>
    NodeView(
      id: BigInt.from(id),
      nodeTypeName: type,
      position: APIVec2(x: x, y: y),
      inputPins: [
        for (var i = 0; i < inputs; i++)
          InputPinView(
            name: 'in$i',
            dataType: 'Geometry',
            multi: false,
            connected: false,
          ),
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
      commentAnchors: const [],
      functionType: '',
      functionPinConsumed: false,
      hasOpenInputPort: false,
      selected: selected,
      active: false,
      displayed: false,
      returnNode: false,
      outputPinStrings: const [],
      zone: zone,
    );

WireView _wire(int source, int dest, {int destParam = 0}) => WireView(
      sourceNodeId: BigInt.from(source),
      sourceOutputPinIndex: 0,
      destNodeId: BigInt.from(dest),
      destParamIndex: BigInt.from(destParam),
      selected: false,
      destinationArgumentKind: APIArgumentKind.external_,
      sourcePin: const APISourcePin.nodeOutput(pinIndex: 0),
      sourceScopeDepth: 0,
    );

NodeNetworkView _network(List<NodeView> nodes,
        {List<WireView> wires = const []}) =>
    NodeNetworkView(
      name: 'test_network',
      nodes: {for (final node in nodes) node.id: node},
      wires: wires,
      readOnly: false,
    );

ScopeResolver _resolver(NodeNetworkView view, {Offset pan = Offset.zero}) =>
    ScopeResolver(
        root: view, panOffset: pan, scale: 1.0, zoomLevel: ZoomLevel.normal);

void main() {
  group('threshold', () {
    final target = Rect.fromLTWH(1000, 0, 160, 60);

    test('6 screen px at scale 1: 5 px snaps, 7 px does not', () {
      final session = _session([target]);
      expect(session.resolve(const Offset(1005, 400)).position.dx, 1000);
      expect(session.resolve(const Offset(1007, 400)).position.dx, 1007);
    });

    test('is in screen px: zoomed out to 0.6, 9 logical px still snaps', () {
      final session = _session([target], scale: 0.6);
      // 6 / 0.6 = 10 logical px.
      expect(session.resolve(const Offset(1009, 400)).position.dx, 1000);
      expect(session.resolve(const Offset(1011, 400)).position.dx, 1011);
    });

    test('an axis out of range keeps the raw coordinate', () {
      final session = _session([target]);
      final result = session.resolve(const Offset(1003, 412.5));
      expect(result.position, const Offset(1000, 412.5));
      expect(result.xKind, SnapKind.edge);
      expect(result.yKind, isNull);
    });
  });

  group('kinds', () {
    test('left = left and right = right draw a line through both boxes', () {
      final c = Rect.fromLTWH(100, 100, 200, 60);
      final session = _session([c]);
      var result = session.resolve(const Offset(102, 400));
      expect(result.position.dx, 100);
      final line = result.guides.single as SnapLineGuide;
      expect(line.from, const Offset(100, 100));
      expect(line.to, const Offset(100, 460));

      // Right edge: 300 - 160 = 140.
      result = session.resolve(const Offset(143, 400));
      expect(result.position.dx, 140);
      expect((result.guides.single as SnapLineGuide).from.dx, 300);
    });

    test('top = top and bottom = bottom', () {
      final c = Rect.fromLTWH(100, 100, 160, 80);
      final session = _session([c]);
      expect(session.resolve(const Offset(600, 97)).position.dy, 100);
      // Bottom: 180 - 60 = 120.
      expect(session.resolve(const Offset(600, 124)).position.dy, 120);
    });

    test('the normal gap right of a neighbour, labelled 50', () {
      final c = Rect.fromLTWH(0, 0, 160, 60);
      final session = _session([c]);
      final result = session.resolve(const Offset(212, 10));
      expect(result.position.dx, 210);
      final bracket = result.guides.whereType<SnapGapGuide>().single;
      expect(bracket.label, '50');
      expect(bracket.from.dx, 160);
      expect(bracket.to.dx, 210);
    });

    test('the normal gap below a neighbour, labelled 30', () {
      final c = Rect.fromLTWH(0, 0, 160, 60);
      final session = _session([c]);
      final result = session.resolve(const Offset(20, 93));
      expect(result.position.dy, 90);
      expect(result.guides.whereType<SnapGapGuide>().single.label, '30');
    });

    test('a gap needs a neighbour: no overlap on the other axis, no snap', () {
      final c = Rect.fromLTWH(0, 0, 160, 60);
      final session = _session([c]);
      // Right of c by 50, but far below it.
      final result = session.resolve(const Offset(212, 500));
      expect(result.position.dx, 212);
    });

    test('equal spacing repeats the gap of the pair being joined', () {
      final a = Rect.fromLTWH(0, 0, 160, 60);
      final b = Rect.fromLTWH(202, 0, 160, 60); // gap 42
      final session = _session([a, b]);
      // Raw 403: the normal gap right of b (412) is out of range.
      final result = session.resolve(const Offset(403, 7));
      expect(result.position.dx, 404); // 362 + 42
      final brackets = result.guides.whereType<SnapGapGuide>().toList();
      expect(brackets.map((g) => g.label), ['= 42', '= 42']);
      expect(brackets[0].from.dx, 160);
      expect(brackets[0].to.dx, 202);
      expect(brackets[1].from.dx, 362);
      expect(brackets[1].to.dx, 404);
    });

    test('equal spacing works on the left end and down a column too', () {
      final a = Rect.fromLTWH(500, 0, 160, 60);
      final b = Rect.fromLTWH(500, 80, 160, 60); // gap 20 down
      final session = _session([a, b]);
      // Below b: 140 + 20.
      expect(session.resolve(const Offset(553, 158)).position.dy, 160);
      // Above a: 0 - 20 - 60.
      expect(session.resolve(const Offset(550, -78)).position.dy, -80);
    });

    test('a straight wire: the grabbed pin level with the other end', () {
      const wire =
          SnapWire(otherPin: Offset(160, 50), grabbedPinOffset: Offset(0, 44));
      final session = _session(const [], wires: [wire]);
      final result = session.resolve(const Offset(300, 9));
      expect(result.position.dy, 6); // 50 - 44
      expect(result.yKind, SnapKind.straightWire);
      final guide = result.guides.single as SnapWireGuide;
      expect(guide.from, const Offset(160, 50));
      expect(guide.to, const Offset(300, 50));
    });
  });

  group('priority', () {
    test('straight wire beats an edge, even a nearer one', () {
      final c = Rect.fromLTWH(1000, 10, 160, 60);
      const wire =
          SnapWire(otherPin: Offset(0, 50), grabbedPinOffset: Offset(0, 46));
      final result = _session([c], wires: [wire]).resolve(const Offset(300, 9));
      // The edge (top 10) is 1 px away; the wire (top 4) 5 px.
      expect(result.position.dy, 4);
      expect(result.yKind, SnapKind.straightWire);
    });

    test('an edge beats a gap', () {
      // Left of c2 at 213 vs. normal gap right of c1 at 210.
      final c1 = Rect.fromLTWH(0, 0, 160, 60);
      final c2 = Rect.fromLTWH(213, 900, 160, 60);
      final result = _session([c1, c2]).resolve(const Offset(210, 0));
      expect(result.position.dx, 213);
      expect(result.xKind, SnapKind.edge);
    });

    test('a gap beats equal spacing', () {
      // The pair's gap is 46: equal spacing asks for 366 + 46 = 412, the
      // normal gap right of b for 366 + 50 = 416. Raw 411 is nearer the
      // former, and both are in range.
      final a = Rect.fromLTWH(0, 0, 160, 60);
      final b = Rect.fromLTWH(206, 0, 160, 60);
      final result = _session([a, b]).resolve(const Offset(411, 10));
      expect(result.position.dx, 416); // 366 + 50
      expect(result.xKind, SnapKind.gap);
    });

    test('within one kind, the nearest wins', () {
      final c1 = Rect.fromLTWH(100, 500, 160, 60);
      final c2 = Rect.fromLTWH(104, 900, 160, 60);
      expect(_session([c1, c2]).resolve(const Offset(103, 0)).position.dx, 104);
      expect(_session([c1, c2]).resolve(const Offset(101, 0)).position.dx, 100);
    });
  });

  group('buildSnapSession', () {
    test('candidates exclude the moving selection and nodes off screen', () {
      final view = _network([
        _node(1, 0, 0),
        _node(2, 1000, 0, selected: true), // grabbed
        _node(3, 2000, 0, selected: true), // moving with it
        _node(4, 3, 400), // on screen
        _node(5, 5000, 5000), // off screen
      ]);
      final session = buildSnapSession(
        resolver: _resolver(view),
        scopeChain: const [],
        grabbedId: BigInt.from(2),
        movingIds: {BigInt.from(2), BigInt.from(3)},
        viewport: const Rect.fromLTWH(0, 0, 1600, 900),
      )!;
      // Left = left with node 1 (or 4); node 3 is moving, node 5 off screen.
      expect(session.resolve(const Offset(2003, 700)).position.dx, 2003);
      expect(session.resolve(const Offset(5002, 700)).position.dx, 5002);
      expect(session.resolve(const Offset(4, 700)).position.dx, 3);
    });

    test('a comment is a candidate', () {
      final comment = _node(1, 300, 300, type: 'Comment');
      final view = _network([comment, _node(2, 0, 0)]);
      final session = buildSnapSession(
        resolver: _resolver(view),
        scopeChain: const [],
        grabbedId: BigInt.from(2),
        movingIds: {BigInt.from(2)},
        viewport: const Rect.fromLTWH(0, 0, 1600, 900),
      )!;
      expect(session.resolve(const Offset(303, 0)).position.dx, 300);
    });

    test('a body node snaps to body siblings only, in body coordinates', () {
      final zone = ZoneView(
        nodes: {
          BigInt.from(1): _node(1, 40, 40),
          BigInt.from(2): _node(2, 400, 200),
        },
        wires: const [],
        zoneInputPins: const [],
        zoneOutputPins: const [],
        storedWidth: 800,
        storedHeight: 600,
        collapseMode: APICollapseMode.expanded,
        collapsable: false,
        collapsed: false,
        bodySceneEvaluable: false,
      );
      final view = _network([
        _node(7, 0, 0, type: 'map', zone: zone),
        _node(8, 500, 1000), // top level: not a candidate in the body
      ]);
      final session = buildSnapSession(
        resolver: _resolver(view),
        scopeChain: [BigInt.from(7)],
        grabbedId: BigInt.from(2),
        movingIds: {BigInt.from(2)},
        viewport: const Rect.fromLTWH(0, 0, 1600, 1200),
      )!;
      expect(session.resolve(const Offset(42, 300)).position.dx, 40);
      expect(session.resolve(const Offset(502, 300)).position.dx, 502);
    });

    test('a wire to a staying node becomes a straight-wire target', () {
      final view = _network(
        [_node(1, 0, 0), _node(2, 400, 37, inputs: 1)],
        wires: [_wire(1, 2)],
      );
      final resolver = _resolver(view);
      final session = buildSnapSession(
        resolver: resolver,
        scopeChain: const [],
        grabbedId: BigInt.from(2),
        movingIds: {BigInt.from(2)},
        viewport: const Rect.fromLTWH(0, 0, 1600, 900),
      )!;
      // Both pins sit at the same offset below their node's top here (one
      // output, one input), so the straight wire puts the tops level.
      final result = session.resolve(const Offset(400, 4));
      expect(result.yKind, SnapKind.straightWire);
      final guide = result.guides.whereType<SnapWireGuide>().single;
      expect(guide.from.dy, guide.to.dy);
    });
  });

  group('model', () {
    StructureDesignerModel model0(NodeNetworkView view) {
      final model = StructureDesignerModel()..nodeNetworkView = view;
      model.snapCanvas =
          () => (resolver: _resolver(view), canvasSize: const Size(1600, 900));
      return model;
    }

    test('only the grabbed node snaps; the selection follows by its delta', () {
      final view = _network([
        _node(1, 0, 0),
        _node(2, 300, 300, selected: true),
        _node(3, 600, 500, selected: true),
      ]);
      final model = model0(view);
      // Grab node 2 and move it to within 4 px of node 1's left edge.
      model.dragSelectedNodes(const Offset(-296, 0),
          grabbedNodeId: BigInt.from(2));
      expect(view.nodes[BigInt.from(2)]!.position.x, 0);
      // Node 3 moved by the same, snapped, delta: -300.
      expect(view.nodes[BigInt.from(3)]!.position.x, 300);
      expect(model.snapResult, isNotNull);
    });

    test('a snap does not stick: the pointer leaves it after the threshold',
        () {
      final view = _network([_node(1, 0, 0), _node(2, 300, 300)]);
      final model = model0(view);
      model.dragNodePosition(BigInt.from(2), const Offset(-297, 0));
      expect(view.nodes[BigInt.from(2)]!.position.x, 0); // raw 3
      model.dragNodePosition(BigInt.from(2), const Offset(5, 0));
      // Raw is now 8, out of range: the node is back under the pointer.
      expect(view.nodes[BigInt.from(2)]!.position.x, 8);
      expect(model.snapResult, isNull);
    });

    testWidgets('Alt bypasses snapping; a drag tick never notifies',
        (tester) async {
      final view = _network([_node(1, 0, 0), _node(2, 300, 300)]);
      final model = model0(view);
      var notified = 0;
      model.addListener(() => notified++);
      final ticks = model.dragRepaint.value;

      await simulateKeyDownEvent(LogicalKeyboardKey.altLeft);
      model.dragNodePosition(BigInt.from(2), const Offset(-297, 0));
      expect(view.nodes[BigInt.from(2)]!.position.x, 3);
      expect(model.snapResult, isNull);
      await simulateKeyUpEvent(LogicalKeyboardKey.altLeft);

      model.dragNodePosition(BigInt.from(2), const Offset(0, 0));
      expect(view.nodes[BigInt.from(2)]!.position.x, 0);

      expect(notified, 0);
      expect(model.dragRepaint.value, ticks + 2);
    });
  });
}
