/// The shared mechanosynthesis scrubber and chapter list
/// (`doc/design_mechanosynth_editor.md` Phase 4).
///
/// The widget takes plain numbers and callbacks rather than reaching for the
/// model, so this needs no kernel. What is worth pinning here is exactly what
/// the extraction risks breaking: that **both** hosts can build it — the
/// replayer from `APIMechanosynthInfo`, the editor from
/// `APIMechanosynthEditData`, and the editor's travel is the authored block
/// alone — that an edit is reported once through the callback, and that a
/// chapter row jumps to the step *before* its first, which is the one rule in
/// here a reader would otherwise assume off by one.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_scrubber.dart';

APIMechanosynthChapter _chapter(String phase, int first, int last,
        {int layer = -1}) =>
    APIMechanosynthChapter(
        phase: phase, layer: layer, firstStep: first, lastStep: last);

final _chapters = [
  _chapter('etch', 1, 4, layer: 0),
  _chapter('grow', 5, 9, layer: 1),
];

APIMechanosynthInfo _info({int count = 9, int applied = 3}) =>
    APIMechanosynthInfo(
      count: count,
      applied: applied,
      currentOp: 'habst',
      currentNote: '',
      currentMethod: 'probe',
      currentPhase: 'etch',
      currentLayer: 0,
      currentSite: -1,
      chapters: _chapters,
    );

APIAuthoredStep _step(String op) => APIAuthoredStep(
      op: op,
      t: const APIVec3(x: 0, y: 0, z: 0),
      note: '',
      method: 'probe',
      phase: 'etch',
      layer: 0,
      site: -1,
      residual: 0.0,
      exact: true,
      approximate: false,
    );

APIMechanosynthEditData _editData({int authored = 3, int applied = 2}) =>
    APIMechanosynthEditData(
      // A wired prefix the cursor must *not* include in its travel.
      prefixCount: 142,
      authored: [for (var i = 0; i < authored; i++) _step('op$i')],
      cursor: applied,
      applied: applied,
      opNames: const ['habst', 'dimerize'],
      inexactCount: 0,
      approximateCount: 0,
      toolState: 'idle',
      chapters: _chapters,
    );

Future<void> _pump(WidgetTester tester, Widget child) async {
  await tester.pumpWidget(MaterialApp(
    home: Scaffold(
      body: SizedBox(width: 500, child: child),
    ),
  ));
  await tester.pump();
}

Slider _slider(WidgetTester tester) =>
    tester.widget<Slider>(find.byKey(const Key('mechanosynth_step_slider')));

void main() {
  group('scrubber', () {
    testWidgets('builds from the replayer info', (tester) async {
      await _pump(
        tester,
        MechanosynthScrubber.fromInfo(_info(), onChanged: (_) {}),
      );
      final slider = _slider(tester);
      expect(slider.max, 9.0);
      expect(slider.value, 3.0);
      expect(find.text('Step'), findsOneWidget);
    });

    testWidgets('builds from the editor data, over the authored block only',
        (tester) async {
      await _pump(
        tester,
        MechanosynthScrubber.fromEditData(_editData(), onChanged: (_) {}),
      );
      final slider = _slider(tester);
      // Three authored steps — the 142-step wired prefix is not part of the
      // travel, because the cursor can only sit inside the authored block.
      expect(slider.max, 3.0);
      expect(slider.value, 2.0);
      expect(find.text('Cursor'), findsOneWidget);
    });

    testWidgets('an empty block disables the slider', (tester) async {
      await _pump(
        tester,
        MechanosynthScrubber.fromEditData(_editData(authored: 0, applied: 0),
            onChanged: (_) {}),
      );
      expect(_slider(tester).onChanged, isNull);
    });

    testWidgets('a wired step pin disables the slider', (tester) async {
      await _pump(
        tester,
        MechanosynthScrubber.fromInfo(_info(),
            enabled: false, onChanged: (_) {}),
      );
      expect(_slider(tester).onChanged, isNull);
    });

    testWidgets('a drag reports once, on release, and brackets itself',
        (tester) async {
      final changes = <int>[];
      final previews = <int?>[];
      var started = 0;
      var ended = 0;
      await _pump(
        tester,
        MechanosynthScrubber.fromInfo(
          _info(),
          onChanged: changes.add,
          onDragStart: () => started++,
          onDragEnd: () => ended++,
          onPreview: previews.add,
        ),
      );

      final slider = find.byKey(const Key('mechanosynth_step_slider'));
      final centre = tester.getCenter(slider);
      final gesture = await tester.startGesture(centre);
      await tester.pump();
      await gesture.moveBy(const Offset(60, 0));
      await tester.pump();
      // Nothing is written mid-drag: a write is a full replay plus a
      // tessellation, on the UI thread.
      expect(changes, isEmpty);
      expect(previews, isNotEmpty);

      await gesture.up();
      await tester.pump();
      expect(changes, hasLength(1));
      expect(changes.single, greaterThan(4));
      expect(started, 1);
      expect(ended, 1);
      // The release clears the preview, so the host stops suppressing its
      // readout.
      expect(previews.last, isNull);
    });
  });

  group('chapter list', () {
    testWidgets('a row jumps to the step before its first', (tester) async {
      final jumps = <int>[];
      await _pump(
        tester,
        MechanosynthChapterList(
          chapters: _chapters,
          applied: 0,
          onJump: jumps.add,
        ),
      );
      await tester.tap(find.textContaining('grow'));
      await tester.pump();
      // "grow" covers steps 5..9, so its row lands on 4: the next tick applies
      // the phase's first reaction rather than skipping past it.
      expect(jumps, [4]);
    });

    testWidgets('a single phase renders nothing', (tester) async {
      await _pump(
        tester,
        MechanosynthChapterList(
          chapters: [_chapter('etch', 1, 9)],
          applied: 0,
          onJump: (_) {},
        ),
      );
      expect(find.textContaining('etch'), findsNothing);
      expect(find.text('Phases'), findsNothing);
    });

    testWidgets('a wired step pin makes the rows inert', (tester) async {
      final jumps = <int>[];
      await _pump(
        tester,
        MechanosynthChapterList(
          chapters: _chapters,
          applied: 0,
          enabled: false,
          onJump: jumps.add,
        ),
      );
      await tester.tap(find.textContaining('grow'));
      await tester.pump();
      expect(jumps, isEmpty);
    });
  });
}
