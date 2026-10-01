/// The document tabs' gesture model and the quit dialog's list
/// (`doc/design_multiple_documents.md` D10, Phase 3).
///
/// [DocumentTabs] is given plain `APIDocumentTab` data classes and callbacks,
/// so its rules — the reorder arithmetic and the pointer gate — run here
/// without the Rust library.
library;

import 'package:flutter/gestures.dart' show kMiddleMouseButton;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/document_tabs.dart';

APIDocumentTab _tab(int id,
        {bool active = false, bool dirty = false, bool saved = true}) =>
    APIDocumentTab(
      id: BigInt.from(id),
      displayName: saved ? 'doc$id.cnnd' : 'Untitled',
      filePath: saved ? 'C:/designs/doc$id.cnnd' : null,
      isDirty: dirty,
      isActive: active,
    );

class _Recorder {
  final activated = <BigInt>[];
  final closed = <BigInt>[];
  final moved = <(BigInt, int)>[];
  bool busy = false;

  DocumentTabs gestures(List<APIDocumentTab> tabs) => DocumentTabs(
        tabs: tabs,
        onActivate: (t) => activated.add(t.id),
        onClose: (t) => closed.add(t.id),
        onMove: (id, index) => moved.add((id, index)),
        pointerBusy: () => busy,
      );
}

void main() {
  final tabs = [_tab(1, active: true), _tab(2), _tab(3)];

  group('reorder (ReorderableListView indices)', () {
    test('dragging forward counts the index after removal', () {
      final r = _Recorder();
      r.gestures(tabs).reorder(0, 2); // dropped between tab 2 and tab 3
      expect(r.moved, [(BigInt.one, 1)]);
    });

    test('dragging past the end puts the tab last', () {
      final r = _Recorder();
      r.gestures(tabs).reorder(0, 3);
      expect(r.moved, [(BigInt.one, 2)]);
    });

    test('dragging backward', () {
      final r = _Recorder();
      r.gestures(tabs).reorder(2, 0);
      expect(r.moved, [(BigInt.from(3), 0)]);
    });

    test('dropping a tab onto its own place moves nothing', () {
      final r = _Recorder();
      final g = r.gestures(tabs);
      g.reorder(1, 1);
      g.reorder(1, 2); // just after itself = where it is
      expect(r.moved, isEmpty);
    });

    test('reordering is not gated by the pointer', () {
      final r = _Recorder()..busy = true;
      r.gestures(tabs).reorder(0, 3);
      expect(r.moved, hasLength(1));
    });
  });

  group('activate and close', () {
    test('a click on a parked tab activates it; on the active tab, nothing',
        () {
      final r = _Recorder();
      final g = r.gestures(tabs);
      g.pointerDown(tabs[1]);
      g.tap(tabs[1]);
      g.pointerDown(tabs[0]);
      g.tap(tabs[0]);
      expect(r.activated, [BigInt.two]);
    });

    test('the close button and a middle click both close', () {
      final r = _Recorder();
      final g = r.gestures(tabs);
      g.pointerDown(tabs[2]);
      g.close(tabs[2]);
      g.pointerDown(tabs[1], button: kMiddleMouseButton);
      expect(r.closed, [BigInt.from(3), BigInt.two]);
      // The middle click consumed the gesture: no activation follows it.
      g.tap(tabs[1]);
      expect(r.activated, isEmpty);
    });

    test('every gesture is disabled while another pointer is down', () {
      final r = _Recorder()..busy = true;
      final g = r.gestures(tabs);
      g.pointerDown(tabs[1]);
      g.tap(tabs[1]);
      g.pointerDown(tabs[2]);
      g.close(tabs[2]);
      g.pointerDown(tabs[2], button: kMiddleMouseButton);
      expect(r.activated, isEmpty);
      expect(r.closed, isEmpty);
    });

    test('the gate is taken at pointer down, not at the tap', () {
      // By the time a tap is recognised the tab's own pointer counts as down;
      // what matters is whether something else was down when it started.
      final r = _Recorder();
      final g = r.gestures(tabs);
      g.pointerDown(tabs[1]);
      r.busy = true;
      g.tap(tabs[1]);
      expect(r.activated, [BigInt.two]);
    });

    test('a tap without a pointer down does nothing', () {
      final r = _Recorder();
      r.gestures(tabs).tap(tabs[1]);
      expect(r.activated, isEmpty);
    });
  });

  test('labels and tooltips', () {
    expect(documentTabLabel(_tab(1, dirty: true)), 'doc1.cnnd*');
    expect(documentTabLabel(_tab(1)), 'doc1.cnnd');
    expect(documentTabTooltip(_tab(1)), 'C:/designs/doc1.cnnd');
    expect(
        documentTabTooltip(_tab(1, saved: false)), 'Untitled (not saved yet)');
  });

  test('the quit dialog lists exactly the dirty documents', () {
    expect(
      dirtyDocumentNames([
        _tab(1, dirty: true),
        _tab(2),
        _tab(3, dirty: true, saved: false),
        _tab(4, active: true, dirty: true),
      ]),
      ['doc1.cnnd', 'Untitled', 'doc4.cnnd'],
    );
    expect(dirtyDocumentNames([_tab(1), _tab(2)]), isEmpty);
  });

  testWidgets('both layouts render the tabs and route a click', (tester) async {
    final r = _Recorder();
    for (final vertical in [true, false]) {
      r.activated.clear();
      final g = r.gestures([_tab(1, active: true), _tab(2, dirty: true)]);
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 600,
            height: 400,
            child: vertical
                ? DocumentTabList(gestures: g)
                : DocumentTabStrip(gestures: g),
          ),
        ),
      ));
      expect(find.text('doc1.cnnd'), findsOneWidget);
      expect(find.text('doc2.cnnd*'), findsOneWidget);
      await tester.tap(find.text('doc2.cnnd*'));
      await tester.pump();
      expect(r.activated, [BigInt.two], reason: 'vertical: $vertical');
    }
  });
}
