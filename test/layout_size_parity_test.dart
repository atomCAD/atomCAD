/// The Flutter half of the node-size parity fixture
/// (`doc/design_incremental_layout.md`, "Prerequisite: one size function").
///
/// Rust now has a single `rendered_node_size`, and it has to agree with the
/// rule that actually paints — `ScopeResolver.effectiveNodeSizeLogical` and
/// `_computeBodySize`. They have drifted before, and the drift was invisible
/// until a node overlapped something on screen.
///
/// So this is not an assertion test. It is the **generator** for the other
/// half of a two-file contract under `rust/tests/fixtures/`:
///
/// | File | Written by | Read by |
/// |---|---|---|
/// | `layout_size_parity_input.json` | the Rust test | this test |
/// | `layout_size_parity.json` | this test | the Rust test |
///
/// The input describes one network in *view* terms — pin counts, stored body
/// sizes, the resolved collapsed flag, body-local positions. This test rebuilds
/// the `NodeView` tree from it, runs the real `ScopeResolver`, and writes every
/// node's logical size back keyed by name path.
/// `layout_size_test.rs::rendered_node_size_matches_flutter` then asserts
/// equality to the pixel and fails if this file was not re-run.
///
/// Refresh cycle when either side changes:
///
/// ```text
/// cargo test -p atomcad-structure-designer --test structure_designer layout_size
/// flutter test test/layout_size_parity_test.dart
/// cargo test -p atomcad-structure-designer --test structure_designer layout_size
/// ```
///
/// No Rust FFI is involved — `ScopeResolver` is pure Dart over the view types,
/// which is what makes this runnable as an ordinary `flutter_test`.
library;

import 'dart:convert';
import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int32List;
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

/// `rust/tests/fixtures/`, relative to the Flutter project root the test runs
/// from.
Directory get _fixtures => Directory('rust/tests/fixtures');

File get _inputFile => File('${_fixtures.path}/layout_size_parity_input.json');
File get _sizesFile => File('${_fixtures.path}/layout_size_parity.json');

/// Ids are assigned as the tree is walked. They are private to this test — the
/// contract is keyed by name path — but `ScopeResolver` keys its layout cache
/// by id chain, so they have to be unique within each scope.
int _nextId = 1;

/// One dummy pin. Only the *count* of pins is read by the size rule, so the
/// names and types are arbitrary; keeping them uniform makes a diff of this
/// file about the shape of the network rather than about pin metadata.
InputPinView _inputPin(int i) => InputPinView(
      name: 'in$i',
      dataType: 'Int',
      multi: false,
      connected: false,
    );

OutputPinView _outputPin(int i) => OutputPinView(
      name: 'out$i',
      dataType: 'Int',
      resolvedViaFallback: false,
      index: i,
    );

/// Rebuild one `NodeView` (and, recursively, its body) from the Rust-generated
/// description.
NodeView _nodeFrom(Map<String, dynamic> json) {
  final id = BigInt.from(_nextId++);
  final position = (json['position'] as List).cast<num>();
  final inputPins = json['inputPins'] as int;
  final outputPins = json['outputPins'] as int;
  final comment = json['comment'] as List?;
  final zoneJson = json['zone'] as Map<String, dynamic>?;

  ZoneView? zone;
  if (zoneJson != null) {
    final children = <BigInt, NodeView>{};
    for (final child
        in (zoneJson['nodes'] as List).cast<Map<String, dynamic>>()) {
      final view = _nodeFrom(child);
      children[view.id] = view;
    }
    zone = ZoneView(
      zoneInputPins:
          List.generate(zoneJson['zoneInputPins'] as int, (i) => _outputPin(i)),
      zoneOutputPins:
          List.generate(zoneJson['zoneOutputPins'] as int, (i) => _inputPin(i)),
      nodes: children,
      wires: const [],
      storedWidth: (zoneJson['storedWidth'] as num).toDouble(),
      storedHeight: (zoneJson['storedHeight'] as num).toDouble(),
      collapseMode: APICollapseMode.auto,
      // The Rust side resolves the collapse state before writing the fixture
      // (`resolve_body_collapsed` already returns false for a non-collapsable
      // type), so `collapsable` is unconditionally true here and the effective
      // flag is carried by `collapsed` alone — exactly the split Flutter's
      // `compactHof` expects.
      collapsed: zoneJson['collapsed'] as bool,
      collapsable: true,
      bodySceneEvaluable: false,
    );
  }

  return NodeView(
    id: id,
    nodeTypeName: json['nodeTypeName'] as String,
    customName: (json['path'] as String).split('/').last,
    position: APIVec2(x: position[0].toDouble(), y: position[1].toDouble()),
    inputPins: List.generate(inputPins, _inputPin),
    outputType: 'Int',
    outputPins: List.generate(outputPins, _outputPin),
    displayedPins: Int32List(0),
    functionType: '',
    functionPinConsumed: false,
    selected: false,
    active: false,
    displayed: false,
    returnNode: false,
    outputPinStrings: const [],
    subtitle: (json['hasSubtitle'] as bool) ? 'subtitle' : null,
    commentWidth: comment == null ? null : (comment[0] as num).toDouble(),
    commentHeight: comment == null ? null : (comment[1] as num).toDouble(),
    commentAnchors: const [],
    zone: zone,
  );
}

/// Walk the rebuilt tree in the same order the description lists it, recording
/// `name path -> [width, height]`.
void _measure(
  ScopeResolver resolver,
  List<Map<String, dynamic>> descriptions,
  Map<BigInt, NodeView> views,
  List<BigInt> chain,
  Map<String, List<double>> out,
) {
  final byName = {
    for (final view in views.values) view.customName!: view,
  };
  for (final description in descriptions) {
    final path = description['path'] as String;
    final view = byName[path.split('/').last]!;
    final size = resolver.effectiveNodeSizeLogical(view, chain);
    out[path] = [size.width, size.height];

    final zoneJson = description['zone'] as Map<String, dynamic>?;
    if (zoneJson != null) {
      _measure(
        resolver,
        (zoneJson['nodes'] as List).cast<Map<String, dynamic>>(),
        view.zone!.nodes,
        [...chain, view.id],
        out,
      );
    }
  }
}

void main() {
  test('regenerates layout_size_parity.json from the Rust-described network',
      () {
    expect(
      _inputFile.existsSync(),
      isTrue,
      reason: '${_inputFile.path} is missing. Run the Rust layout_size test '
          'first — it writes the network description this test consumes.',
    );

    final input =
        jsonDecode(_inputFile.readAsStringSync()) as Map<String, dynamic>;
    final descriptions = (input['nodes'] as List).cast<Map<String, dynamic>>();

    _nextId = 1;
    final roots = <BigInt, NodeView>{};
    for (final description in descriptions) {
      final view = _nodeFrom(description);
      roots[view.id] = view;
    }

    // Logical sizes, so the transform must be the identity: `scale != 1` would
    // fold a zoom factor into the numbers Rust compares against.
    final resolver = ScopeResolver(
      root: NodeNetworkView(name: 'parity', nodes: roots, wires: const []),
      panOffset: Offset.zero,
      scale: 1.0,
      zoomLevel: ZoomLevel.normal,
    );

    final sizes = <String, List<double>>{};
    _measure(resolver, descriptions, roots, const [], sizes);

    // Sorted, so the checked-in file has a stable diff.
    final ordered = Map.fromEntries(
      sizes.entries.toList()..sort((a, b) => a.key.compareTo(b.key)),
    );
    _sizesFile.writeAsStringSync(
      '${const JsonEncoder.withIndent('  ').convert(ordered)}\n',
    );

    // A sanity floor, not the contract: the contract is the Rust-side equality.
    expect(sizes, isNotEmpty);
    for (final entry in sizes.entries) {
      expect(entry.value[0], greaterThan(0),
          reason: '${entry.key} has no width');
      expect(entry.value[1], greaterThan(0),
          reason: '${entry.key} has no height');
    }
  });
}
