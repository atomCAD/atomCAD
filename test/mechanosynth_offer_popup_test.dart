/// The placement tool's offer popup (`doc/design_mechanosynth_editor.md`
/// Phase 4, §Placement tool).
///
/// The popup takes a canned applicability list and three callbacks, so its
/// rules are decidable without a viewport or a kernel — and they are rules
/// worth pinning, because each of them is a *refusal* or a *promise* the rest
/// of the design leans on:
///
/// - near-miss rows sit below the rule, carry no apply button, and explain
///   themselves instead (the kernel refuses the same thing independently, so a
///   regression here is a usability bug rather than a correctness one — which
///   is exactly why it needs a test of its own);
/// - **hover previews, click places** — and the hover preview is *armed*, not
///   fired: a pointer crossing the list must leave no evaluations behind it.
///   Opening the popup and filtering it must likewise spend none;
/// - moving the selection with the arrows previews exactly once per move,
///   because the ghost preview is what makes a library of environment variants
///   learnable;
/// - typing filters without re-querying, since the sweep was paid for on the
///   click.
library;

import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/mechanosynth_offer_popup.dart';

const _origin = APIVec3(x: 0, y: 0, z: 0);

APIGhostAtom _ghost(String kind) => APIGhostAtom(
    kind: kind, position: _origin, from: _origin, atomicNumber: 14);

APIMechanosynthCandidate _candidate(int index,
        {double residual = 0.0, bool mirrored = false}) =>
    APIMechanosynthCandidate(
      index: index,
      residual: residual,
      exact: residual == 0.0,
      mirrored: mirrored,
      approximate: false,
      ghost: [_ghost('added')],
    );

APIMechanosynthOffer _offer(
  String op, {
  bool fits = true,
  double residual = 0.0,
  int candidates = 1,
  bool exact = true,
  String note = '',
  String toolType = '',
  String toolState = '',
  String toolReason = '',
}) =>
    APIMechanosynthOffer(
      op: op,
      note: note,
      candidateCount: fits ? candidates : 0,
      bestResidual: residual,
      fits: fits,
      exact: exact,
      mirrored: false,
      approximate: false,
      ghost: [_ghost('added')],
      // A near miss holds its one rejected fit here too, so it previews like
      // anything else — it is simply never expanded into a choice.
      candidates: [
        for (var i = 0; i < (fits ? candidates : 1); i++)
          _candidate(i, residual: residual, mirrored: i.isOdd),
      ],
      // An empty `toolType` is the state every row carries with the `tools`
      // pin unwired — no annotation at all, which is the modelling use.
      toolType: toolType,
      toolState: toolState,
      toolReady: toolReason.isEmpty,
      toolReason: toolReason,
      // The kernel's own definition: it fits **and** its tool is ready.
      offerable: fits && toolReason.isEmpty,
    );

/// The kernel sorts applicable before near miss; the fixture arrives sorted,
/// as a real sweep does.
final _offers = <APIMechanosynthOffer>[
  _offer('si_donate_dimer', note: 'Si donation onto a reconstructed dimer'),
  _offer('cl_donate_dimer'),
  _offer('dimerize', candidates: 2),
  _offer('si_donate', fits: false, residual: 0.31),
  _offer('cl_donate', fits: false, residual: 0.44),
];

class _Harness {
  final chosen = <String>[];
  final previews = <String?>[];
  var cancels = 0;
}

/// Hovers a row and lets the preview delay elapse.
Future<void> _hover(WidgetTester tester, Key row,
    {Duration settle = const Duration(milliseconds: 200)}) async {
  final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
  await gesture.addPointer(location: Offset.zero);
  addTearDown(gesture.removePointer);
  await tester.pump();
  await gesture.moveTo(tester.getCenter(find.byKey(row)));
  await tester.pump();
  await tester.pump(settle);
}

Future<_Harness> _pump(
  WidgetTester tester, {
  List<APIMechanosynthOffer>? offers,
  double? Function(List<APIGhostAtom>)? arrowAngleFor,
  Duration previewDelay = const Duration(milliseconds: 100),
}) async {
  final harness = _Harness();
  await tester.pumpWidget(MaterialApp(
    home: Scaffold(
      body: Align(
        alignment: Alignment.topLeft,
        child: MechanosynthOfferPopup(
          anchorAtomicNumber: 14,
          offers: offers ?? _offers,
          arrowAngleFor: arrowAngleFor,
          previewDelay: previewDelay,
          onChoose: (op, index) => harness.chosen.add('$op#$index'),
          onPreview: (preview) => harness.previews.add(preview?.op),
          onCancel: () => harness.cancels++,
        ),
      ),
    ),
  ));
  await tester.pump();
  return harness;
}

/// The row keys in display order. Not "every `InkWell`": each applicable row
/// now carries an apply `IconButton`, which has an `InkWell` of its own.
List<String> _rowKeys(WidgetTester tester) => tester
    .widgetList<InkWell>(find.byType(InkWell))
    .map((w) => w.key)
    .whereType<ValueKey<String>>()
    .map((k) => k.value)
    .where((k) => k.startsWith('mechanosynth_popup_row_'))
    .toList();

String _header(WidgetTester tester) => tester
    .widget<Text>(find.byKey(const Key('mechanosynth_popup_header')))
    .data!;

Future<void> _key(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyEvent(key);
  await tester.pump();
}

void main() {
  testWidgets('the header counts the operations that fit, not the rows',
      (tester) async {
    await _pump(tester);
    expect(_header(tester), '3 operations apply to this Si');
  });

  testWidgets('an empty sweep is an answer, not an error', (tester) async {
    await _pump(tester, offers: const []);
    expect(_header(tester), 'Nothing applies to this Si');
    expect(_rowKeys(tester), isEmpty);
  });

  testWidgets('near-miss rows sit below a rule and are dimmed', (tester) async {
    await _pump(tester);
    // A rule separates the two sections; without near misses there is none.
    expect(find.byType(Divider), findsOneWidget);

    final rows = _rowKeys(tester);
    expect(rows.indexWhere((k) => k.contains('si_donate_dimer')),
        lessThan(rows.indexWhere((k) => k.endsWith('si_donate_0'))));
    // The near-miss badge is the residual, which is the information that makes
    // the row worth showing at all.
    expect(find.textContaining('0.31 Å off'), findsOneWidget);
  });

  testWidgets('the library note is behind an info icon, not on a second line',
      (tester) async {
    // At 300 px the note was always elided to a fragment, so it read as clutter
    // while saying nothing. It is whole, on demand, and the row is half as
    // tall.
    await _pump(tester);
    expect(find.textContaining('Si donation onto a reconstructed dimer'),
        findsNothing);
    final info = tester.widget<MechanosynthNoteTooltip>(
        find.byKey(const Key('mechanosynth_popup_info_si_donate_dimer_0')));
    expect(info.note, 'Si donation onto a reconstructed dimer');
  });

  testWidgets('a row with no note carries no info icon', (tester) async {
    await _pump(tester);
    expect(find.byKey(const Key('mechanosynth_popup_info_cl_donate_dimer_0')),
        findsNothing);
  });

  testWidgets('a near-miss row cannot be placed and explains itself instead',
      (tester) async {
    final harness = await _pump(tester);
    await tester
        .tap(find.byKey(const Key('mechanosynth_popup_row_si_donate_0')));
    await tester.pump();

    // A click on a near miss stays a *question*, because it cannot be placed:
    // it answers both halves at once, the amber preview on the workpiece and
    // the reason in place of the row.
    expect(harness.chosen, isEmpty);
    expect(harness.previews.last, 'si_donate');
    final reason = find.byKey(const Key('mechanosynth_popup_reason_si_donate'));
    expect(reason, findsOneWidget);
    expect(
        find.textContaining(
            'is not an environment si_donate was calculated for'),
        findsOneWidget);
  });

  testWidgets('a click places the row', (tester) async {
    final harness = await _pump(tester);
    await tester
        .tap(find.byKey(const Key('mechanosynth_popup_row_cl_donate_dimer_0')));
    await tester.pump();
    expect(harness.chosen, ['cl_donate_dimer#0']);
  });

  testWidgets('resting on a row previews it', (tester) async {
    final harness = await _pump(tester);
    await _hover(tester, const Key('mechanosynth_popup_row_cl_donate_dimer_0'));
    expect(harness.previews, ['cl_donate_dimer']);
    expect(harness.chosen, isEmpty, reason: 'hovering places nothing');
  });

  testWidgets('a pointer crossing a row leaves no evaluation behind it',
      (tester) async {
    // The rule the whole delay exists for: previewing is a synchronous
    // evaluation on the UI thread, so a pointer sweeping the list must not
    // spend one per row it passes.
    final harness =
        await _pump(tester, previewDelay: const Duration(milliseconds: 300));
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.addPointer(location: Offset.zero);
    addTearDown(gesture.removePointer);
    await tester.pump();

    for (final key in const [
      'mechanosynth_popup_row_si_donate_dimer_0',
      'mechanosynth_popup_row_cl_donate_dimer_0',
      'mechanosynth_popup_row_dimerize_1',
    ]) {
      await gesture.moveTo(tester.getCenter(find.byKey(Key(key))));
      await tester.pump(const Duration(milliseconds: 40));
    }
    expect(harness.previews, isEmpty,
        reason: 'none of the crossed rows was rested on long enough');

    // Coming to rest does preview, once.
    await tester.pump(const Duration(milliseconds: 400));
    expect(harness.previews, ['dimerize']);
  });

  testWidgets('opening the popup previews nothing', (tester) async {
    final harness = await _pump(tester);
    expect(harness.previews, isEmpty,
        reason: 'a preview costs an evaluation, so the list must not spend '
            'one before the user has asked for anything');
  });

  testWidgets('Up/Down move the selection and report each preview once',
      (tester) async {
    final harness = await _pump(tester);

    await _key(tester, LogicalKeyboardKey.arrowDown);
    await _key(tester, LogicalKeyboardKey.arrowDown);
    expect(harness.previews, ['si_donate_dimer', 'cl_donate_dimer']);

    await _key(tester, LogicalKeyboardKey.arrowUp);
    expect(harness.previews.last, 'si_donate_dimer');
    expect(harness.previews, hasLength(3));
  });

  testWidgets('Enter takes the selected row', (tester) async {
    final harness = await _pump(tester);
    await _key(tester, LogicalKeyboardKey.arrowDown);
    await _key(tester, LogicalKeyboardKey.arrowDown);
    await _key(tester, LogicalKeyboardKey.enter);
    expect(harness.chosen, ['cl_donate_dimer#0']);
  });

  testWidgets('Enter with nothing selected places nothing', (tester) async {
    final harness = await _pump(tester);
    await _key(tester, LogicalKeyboardKey.enter);
    expect(harness.chosen, isEmpty);
  });

  testWidgets('typing filters by prefix and does not re-query', (tester) async {
    final harness = await _pump(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyC);
    await tester.pump();

    expect(_header(tester), contains('"c"'));
    expect(find.byKey(const Key('mechanosynth_popup_row_cl_donate_dimer_0')),
        findsOneWidget);
    expect(find.byKey(const Key('mechanosynth_popup_row_si_donate_dimer_0')),
        findsNothing);
    // Filtering only hides rows. It must not pick one for the user: that would
    // spend an evaluation per keystroke.
    expect(harness.previews, [null]);

    await _key(tester, LogicalKeyboardKey.backspace);
    expect(find.byKey(const Key('mechanosynth_popup_row_si_donate_dimer_0')),
        findsOneWidget);
  });

  testWidgets('Escape reports a cancel', (tester) async {
    final harness = await _pump(tester);
    await _key(tester, LogicalKeyboardKey.escape);
    expect(harness.cancels, 1);
    expect(harness.chosen, isEmpty);
  });

  group('orientation variants', () {
    // An operation that fits more than one way is listed inline: a header line
    // naming it once, then one row per placement. The second popup it replaces
    // cost a click and hid the alternatives behind a name.
    final offers = <APIMechanosynthOffer>[
      _offer('si_donate_dimer'),
      _offer('dimerize', candidates: 2),
    ];

    testWidgets('each placement is its own row, under one header',
        (tester) async {
      await _pump(tester, offers: offers);

      // The header names the operation and carries no apply button: it does
      // not stand for any one of the placements beneath it.
      expect(find.text('dimerize'), findsOneWidget);
      expect(find.text('2 ways'), findsOneWidget);
      expect(find.byKey(const Key('mechanosynth_popup_row_dimerize_0')),
          findsOneWidget);
      expect(find.byKey(const Key('mechanosynth_popup_row_dimerize_1')),
          findsOneWidget);
      // …and `mirrored` is on the badge, which is the only thing telling two
      // placements of one operation apart when their arrows agree.
      expect(find.textContaining('mirrored'), findsOneWidget);
    });

    testWidgets('a header is not selectable and previews nothing',
        (tester) async {
      final harness = await _pump(tester, offers: offers);
      final rows = _rowKeys(tester);
      // The header is not an InkWell row at all, so it cannot be clicked.
      expect(rows.where((k) => k.endsWith('dimerize_0')), hasLength(1));

      // Arrowing down walks straight past it onto the first placement.
      await _key(tester, LogicalKeyboardKey.arrowDown);
      await _key(tester, LogicalKeyboardKey.arrowDown);
      expect(harness.previews, ['si_donate_dimer', 'dimerize']);
      await _key(tester, LogicalKeyboardKey.enter);
      expect(harness.chosen, ['dimerize#0']);
    });

    testWidgets('clicking a variant places its own candidate index',
        (tester) async {
      final harness = await _pump(tester, offers: offers);
      await tester
          .tap(find.byKey(const Key('mechanosynth_popup_row_dimerize_1')));
      await tester.pump();
      expect(harness.chosen, ['dimerize#1']);
    });

    testWidgets('a single-placement operation is a plain row, not a group',
        (tester) async {
      await _pump(tester, offers: offers);
      expect(find.text('1 ways'), findsNothing);
      expect(find.byKey(const Key('mechanosynth_popup_group_si_donate_dimer')),
          findsNothing);
      expect(find.byKey(const Key('mechanosynth_popup_row_si_donate_dimer_0')),
          findsOneWidget);
    });

    testWidgets('the arrow is rotated to the angle the host reports',
        (tester) async {
      await _pump(tester,
          offers: offers, arrowAngleFor: (ghost) => math.pi / 2);
      final rotated = tester.widgetList<Transform>(find.descendant(
          of: find.byKey(const Key('mechanosynth_popup_row_dimerize_0')),
          matching: find.byType(Transform)));
      expect(rotated, isNotEmpty,
          reason: 'a variant row carries a direction arrow');
    });

    testWidgets('a placement with no direction falls back to a dot',
        (tester) async {
      // A bond-only operation has no ghost atoms to take a centroid of, so the
      // host reports no angle; the ordinal beside it still tells the rows
      // apart.
      await _pump(tester, offers: offers, arrowAngleFor: (ghost) => null);
      expect(
          find.descendant(
              of: find.byKey(const Key('mechanosynth_popup_row_dimerize_0')),
              matching: find.byIcon(Icons.circle)),
          findsOneWidget);
    });
  });

  // ==========================================================================
  // Tool-blocked rows (doc/design_mechanosynth_tools.md)
  // ==========================================================================
  //
  // A `tip` operation needs its instrument bound, in the right state, and
  // matching at its pose. A row that fits the host perfectly but whose tip is
  // spent **cannot be committed**, so it belongs with the near misses rather
  // than among the offers — and the thing it has to say is not a residual but
  // a reason. These are the rules a regression would turn into "the popup
  // offered me a step the kernel then refused".
  group('tool-blocked rows', () {
    final offers = <APIMechanosynthOffer>[
      _offer('hdump', toolType: 'habst_tool', toolState: 'spent'),
      _offer('expose'),
      _offer('habst',
          toolType: 'habst_tool',
          toolState: 'spent',
          toolReason: 'habst_tool is spent'),
      _offer('habst_probe',
          toolType: 'probe',
          toolReason: 'no molecule tagged `probe` on the tools pin'),
      _offer('si_donate', fits: false, residual: 0.31),
    ];

    testWidgets('a blocked row sits below the rule with the near misses',
        (tester) async {
      await _pump(tester, offers: offers);
      final rows = _rowKeys(tester);
      expect(rows.indexWhere((k) => k.endsWith('hdump_0')),
          lessThan(rows.indexWhere((k) => k.endsWith('habst_0'))));
      expect(rows.indexWhere((k) => k.endsWith('expose_0')),
          lessThan(rows.indexWhere((k) => k.endsWith('habst_probe_0'))));
      expect(find.byType(Divider), findsOneWidget);
    });

    testWidgets('the header counts what can be placed, not what fits',
        (tester) async {
      // Four of the five rows fit; two of them are blocked by their tool.
      await _pump(tester, offers: offers);
      expect(_header(tester), '2 operations apply to this Si');
    });

    testWidgets('the reason takes the badge slot, where the residual would be',
        (tester) async {
      await _pump(tester, offers: offers);
      expect(find.text('habst_tool is spent'), findsOneWidget);
      expect(find.text('no molecule tagged `probe` on the tools pin'),
          findsOneWidget);
    });

    testWidgets('clicking a blocked row explains instead of placing',
        (tester) async {
      final harness = await _pump(tester, offers: offers);
      await tester.tap(find.byKey(const Key('mechanosynth_popup_row_habst_0')));
      await tester.pump();
      expect(harness.chosen, isEmpty);
      // The refusal names the fix, and the fix for a spent tool is a *step*,
      // not an edit to the library.
      expect(find.byKey(const Key('mechanosynth_popup_reason_habst')),
          findsOneWidget);
      expect(find.textContaining('recharge'), findsOneWidget);
    });

    testWidgets('a near miss keeps its own refusal, which names the library',
        (tester) async {
      await _pump(tester, offers: offers);
      await tester
          .tap(find.byKey(const Key('mechanosynth_popup_row_si_donate_0')));
      await tester.pump();
      expect(find.textContaining("loosen the library's tolerance"),
          findsOneWidget);
    });

    testWidgets('a blocked row still previews, in the warning colour',
        (tester) async {
      // Seeing the ghost is half the answer to "why not here?", and a blocked
      // row's placement is a real fit — it is simply not commitable.
      final harness = await _pump(tester, offers: offers);
      await tester.tap(find.byKey(const Key('mechanosynth_popup_row_habst_0')));
      await tester.pump();
      expect(harness.previews, ['habst']);
    });

    testWidgets('a ready tool is an annotation, not a chip', (tester) async {
      // The panel's *Tools* readout is what says a recharge is due; a ready
      // instrument on a row is reference, and the row has no width to spare.
      await _pump(tester, offers: offers);
      final info = tester.widget<MechanosynthNoteTooltip>(
          find.byKey(const Key('mechanosynth_popup_info_hdump_0')));
      expect(info.note, 'habst_tool · spent');
    });

    testWidgets('a bulk row carries no tool annotation at all', (tester) async {
      await _pump(tester, offers: offers);
      expect(find.byKey(const Key('mechanosynth_popup_info_expose_0')),
          findsNothing);
    });

    testWidgets('with tools unwired the rows are what they always were',
        (tester) async {
      await _pump(tester);
      expect(_header(tester), '3 operations apply to this Si');
      expect(find.byKey(const Key('mechanosynth_popup_info_cl_donate_dimer_0')),
          findsNothing);
    });
  });
}
