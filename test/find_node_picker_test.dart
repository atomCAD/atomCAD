/// The Find Node picker's own behaviour — filtering, the keyboard, the scope
/// switch (`doc/design_node_names_in_ui.md` D7).
///
/// The picker takes a plain search callback rather than reaching for the model,
/// so this needs no kernel: the fake below is a list of matches and a substring
/// filter. What is *not* asserted here is the ranking or the path spelling —
/// those live in Rust (`find_nodes_by_name`) and are tested there, which is the
/// point of having exactly one path rule.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/find_node_picker.dart';

APINodeNameMatch _match(String namePath,
        {String network = 'main', String type = 'expr'}) =>
    APINodeNameMatch(
      network: network,
      scopePath: Uint64List.fromList(const []),
      nodeId: BigInt.from(namePath.hashCode.abs() % 1000),
      namePath: namePath,
      nodeTypeName: type,
    );

final List<APINodeNameMatch> _activeNetworkNodes = [
  _match('e1'),
  _match('e1_extra'),
  _match('map4/e1'),
  _match('chassis', type: 'union'),
];

final List<APINodeNameMatch> _otherNetworkNodes = [
  _match('e1_elsewhere', network: 'styling'),
];

/// Stands in for `find_nodes_by_name`: a case-insensitive substring filter over
/// a fixed list, widened by the *all networks* flag. The ordering is whatever
/// the fixture list has — ranking is Rust's job.
List<APINodeNameMatch> _search(String query, bool allNetworks) {
  final pool = [
    ..._activeNetworkNodes,
    if (allNetworks) ..._otherNetworkNodes,
  ];
  final needle = query.trim().toLowerCase();
  return pool.where((m) => m.namePath.toLowerCase().contains(needle)).toList();
}

Future<void> _pumpPicker(
  WidgetTester tester, {
  ValueChanged<APINodeNameMatch>? onPick,
  VoidCallback? onClose,
  ValueChanged<bool>? onAllNetworksChanged,
}) async {
  await tester.pumpWidget(MaterialApp(
    home: Scaffold(
      body: Align(
        alignment: Alignment.topCenter,
        child: SizedBox(
          width: 400,
          child: FindNodePicker(
            search: _search,
            activeNetwork: 'main',
            onPick: onPick ?? (_) {},
            onClose: onClose ?? () {},
            onAllNetworksChanged: onAllNetworksChanged,
          ),
        ),
      ),
    ),
  ));
  await tester.pump();
}

/// The name paths currently rendered, in row order.
List<String> _renderedPaths(WidgetTester tester) {
  final paths = <String>[];
  for (final m in [..._activeNetworkNodes, ..._otherNetworkNodes]) {
    if (find.text(m.namePath).evaluate().isNotEmpty) paths.add(m.namePath);
  }
  return paths;
}

void main() {
  testWidgets('opens listing every node of the active network', (tester) async {
    await _pumpPicker(tester);

    // An empty query is a name directory (D8), and nothing from another
    // network is in it until the switch is flipped.
    expect(_renderedPaths(tester), ['e1', 'e1_extra', 'map4/e1', 'chassis']);
    expect(find.text('e1_elsewhere'), findsNothing);
  });

  testWidgets('typing filters the list', (tester) async {
    await _pumpPicker(tester);

    await tester.enterText(find.byKey(const Key('find_node_field')), 'e1');
    await tester.pump();
    expect(_renderedPaths(tester), ['e1', 'e1_extra', 'map4/e1']);

    await tester.enterText(find.byKey(const Key('find_node_field')), 'chas');
    await tester.pump();
    expect(_renderedPaths(tester), ['chassis']);

    await tester.enterText(
        find.byKey(const Key('find_node_field')), 'nonesuch');
    await tester.pump();
    expect(_renderedPaths(tester), isEmpty);
    expect(find.text('No matching node'), findsOneWidget);
  });

  testWidgets('Enter jumps to the highlighted match, Up/Down move it and wrap',
      (tester) async {
    final picked = <String>[];
    await _pumpPicker(tester, onPick: (m) => picked.add(m.namePath));

    // The highlight starts on the first (best-ranked) row.
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(picked, ['e1']);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(picked, ['e1', 'map4/e1']);

    // Up from the top wraps to the bottom…
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(picked, ['e1', 'map4/e1', 'chassis']);

    // …and Down from the bottom wraps back to the top.
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(picked, ['e1', 'map4/e1', 'chassis', 'e1']);
  });

  testWidgets('typing resets the highlight to the best match', (tester) async {
    final picked = <String>[];
    await _pumpPicker(tester, onPick: (m) => picked.add(m.namePath));

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.enterText(find.byKey(const Key('find_node_field')), 'e1');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(picked, ['e1']);
  });

  testWidgets('a click on a row jumps to it', (tester) async {
    final picked = <String>[];
    await _pumpPicker(tester, onPick: (m) => picked.add(m.namePath));

    await tester.tap(find.text('map4/e1'));
    await tester.pump();
    expect(picked, ['map4/e1']);
  });

  testWidgets('Esc closes without jumping', (tester) async {
    final picked = <String>[];
    var closed = 0;
    await _pumpPicker(tester,
        onPick: (m) => picked.add(m.namePath), onClose: () => closed++);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    expect(closed, 1);
    expect(picked, isEmpty);
  });

  testWidgets('the all-networks switch re-queries and chips the rows',
      (tester) async {
    bool? reported;
    await _pumpPicker(tester, onAllNetworksChanged: (v) => reported = v);

    expect(find.text('e1_elsewhere'), findsNothing);
    // The network name is not shown while the search is single-network.
    expect(find.text('styling'), findsNothing);

    await tester.tap(find.byKey(const Key('find_node_all_networks')));
    await tester.pump();

    expect(reported, isTrue);
    expect(find.text('e1_elsewhere'), findsOneWidget);
    // Rows now carry their network as a trailing chip.
    expect(find.text('styling'), findsOneWidget);
    expect(find.text('main'), findsNWidgets(_activeNetworkNodes.length));
  });
}
