/// The `mechanosynth_edit` placement tool's popup: what the wired operation
/// library can do at the atom the user clicked, and — once an operation is
/// chosen — which way to place it.
///
/// It is a **viewport overlay anchored to the clicked atom**, not a section of
/// the property panel: the answer appears where the question was asked, which
/// is the whole point of the atom-first inversion, and a 300 px panel on the
/// far right would put eye and mouse travel on every placement. The viewport
/// owns the anchoring and the clamping (`structure_designer_viewport.dart`);
/// this file owns the list, its keyboard model and its refusals, and takes
/// everything it shows as plain data so it is decidable without a kernel.
/// See `doc/design_mechanosynth_editor.md` §Placement tool.
///
/// Three rules are load-bearing rather than decorative:
///
/// - **Blocked rows cannot be placed.** They are the `offerable == false`
///   entries — a **near miss** (`fits == false`, shown with its residual) or a
///   row whose **tool is not ready** (`fits == true`, shown with the kernel's
///   reason where the residual would be: *habst_tool is spent*, *no molecule
///   tagged `probe` on the tools pin*). Both sit
///   below a rule, dimmed, and with no apply button. They *are* selectable —
///   seeing the amber ghost is half the answer to "why not here?" — but a click
///   on one also replaces the row with the reason and inserts nothing. There is
///   deliberately no cast past the gate: an inexact step *inside* the gate
///   commits with a residual chip,
///   but outside it the library is stating it has not computed this situation,
///   and the honest fixes are the two the message names. A tool-blocked row's
///   fix is different in kind — it is a *step*, the recharge, not an edit to
///   the library — so the two refusals say different things. The kernel refuses
///   both independently (`mechanosynth_edit_choose`), so this is the
///   explanation, not the enforcement.
/// - **Hovering a row previews it, after a delay; clicking one places it.**
///   The preview is a real object in the scene — decorator ghosts, tessellated
///   with the workpiece — so showing it costs a synchronous evaluation on the
///   UI thread. The delay is what makes hover affordable: a pointer crossing
///   the list never previews the rows it passed over, because each move
///   cancels the pending timer, and the blocked frame cannot start a second
///   evaluation while the first is running. [previewDelay] is the host's to
///   choose, and the host sizes it from what the last refresh actually cost.
/// - **Typing filters, it does not re-query.** The sweep is one `place()` per
///   library operation and was paid for on the click; the filter only hides
///   rows.
/// - **Muting hides a row in place, and says so in the footer.** The eye-off
///   button leaves an operation out of this node's offer sweep
///   (`doc/design_mechanosynth_op_muting.md`); like the filter it costs no
///   re-query, because the rows in hand were fitted against a workpiece muting
///   does not touch — and the kernel leaves its stored offer list alone for
///   the same reason, so the remaining rows stay placeable. What muting may
///   **never** do is go quiet: an empty list is read as a statement about the
///   library's coverage, so the footer reports *4 of 19 operations muted*
///   whenever anything is, and *show all here* sweeps the whole library for
///   this one anchor. A row that sweep brings back is badged by its lit
///   eye-off and is placeable like any other — mute filters the sweep and
///   nothing else.
library;

import 'dart:async';

import 'package:flutter/foundation.dart' show setEquals;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/element_symbol_input.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// About 300 px wide, eight rows before it scrolls — the two numbers the
/// viewport needs to clamp the popup into view.
const double MECHANOSYNTH_POPUP_WIDTH = 300.0;
const double MECHANOSYNTH_POPUP_MAX_LIST_HEIGHT = 260.0;

/// Right padding on every row, so the desktop scroll bar — which Flutter's
/// default `ScrollBehavior` overlays on the content rather than reserving space
/// for — does not sit on top of the apply buttons.
const double MECHANOSYNTH_POPUP_SCROLLBAR_GUTTER = 12.0;

/// The apply button's green, matching `MS_GHOST_ADDED` in the tessellator.
const Color MECHANOSYNTH_APPLY_GREEN = Color(0xFF4CB94C);

/// A tooltip that **wraps** instead of spanning the window.
///
/// Flutter's `Tooltip` puts no upper bound on its width: a one-paragraph
/// operation note lays itself out as a single line across the whole screen,
/// which is unreadable and covers the thing it is describing. Constraining it
/// needs `richMessage` + a `WidgetSpan`, since `message` has no width knob —
/// and `Tooltip` **asserts** that `textStyle` is null when `richMessage` is
/// given, so the styling has to live on the `Text` inside.
class MechanosynthNoteTooltip extends StatelessWidget {
  final String note;
  final Widget child;

  static const double maxWidth = 320.0;

  const MechanosynthNoteTooltip({
    super.key,
    required this.note,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Tooltip(
      waitDuration: const Duration(milliseconds: 250),
      richMessage: WidgetSpan(
        alignment: PlaceholderAlignment.baseline,
        baseline: TextBaseline.alphabetic,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: maxWidth),
          child: Text(
            note,
            style: TextStyle(fontSize: 12, color: scheme.onInverseSurface),
          ),
        ),
      ),
      child: child,
    );
  }
}

/// What the selected row would do, handed back so the host can ghost it.
/// A blocked row's preview is drawn in a warning colour; it is for showing,
/// never for placing.
class MechanosynthPreview {
  final String op;
  final int candidateIndex;

  /// The row cannot be committed — an over-gate fit, or a fit whose tool is not
  /// ready. Named for the first case, which was the only one when it was
  /// written; the colour means "showing, not placing" in both.
  final bool nearMiss;
  final List<APIGhostAtom> ghost;

  const MechanosynthPreview({
    required this.op,
    required this.candidateIndex,
    required this.nearMiss,
    required this.ghost,
  });
}

/// What a row *is*. An operation that can be placed more than one way expands
/// into a [group] header naming it once, followed by one [variant] per
/// orientation — so choosing an orientation is a click in this list rather than
/// a click into a second one.
enum _RowKind {
  /// One operation, one way to place it. Name, badge and apply button.
  single,

  /// The name of an operation whose placements follow. Not selectable and not
  /// placeable: it is a label, and choosing for the user which orientation it
  /// stands for is the thing the expansion exists to stop doing.
  group,

  /// One placement of the operation named by the header above it.
  variant,
}

class _Row {
  final _RowKind kind;
  final String op;

  /// The row's index in the *operation's own* candidate list, which is what
  /// `mechanosynth_edit_choose` takes.
  final int candidateIndex;
  final String title;
  final String subtitle;
  final String badge;
  final bool fits;

  /// Whether the row can be **committed**: it fits geometrically *and*, when
  /// its operation needs an instrument, that instrument is bound, in the right
  /// state, and matching at its pose.
  ///
  /// This, not [fits], is what puts a row above or below the rule. A row that
  /// fits a host perfectly but whose tip is spent cannot be placed either, and
  /// listing it among the offers would mean listing something the kernel is
  /// about to refuse.
  final bool offerable;

  /// The operation's instrument, when tools are wired and the operation is
  /// `tip`. Empty means the row carries **no** tool annotation at all — the
  /// state every row is in with the `tools` pin unwired, which is the
  /// modelling use.
  final String toolType;
  final String toolState;

  /// Why the tool is not ready, in the words the row shows where the residual
  /// would be. Empty on a ready row.
  final String toolReason;

  final double residual;
  final List<APIGhostAtom> ghost;

  /// `1`-based position among its operation's placements, and how many there
  /// are. The ordinal is the discriminator of last resort: two placements
  /// symmetric about the view axis project to the *same* arrow, so the arrow
  /// can never be the only thing telling them apart.
  final int ordinal;
  final int ordinalCount;

  /// The node mutes this operation, so only a *show all here* sweep produced
  /// the row.
  ///
  /// It is **badged, never blocked**: mute filters the sweep, and a row that is
  /// in the list is placeable whatever put it there. The badge is the lit
  /// eye-off button, which doubles as the way back — one control, two states.
  final bool muted;

  const _Row({
    required this.kind,
    required this.op,
    required this.candidateIndex,
    required this.title,
    required this.subtitle,
    required this.badge,
    required this.fits,
    required this.offerable,
    required this.residual,
    required this.ghost,
    this.toolType = '',
    this.toolState = '',
    this.toolReason = '',
    this.ordinal = 0,
    this.ordinalCount = 0,
    this.muted = false,
  });

  /// Whether this row can be selected (and so previewed) or placed. A group
  /// header can do neither.
  bool get isSelectable => kind != _RowKind.group;

  /// Whether the row sits below the rule: it cannot be committed, either
  /// because it does not fit or because its tool is not ready.
  bool get isBlocked => !offerable;

  /// `habst_tool · charged`, for the annotation on a ready row. Empty when the
  /// row carries no tool at all.
  String get toolSummary {
    if (toolType.isEmpty) return '';
    return toolState.isEmpty ? toolType : '$toolType · $toolState';
  }
}

class MechanosynthOfferPopup extends StatefulWidget {
  /// The clicked atom's atomic number — the header says "3 operations apply to
  /// this Si".
  final int anchorAtomicNumber;

  /// The offer rows, applicable and near-miss mixed; the kernel has already
  /// sorted them applicable-first. Ignored in candidate mode.
  final List<APIMechanosynthOffer> offers;

  /// Takes a row: the operation and the candidate index within it.
  final void Function(String op, int candidateIndex) onChoose;

  /// The **selected** row, on every change of selection — a click, an arrow
  /// key, or the selection being dropped. `null` means "no preview": nothing
  /// selected, an empty list, or a filter that matched nothing.
  ///
  /// Never called on hover. Each call costs the host an evaluation.
  final ValueChanged<MechanosynthPreview?> onPreview;

  /// Escape, or a click outside.
  final VoidCallback onCancel;

  /// Leave an operation out of this node's offer sweep, or put it back.
  ///
  /// The popup is where the clutter is noticed, so it is the cheapest place to
  /// act on it. The host writes the node data; **this widget does not re-query**
  /// — the remaining rows were computed against the same workpiece a moment
  /// ago, so they are still correct, and a second sweep would be paid for
  /// nothing.
  final void Function(String op, bool muted)? onMute;

  /// Which operations are currently muted, as the **host** understands it.
  ///
  /// One source rather than two: the kernel's sweep says which rows it brought
  /// back muted, and a mute taken from this popup changes the answer without a
  /// second sweep, so the host folds both together and this widget reads the
  /// result. A row is drawn as muted exactly when its operation is in here.
  final Set<String> mutedOps;

  /// How many of the wired library's operations the sweep did not look at,
  /// and how many the library has. `0` muted means the footer says nothing.
  final int mutedCount;
  final int libraryCount;

  /// Sweep this anchor again over the **whole** library.
  ///
  /// The escape hatch that keeps an empty list honest: the list's promise is
  /// that it reports what the library can do here, and muting would quietly
  /// turn that into a lie without a way to ask the full question.
  final VoidCallback? onShowAll;

  /// Whether [onShowAll] has already been taken for this anchor. Host state,
  /// because the answering sweep reports nothing skipped and the footer would
  /// otherwise vanish at the moment it has something to say.
  final bool showingAll;

  /// A drag on the header, in viewport pixels. The viewport accumulates these
  /// into an offset from the automatic position: the popup is placed clear of
  /// the reaction to begin with, but "clear" is a guess about a 3D scene the
  /// user can see and this widget cannot, so the last word is theirs.
  final ValueChanged<Offset>? onDrag;

  /// Puts the popup back where the automatic placement wants it. Shown on the
  /// header only once [onDrag] has actually moved it.
  final VoidCallback? onResetPosition;

  /// Whether the popup currently sits somewhere the user dragged it.
  final bool moved;

  /// How long the pointer must rest on a row before it previews.
  ///
  /// The host measures the last refresh and sizes this from it, so a small
  /// molecule feels immediate and a million-atom slab backs off instead of
  /// stuttering. Zero is legal and is what the tests use.
  final Duration previewDelay;

  /// The screen-space direction a placement goes in, as an angle in radians
  /// (`0` = right, increasing clockwise, since screen `y` grows downward), or
  /// `null` when it cannot be worked out.
  ///
  /// Supplied by the host because it needs the live camera: the arrow points
  /// the way the reaction goes *as the user is currently looking at it*, and
  /// re-aims as they orbit. A bond-only operation has no ghost atoms and so no
  /// direction — such operations have one placement anyway, so no arrow is
  /// needed to tell two apart.
  final double? Function(List<APIGhostAtom> ghost)? arrowAngleFor;

  const MechanosynthOfferPopup({
    super.key,
    required this.anchorAtomicNumber,
    required this.offers,
    required this.onChoose,
    required this.onPreview,
    required this.onCancel,
    this.onDrag,
    this.onResetPosition,
    this.moved = false,
    this.arrowAngleFor,
    this.previewDelay = const Duration(milliseconds: 140),
    this.onMute,
    this.onShowAll,
    this.mutedOps = const {},
    this.mutedCount = 0,
    this.libraryCount = 0,
    this.showingAll = false,
  });

  @override
  State<MechanosynthOfferPopup> createState() => _MechanosynthOfferPopupState();
}

class _MechanosynthOfferPopupState extends State<MechanosynthOfferPopup> {
  final FocusNode _focusNode = FocusNode();

  /// Index into [_rows] — the *filtered* list, so the selection is always on
  /// something visible. `-1` is "nothing selected", which is where a new list
  /// starts: selecting costs an evaluation, so the list must not spend one
  /// before the user has asked for anything.
  int _highlight = -1;

  /// The typed prefix. Filters by operation name; never re-queries.
  String _filter = '';

  /// The near-miss row whose reason is currently shown in place of its normal
  /// content, if any.
  String? _refused;

  /// What was last reported through `onPreview`, so a rebuild that leaves the
  /// selection where it was does not report it again — each report is an
  /// evaluation, so exactly one per move is not a nicety.
  String? _reportedKey;

  /// The pending hover preview. Cancelled by any further pointer movement, so
  /// a pointer crossing the list previews only where it comes to rest.
  Timer? _hoverTimer;

  /// The row the pointer is over, which is where the mute button appears.
  ///
  /// Separate from [_highlight] — that one means *previewed*, and costs an
  /// evaluation — and **it costs nothing**: showing a button is a repaint, so
  /// it needs no delay and arms no timer. `-1` is none.
  int _hovered = -1;

  @override
  void initState() {
    super.initState();
    _focusNode.requestFocus();
  }

  @override
  void dispose() {
    _hoverTimer?.cancel();
    _focusNode.dispose();
    super.dispose();
  }

  /// Arms a preview of [index], replacing any pending one.
  ///
  /// The row is *not* previewed now: that is the whole point. A pointer moving
  /// through the list re-arms this on every row it crosses and fires on none
  /// of them.
  void _hoverRow(int index) {
    if (index == _highlight) return;
    _hoverTimer?.cancel();
    if (widget.previewDelay == Duration.zero) {
      _setHighlight(index);
      return;
    }
    _hoverTimer = Timer(widget.previewDelay, () {
      if (mounted) _setHighlight(index);
    });
  }

  void _cancelPendingHover() {
    _hoverTimer?.cancel();
    _hoverTimer = null;
  }

  @override
  void didUpdateWidget(MechanosynthOfferPopup oldWidget) {
    super.didUpdateWidget(oldWidget);
    // A new sweep starts with nothing selected.
    if (!identical(oldWidget.offers, widget.offers)) {
      _highlight = -1;
      _reportedKey = null;
      _filter = '';
      _refused = null;
      _hovered = -1;
    }
    // Muting a row removes it from the list, so every index below it shifts.
    // `_highlight` is an index into the *filtered* rows, so leaving it would
    // silently move the selection — and Enter would then place a row the user
    // never chose.
    if (!setEquals(oldWidget.mutedOps, widget.mutedOps)) {
      _highlight = -1;
      _reportedKey = null;
      _hovered = -1;
    }
  }

  /// Every row the popup could show, before filtering.
  /// Every row, before filtering.
  ///
  /// An operation with more than one placement becomes a **group**: its name on
  /// a header line, then one **variant** row per orientation. A row that cannot
  /// be committed is never expanded — a near miss's `candidates` hold the
  /// single rejected fit, and a tool-blocked row's placements are all blocked
  /// by the same tool, so offering a choice between them would be nonsense in
  /// both cases.
  List<_Row> get _allRows {
    final rows = <_Row>[];
    for (final offer in widget.offers) {
      // **Muting hides the row in place.** The kernel's stored sweep keeps it,
      // which is harmless — nothing can choose a row that is not drawn — and
      // that is what lets a mute from this popup cost no second sweep. A
      // *show all here* sweep is the exception: there the muted rows are the
      // answer, so they are shown and badged.
      if (_isMuted(offer) && !widget.showingAll) continue;
      final expand = offer.offerable && offer.candidates.length > 1;
      if (!expand) {
        rows.add(_Row(
          kind: _RowKind.single,
          op: offer.op,
          candidateIndex: 0,
          title: offer.op,
          subtitle: offer.note,
          // **The reason takes the badge slot.** A tool-blocked row fits, so
          // its residual says nothing a user needs; what they need is *habst_tool
          // is spent*, and that is the one place on the row wide enough for it.
          badge: !offer.fits
              ? '${formatNatural(offer.bestResidual, 2)} Å off'
              : offer.offerable
                  ? _badge(
                      exact: offer.exact,
                      approximate: offer.approximate,
                      residual: offer.bestResidual,
                    )
                  : offer.toolReason,
          fits: offer.fits,
          offerable: offer.offerable,
          toolType: offer.toolType,
          toolState: offer.toolState,
          toolReason: offer.toolReason,
          residual: offer.bestResidual,
          ghost: offer.ghost,
          muted: _isMuted(offer),
        ));
        continue;
      }

      rows.add(_Row(
        kind: _RowKind.group,
        op: offer.op,
        candidateIndex: 0,
        title: offer.op,
        subtitle: offer.note,
        badge: '${offer.candidates.length} ways',
        fits: true,
        offerable: true,
        toolType: offer.toolType,
        toolState: offer.toolState,
        residual: offer.bestResidual,
        ghost: const [],
        muted: _isMuted(offer),
      ));
      for (var i = 0; i < offer.candidates.length; i++) {
        final candidate = offer.candidates[i];
        rows.add(_Row(
          kind: _RowKind.variant,
          op: offer.op,
          candidateIndex: candidate.index,
          title: '',
          subtitle: '',
          badge: _badge(
            exact: candidate.exact,
            approximate: candidate.approximate,
            residual: candidate.residual,
            mirrored: candidate.mirrored,
          ),
          fits: true,
          offerable: true,
          toolType: offer.toolType,
          toolState: offer.toolState,
          residual: candidate.residual,
          ghost: candidate.ghost,
          ordinal: i + 1,
          ordinalCount: offer.candidates.length,
        ));
      }
    }
    return rows;
  }

  /// The rows actually on screen.
  List<_Row> get _rows {
    if (_filter.isEmpty) return _allRows;
    final needle = _filter.toLowerCase();
    // Matching on `op` keeps a group header and its variants together: they
    // share the operation name, so they survive or vanish as one.
    return _allRows
        .where((row) => row.op.toLowerCase().startsWith(needle))
        .toList();
  }

  static String _badge({
    required bool exact,
    required bool approximate,
    required double residual,
    bool mirrored = false,
  }) {
    final fit = approximate
        ? 'approximate'
        : exact
            ? 'exact'
            : 'inexact, ${formatNatural(residual, 2)} Å';
    // `mirrored` belongs in the badge, not in a note: it is the one thing that
    // tells two placements of the same operation apart, so it has to survive
    // wherever the fit facts go.
    return mirrored ? '$fit · mirrored' : fit;
  }

  String get _anchorSymbol =>
      elementNumberToSymbol[widget.anchorAtomicNumber] ?? '?';

  bool _isMuted(APIMechanosynthOffer offer) =>
      widget.mutedOps.contains(offer.op);

  String get _header {
    // Counts what can be **placed**, not what fits: a row whose tip is spent is
    // not one of "3 operations apply here" from the user's point of view — and
    // not a muted one either, or muting a row from this list would leave the
    // header claiming an operation the list no longer shows.
    final fitting = widget.offers
        .where((o) => o.offerable && (widget.showingAll || !_isMuted(o)))
        .length;
    if (fitting == 0) {
      return 'Nothing applies to this $_anchorSymbol';
    }
    final noun = fitting == 1 ? 'operation applies' : 'operations apply';
    return '$fitting $noun to this $_anchorSymbol';
  }

  /// Selects a row, which previews it. Every caller is a deliberate user
  /// action — a click or an arrow key — never a hover and never a rebuild.
  ///
  /// A group header is a label, so the selection skips over it in whichever
  /// direction it was moving; landing on one would preview nothing and place
  /// nothing.
  void _setHighlight(int index, {int step = 1}) {
    final rows = _rows;
    if (rows.isEmpty) {
      setState(() => _highlight = -1);
      _reportPreview();
      return;
    }
    var next = index.clamp(0, rows.length - 1);
    while (!rows[next].isSelectable) {
      final after = next + step;
      if (after < 0 || after >= rows.length) {
        // Walking off the end: try the other way rather than stopping on a
        // label.
        var back = next - step;
        while (back >= 0 && back < rows.length && !rows[back].isSelectable) {
          back -= step;
        }
        if (back < 0 || back >= rows.length) return;
        next = back;
        break;
      }
      next = after;
    }
    setState(() {
      _highlight = next;
      _refused = null;
    });
    _reportPreview();
  }

  /// Reports the selected row once per change. The key is the row's identity
  /// rather than its position, so a filter that leaves the same row selected
  /// does not re-report it — and re-reporting is not free here.
  void _reportPreview() {
    final rows = _rows;
    if (rows.isEmpty || _highlight < 0 || _highlight >= rows.length) {
      if (_reportedKey != null) {
        _reportedKey = null;
        widget.onPreview(null);
      }
      return;
    }
    final row = rows[_highlight];
    if (!row.isSelectable) return;
    final key = '${row.op}#${row.candidateIndex}#${row.offerable}';
    if (key == _reportedKey) return;
    _reportedKey = key;
    widget.onPreview(MechanosynthPreview(
      op: row.op,
      candidateIndex: row.candidateIndex,
      // The warning colour means "this is being shown, not placed", which is
      // as true of a spent tip's perfect fit as of an over-gate one.
      nearMiss: row.isBlocked,
      ghost: row.ghost,
    ));
  }

  void _take(_Row row) {
    if (row.isBlocked) {
      // The refusal, in place of the row: an over-gate fit says the library has
      // not computed this environment, and a blocked tool says which state it
      // is in. Both have honest fixes, and the message names them.
      setState(() => _refused = row.op);
      return;
    }
    widget.onChoose(row.op, row.candidateIndex);
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final key = event.logicalKey;
    final rows = _rows;

    if (key == LogicalKeyboardKey.escape) {
      widget.onCancel();
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.arrowDown || key == LogicalKeyboardKey.tab) {
      // Tab wraps round the list; the arrows stop at the ends, which is what a
      // list the user is reading top-to-bottom should do.
      if (rows.isEmpty) return KeyEventResult.handled;
      _setHighlight(
          key == LogicalKeyboardKey.tab
              ? (_highlight + 1) % rows.length
              : _highlight + 1,
          step: 1);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.arrowUp) {
      if (rows.isEmpty) return KeyEventResult.handled;
      // From "nothing selected", Up takes the last row rather than staying put.
      _setHighlight(_highlight < 0 ? rows.length - 1 : _highlight - 1,
          step: -1);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      if (rows.isNotEmpty &&
          _highlight >= 0 &&
          _highlight < rows.length &&
          rows[_highlight].isSelectable) {
        _take(rows[_highlight]);
      }
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.backspace) {
      if (_filter.isNotEmpty) {
        setState(() {
          _filter = _filter.substring(0, _filter.length - 1);
          _highlight = -1;
          _reportedKey = null;
        });
        widget.onPreview(null);
      }
      return KeyEventResult.handled;
    }
    final character = event.character;
    if (character != null &&
        character.length == 1 &&
        character.trim().isNotEmpty) {
      setState(() {
        _filter += character;
        _refused = null;
        // Filtering only hides rows; it must not spend an evaluation picking a
        // new selection for the user.
        _highlight = -1;
        _reportedKey = null;
      });
      widget.onPreview(null);
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final rows = _rows;
    // Above the rule: what can be committed. Below it: near misses *and*
    // tool-blocked rows, which the kernel has already sorted after them.
    final applicable = rows.where((row) => !row.isBlocked).toList();
    final nearMisses = rows.where((row) => row.isBlocked).toList();

    return Focus(
      focusNode: _focusNode,
      onKeyEvent: _onKey,
      child: Material(
        elevation: 8,
        borderRadius: BorderRadius.circular(6),
        color: scheme.surfaceContainerHigh,
        child: SizedBox(
          width: MECHANOSYNTH_POPUP_WIDTH,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _buildHeader(context, scheme),
              if (rows.isNotEmpty)
                Flexible(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(
                        maxHeight: MECHANOSYNTH_POPUP_MAX_LIST_HEIGHT),
                    child: SingleChildScrollView(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          for (final row in applicable) _buildRow(row, rows),
                          if (nearMisses.isNotEmpty && applicable.isNotEmpty)
                            Divider(
                                height: 9,
                                thickness: 1,
                                color: scheme.outlineVariant),
                          for (final row in nearMisses) _buildRow(row, rows),
                        ],
                      ),
                    ),
                  ),
                ),
              _buildMutedFooter(context, scheme),
            ],
          ),
        ),
      ),
    );
  }

  /// The honesty line: *4 of 19 operations muted · show all here*.
  ///
  /// Shown whenever anything is muted, **whether or not any of it would have
  /// fitted**. The list's promise is that an empty result is a statement about
  /// the library's coverage (`doc/design_mechanosynth_editor.md` §*A miss
  /// becomes a coverage report*), and a filter that said nothing would turn
  /// that into a lie — an empty popup with no explanation is the worst thing
  /// muting could produce.
  ///
  /// It deliberately does **not** say how many of the muted operations apply
  /// here. Knowing that means running the placement search for them, which is
  /// exactly the cost the mute exists to avoid; *show all here* buys the same
  /// answer, once, when it is asked for.
  Widget _buildMutedFooter(BuildContext context, ColorScheme scheme) {
    final muted = widget.mutedCount;
    if (muted == 0 && !widget.showingAll) return const SizedBox.shrink();

    final text = widget.showingAll
        ? 'Showing all ${widget.libraryCount} operations'
        : '$muted of ${widget.libraryCount} operations muted';

    return Container(
      padding: const EdgeInsets.only(left: 10, right: 4, top: 3, bottom: 3),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHighest,
        borderRadius: const BorderRadius.vertical(bottom: Radius.circular(6)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              text,
              key: const Key('mechanosynth_popup_muted_footer'),
              style: TextStyle(
                fontSize: 11,
                color: scheme.onSurfaceVariant.withValues(alpha: 0.85),
              ),
            ),
          ),
          if (!widget.showingAll && widget.onShowAll != null)
            TextButton(
              key: const Key('mechanosynth_popup_show_all'),
              onPressed: widget.onShowAll,
              style: TextButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 6),
                minimumSize: Size.zero,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              ),
              child:
                  const Text('show all here', style: TextStyle(fontSize: 11)),
            ),
        ],
      ),
    );
  }

  /// The row's mute control, and — when the row is already muted — its badge.
  ///
  /// One control in two states rather than a chip plus a button: a lit eye-off
  /// says *this one is muted* and is also the way back, which is the whole of
  /// what a user needs from a muted row. The unlit one appears on hover, so a
  /// list nobody is pointing at carries no buttons.
  ///
  /// Only on rows that name an operation. A **variant** is one placement of an
  /// operation whose group header carries the name, and muting is per
  /// operation — a per-placement mute would be a different feature.
  Widget _buildMuteButton(_Row row, bool visible, ColorScheme scheme) {
    if (widget.onMute == null || row.kind == _RowKind.variant) {
      return const SizedBox.shrink();
    }
    // **A plain icon, not an `IconButton`.** The slot reserves the same box
    // whether or not the control is in it, so a row does not change height
    // when the pointer arrives — an `IconButton`'s 20 px tap target is taller
    // than the row's text, and the rows below would shift out from under the
    // pointer that is hovering this one.
    final icon = Icon(
      row.muted ? Icons.visibility_off : Icons.visibility_off_outlined,
      key: Key('mechanosynth_popup_mute_icon_${row.op}'),
      size: 14,
      color: row.muted
          ? scheme.tertiary
          : scheme.onSurfaceVariant.withValues(alpha: 0.7),
    );
    return SizedBox(
      width: 22,
      child: (row.muted || visible)
          ? Tooltip(
              message: row.muted
                  ? 'Muted — click to offer ${row.op} again'
                  : 'Mute — leave ${row.op} out of the offer list',
              child: GestureDetector(
                key: Key('mechanosynth_popup_mute_${row.op}'),
                behavior: HitTestBehavior.opaque,
                onTap: () => widget.onMute!(row.op, !row.muted),
                child: icon,
              ),
            )
          : null,
    );
  }

  /// The header doubles as the drag handle. A list that opens *on* the thing it
  /// describes will sometimes cover it however carefully it is placed — the
  /// automatic placement only knows the anchor and the ghosts, not the atoms
  /// the user is actually reading — so moving it has to be one drag, not a
  /// preference.
  Widget _buildHeader(BuildContext context, ColorScheme scheme) {
    final header = Container(
      padding: const EdgeInsets.only(left: 10, right: 4, top: 6, bottom: 6),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHighest,
        borderRadius: const BorderRadius.vertical(top: Radius.circular(6)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  _filter.isEmpty ? _header : '$_header · "$_filter"',
                  key: const Key('mechanosynth_popup_header'),
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w600,
                    color: scheme.onSurfaceVariant,
                  ),
                ),
              ],
            ),
          ),
          if (widget.moved && widget.onResetPosition != null)
            IconButton(
              key: const Key('mechanosynth_popup_reset_position'),
              icon: const Icon(Icons.filter_center_focus, size: 14),
              padding: EdgeInsets.zero,
              constraints: const BoxConstraints(minWidth: 22, minHeight: 22),
              visualDensity: VisualDensity.compact,
              color: scheme.onSurfaceVariant,
              tooltip: 'Snap back beside the atom',
              onPressed: widget.onResetPosition,
            ),
          Icon(Icons.drag_indicator,
              size: 15, color: scheme.onSurfaceVariant.withValues(alpha: 0.7)),
        ],
      ),
    );

    if (widget.onDrag == null) return header;
    return MouseRegion(
      cursor: SystemMouseCursors.move,
      child: GestureDetector(
        key: const Key('mechanosynth_popup_drag_handle'),
        behavior: HitTestBehavior.opaque,
        onPanUpdate: (details) => widget.onDrag!(details.delta),
        child: header,
      ),
    );
  }

  /// The direction the placement goes, as the user is currently looking at it.
  ///
  /// Falls back to a dot when the host cannot work one out — a bond-only
  /// operation has no ghost atoms to take a centroid of. The ordinal beside it
  /// is what keeps two placements distinguishable when their arrows agree,
  /// which happens whenever they are symmetric about the view axis.
  Widget _arrow(_Row row, ColorScheme scheme) {
    final angle = widget.arrowAngleFor?.call(row.ghost);
    final color = scheme.onSurfaceVariant;
    if (angle == null) {
      return Icon(Icons.circle, size: 7, color: color.withValues(alpha: 0.5));
    }
    return Transform.rotate(
      angle: angle,
      child: Icon(Icons.arrow_forward, size: 14, color: color),
    );
  }

  /// What the row's ⓘ says: the library's note, and the instrument when the
  /// row carries a ready one. Empty means no icon at all.
  static String _tooltipText(_Row row) {
    final tool = row.offerable ? row.toolSummary : '';
    if (tool.isEmpty) return row.subtitle;
    return row.subtitle.isEmpty ? tool : '${row.subtitle}\n$tool';
  }

  Widget _buildRow(_Row row, List<_Row> rows) {
    final scheme = Theme.of(context).colorScheme;
    final index = rows.indexOf(row);
    final highlighted = index == _highlight;
    final dim = row.isBlocked;
    final variant = row.kind == _RowKind.variant;
    final titleColor =
        dim ? scheme.onSurfaceVariant.withValues(alpha: 0.6) : scheme.onSurface;

    if (_refused == row.op && dim) {
      // Two different refusals, and they have different fixes. An over-gate fit
      // says the *library* has not computed this environment; a blocked tool
      // says the instrument is in the wrong state, and the fix is a step, not
      // an edit to the library.
      final reason = row.fits
          ? '${row.toolReason}. Author the step that puts it back in the '
              'state this operation needs — a recharge is a placement on the '
              'reservoir like any other.'
          : '${formatNatural(row.residual, 2)} Å off; this host is not an '
              'environment ${row.op} was calculated for. Add the variant, or '
              "loosen the library's tolerance.";
      return Container(
        key: Key('mechanosynth_popup_reason_${row.op}'),
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
        color: scheme.errorContainer.withValues(alpha: 0.4),
        child: Text(
          reason,
          style: TextStyle(fontSize: 11.5, color: scheme.onSurface),
        ),
      );
    }

    // **Hover previews, click places.** Resting on a row is how you ask what it
    // would do; clicking is how you say do it. The preview costs an
    // evaluation, which is why hovering arms a timer rather than firing.
    return InkWell(
      // A group header and its first variant share an operation and a
      // candidate index, so they cannot share a key.
      key: row.kind == _RowKind.group
          ? Key('mechanosynth_popup_group_${row.op}')
          : Key('mechanosynth_popup_row_${row.op}_${row.candidateIndex}'),
      onHover: (hovering) {
        // Tracked for **every** row, a group header included: the mute button
        // is per operation, and a group header is the only place a multi-way
        // operation's name appears. Showing a button costs a repaint, so this
        // needs none of the preview's delay machinery.
        final hoveredNow = hovering ? index : -1;
        if (_hovered != hoveredNow) setState(() => _hovered = hoveredNow);
        if (!row.isSelectable) return;
        if (hovering) {
          _hoverRow(index);
        } else {
          _cancelPendingHover();
        }
      },
      onTap: () {
        if (!row.isSelectable) return;
        _cancelPendingHover();
        // A blocked row cannot be placed, so a click on one is still a
        // question: it previews immediately and puts the reason in place of
        // the row.
        if (row.isBlocked) {
          _setHighlight(index);
          setState(() => _refused = row.op);
          return;
        }
        _take(row);
      },
      child: Container(
        padding: EdgeInsets.only(
            left: variant ? 0 : 10,
            right: MECHANOSYNTH_POPUP_SCROLLBAR_GUTTER,
            top: 3,
            bottom: 3),
        color: highlighted
            ? scheme.primary.withValues(alpha: 0.14)
            : Colors.transparent,
        child: Row(
          children: [
            // A variant is indented behind a rule running down the group, which
            // is what says "these are the same operation" without repeating its
            // name on every line or asking for a tree widget.
            if (variant) ...[
              const SizedBox(width: 10),
              Container(
                width: 1.5,
                height: 16,
                color: scheme.outlineVariant,
              ),
              const SizedBox(width: 7),
              _arrow(row, scheme),
              const SizedBox(width: 5),
              Text(
                '${row.ordinal}',
                style: TextStyle(
                  fontSize: 11,
                  color: scheme.onSurfaceVariant.withValues(alpha: 0.8),
                  fontWeight: highlighted ? FontWeight.w600 : FontWeight.normal,
                ),
              ),
            ],
            Expanded(
              child: Text(
                row.title,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 12.5,
                  color: titleColor,
                  fontWeight: highlighted || row.kind == _RowKind.group
                      ? FontWeight.w600
                      : FontWeight.normal,
                ),
              ),
            ),
            const SizedBox(width: 6),
            // Flexible, not a bare Text: a variant row spends most of its width
            // on the indent, the rule, the arrow and two buttons, and
            // `exact · mirrored` is long enough to overflow what is left at
            // 300 px. It shrinks before it clips, and normally does neither.
            Flexible(
              child: Text(
                row.badge,
                softWrap: false,
                overflow: TextOverflow.fade,
                style: TextStyle(
                  fontSize: 11,
                  color: dim
                      ? scheme.onSurfaceVariant.withValues(alpha: 0.6)
                      : scheme.onSurfaceVariant,
                ),
              ),
            ),
            // The library's own note, behind an icon rather than on a second
            // line of its own. At 300 px the note was always elided to a
            // fragment, so it read as clutter while saying nothing; here it is
            // whole, on demand, and the row is half as tall.
            //
            // **A ready row's instrument goes here too**, rather than into a
            // chip of its own. The row already spends its width on a title, a
            // badge and (for a variant) an indent, an arrow and an ordinal;
            // and a ready tool is not news — the panel's *Tools* readout is
            // what says a recharge is due. A *blocked* tool is news, and it
            // gets the badge instead.
            _buildMuteButton(row, index == _hovered || highlighted, scheme),
            SizedBox(
              // A variant carries no note of its own — the note is the
              // operation's, and it is on the group header — so it reclaims
              // the width instead of reserving it.
              width: variant ? 0 : 22,
              child: _tooltipText(row).isEmpty
                  ? null
                  : MechanosynthNoteTooltip(
                      key: Key(
                          'mechanosynth_popup_info_${row.op}_${row.candidateIndex}'),
                      note: _tooltipText(row),
                      child: Icon(
                        Icons.info_outline,
                        size: 14,
                        color: scheme.onSurfaceVariant
                            .withValues(alpha: dim ? 0.4 : 0.7),
                      ),
                    ),
            ),
          ],
        ),
      ),
    );
  }
}
