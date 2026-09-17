/// Editor widget for `mechanosynth_edit` nodes — the authored build block.
///
/// The node's job is authoring, so this panel is a **step list with a cursor**,
/// not a file form: the library and the prefix arrive on wires (`ops`,
/// `steps`), the prefix is summarised as one collapsed row because a generated
/// block is hundreds of steps nobody edits here, and everything below it is the
/// block the user wrote. See `doc/design_mechanosynth_editor.md`.
///
/// **Placement happens in the viewport, not here.** Placement is atom-first:
/// click an atom and the library answers with what fits it, in a popup anchored
/// to that atom (`mechanosynth_offer_popup.dart`). There is no armed mode to
/// arm from the panel, so the prompt line is the panel's contribution to the
/// tool: it says what a click will do next.
///
/// What the panel's **Operations** section does own is *muting*
/// (`doc/design_mechanosynth_op_muting.md`): which of the wired library's
/// operations this node's offer sweep asks about. A mute is a view filter over
/// that sweep and nothing else — a muted operation still replays, still
/// exports, and still means what it means — so nothing in the step list below
/// changes when one is set.
///
/// Four things about the list are worth knowing before changing it:
///
/// - **The cursor is navigation, not an edit.** Moving it records no undo
///   entry, exactly as the replayer's slider does; every other mutation here is
///   one entry. Selecting a row moves the cursor to that row's number, so the
///   viewport shows the state just after that step.
/// - **There is no general "insert step".** A step that was never fitted
///   against a workpiece has no `(r, t)` to write, so the only insertion the
///   panel offers is **Duplicate**, which copies a fit that already exists —
///   residual and `approximate` flag included, because the copy describes the
///   same fit.
/// - **A chip edit coalesces.** Consecutive writes to the same field of the
///   same step merge into one undo entry in the kernel, so typing a note costs
///   one Ctrl+Z rather than one per keystroke.
/// - **A failing block still shows a step list and a viewport.** When the
///   cursor step's tool side does not match, the pins carry the error — a
///   downstream export must never receive a truncated build — but the viewport
///   keeps drawing the last good state and the cursor row carries the engine's
///   message as an error chip. That is the state the *Making a sequence
///   tool-aware* walk lives in: the user reads the reason, clicks the
///   reservoir, and inserts the recharge in front of the failing step.
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_scrubber.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_status.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

class MechanosynthEditEditor extends StatefulWidget {
  final BigInt nodeId;

  /// The whole panel view, fetched by the router on every rebuild: the prefix
  /// length and the library's operation names come from *evaluating* wires, so
  /// they cannot be cached in the widget.
  final APIMechanosynthEditData? data;

  /// Whether the `ops` / `steps` pins are wired. Without a library there is
  /// nothing to place, and the panel says so rather than offering an empty
  /// palette.
  final bool opsConnected;
  final bool stepsConnected;

  final StructureDesignerModel model;

  const MechanosynthEditEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.opsConnected,
    required this.stepsConnected,
    required this.model,
  });

  @override
  State<MechanosynthEditEditor> createState() => _MechanosynthEditEditorState();
}

class _MechanosynthEditEditorState extends State<MechanosynthEditEditor> {
  /// The palette's filter box.
  String _paletteFilter = '';

  /// Whether the palette is expanded. Collapsed by default: the atom-first flow
  /// needs none of it, and a twenty-operation library would otherwise push the
  /// step list off the panel.
  bool _paletteOpen = false;

  /// The last failed kernel call — an arm with no library wired, a refused
  /// choose. Evaluation failures come through [APIMechanosynthEditData.lastError]
  /// instead and are shown separately.
  String? _errorMessage;

  /// The cursor under the pointer while the scrubber is dragged; suppresses the
  /// "current step" readout, which the panel cannot recompute mid-drag.
  int? _previewCursor;

  void _report(String? error) => setState(() => _errorMessage = error);

  // ==========================================================================
  // Prompt
  // ==========================================================================

  /// What a viewport click will do next. The four states are the kernel's
  /// (`doc/design_mechanosynth_editor.md` §States) and arrive as a string, so
  /// the panel never re-derives them from the rest of the data.
  Widget _buildPrompt(BuildContext context, APIMechanosynthEditData data) {
    final scheme = Theme.of(context).colorScheme;
    final style = TextStyle(fontSize: 12.0, color: scheme.onSurfaceVariant);

    if (!widget.opsConnected) {
      return Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.link_off, size: 16, color: scheme.onSurfaceVariant),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              'Wire an ops_library to the ops pin — the tool places the '
              "library's operations, so there is nothing to offer without one.",
              style: style,
            ),
          ),
        ],
      );
    }

    final String text;
    switch (data.toolState) {
      case 'offers':
        text = 'Choose an operation in the viewport popup · Esc to close';
      case 'candidates':
        text = 'Choose a placement in the viewport popup · Esc to abandon';
      default:
        text = 'Click an atom to see what can be done there.';
    }
    return Text(text, key: const Key('mechanosynth_edit_prompt'), style: style);
  }

  // ==========================================================================
  // Palette: which of the library's operations this node works with
  // ==========================================================================
  //
  // Not a placement tool. Clicking a name used to arm the operation, so that
  // the next viewport click placed it; that mode is gone (see
  // `mechanosynth_edit_ops.rs::commit_candidate`) because the library splits
  // one reaction into one operation per host environment, which makes "the
  // same operation again" the wrong default at the next site. Placement is
  // atom-first: click an atom, and the offer popup answers with what fits
  // *there*.
  //
  // What the list is for instead is **muting**
  // (`doc/design_mechanosynth_op_muting.md`). A library grows on purpose and
  // the offer popup stays short because the fit resolves which *variant*
  // applies — but a whole `bulk` method the author is not using this phase fits
  // nearly everywhere, and is on every popup. Unchecking it here takes it out
  // of this node's offer sweep.
  //
  // Three affordances over one piece of state, and that is the invariant to
  // keep: a group chip and *Mute these* both write individual names into the
  // node's set, so there is no second kind of "muted group" that a library
  // gaining a twentieth operation could silently capture.

  void _setMuted(Iterable<String> names, bool muted) {
    final list = names.toList();
    if (list.isEmpty) return;
    _report(widget.model.setMechanosynthEditMuted(widget.nodeId, list, muted));
  }

  Widget _buildPalette(BuildContext context, APIMechanosynthEditData data) {
    // Shown whenever there is anything to show. **Not** gated on `opsConnected`:
    // an unwired node can still carry a mute set — the pin gets rewired — and a
    // mute the user cannot see is a mute they cannot undo.
    if (data.ops.isEmpty) return const SizedBox.shrink();

    final scheme = Theme.of(context).colorScheme;
    final needle = _paletteFilter.trim().toLowerCase();
    final matches =
        data.ops.where((op) => op.name.toLowerCase().contains(needle)).toList();
    final mutedCount = data.ops.where((op) => op.muted).length;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        InkWell(
          key: const Key('mechanosynth_edit_palette_toggle'),
          onTap: () => setState(() => _paletteOpen = !_paletteOpen),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4.0),
            child: Row(
              children: [
                Icon(
                  _paletteOpen ? Icons.expand_more : Icons.chevron_right,
                  size: 18,
                  color: scheme.onSurfaceVariant,
                ),
                // "15 / 19" only while something is muted, so the state is
                // legible without expanding the section.
                Text(
                  mutedCount == 0
                      ? 'Operations (${data.ops.length})'
                      : 'Operations (${data.ops.length - mutedCount} / '
                          '${data.ops.length})',
                  key: const Key('mechanosynth_edit_palette_count'),
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
        ),
        if (_paletteOpen) ...[
          _buildGroupChips(context, data.ops),
          StringInput(
            label: 'Filter',
            value: _paletteFilter,
            onChanged: (text) => setState(() => _paletteFilter = text),
          ),
          if (needle.isNotEmpty) _buildBulkButtons(context, matches),
          const SizedBox(height: 4),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 200),
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final op in matches) _buildPaletteRow(context, op),
                ],
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.only(top: 4.0),
            child: Text(
              'Unchecked operations are left out of this node\'s offer list. '
              'Muting never changes what an authored step does.',
              style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
            ),
          ),
          const SizedBox(height: 8),
        ],
      ],
    );
  }

  /// One tri-state chip per instrument: all offered, some muted, all muted.
  ///
  /// Tapping mutes the whole group unless it is already wholly muted, in which
  /// case it unmutes — so the chip is where "I am not doing precursor deposits
  /// this week" is one click. It writes the group's names individually; there
  /// is no stored notion of a muted group.
  Widget _buildGroupChips(BuildContext context, List<APIMechanosynthOp> ops) {
    final groups = groupOperationsByInstrument(ops);
    if (groups.length < 2) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;

    return Padding(
      padding: const EdgeInsets.only(bottom: 6.0),
      child: Wrap(
        spacing: 4.0,
        runSpacing: 4.0,
        children: [
          for (final group in groups)
            () {
              final muted = group.value.where((op) => op.muted).length;
              final allMuted = muted == group.value.length;
              final color = methodColor(group.value.first.method) ??
                  scheme.onSurfaceVariant;
              return InkWell(
                key: Key('mechanosynth_edit_group_${group.key}'),
                onTap: () =>
                    _setMuted(group.value.map((op) => op.name), !allMuted),
                child: Tooltip(
                  message: allMuted
                      ? 'Offer ${group.value.length} ${group.key} operations '
                          'again'
                      : 'Leave ${group.value.length} ${group.key} operations '
                          'out of the offer list',
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                        horizontal: 6.0, vertical: 2.0),
                    decoration: BoxDecoration(
                      color: allMuted
                          ? null
                          : color.withValues(alpha: muted == 0 ? 0.18 : 0.08),
                      border: Border.all(
                          color:
                              color.withValues(alpha: allMuted ? 0.25 : 0.55)),
                      borderRadius: BorderRadius.circular(4.0),
                    ),
                    child: Text(
                      '${group.key} ${group.value.length - muted}/'
                      '${group.value.length}',
                      style: TextStyle(
                        fontSize: 11,
                        color: allMuted
                            ? scheme.onSurfaceVariant.withValues(alpha: 0.6)
                            : color,
                        decoration:
                            allMuted ? TextDecoration.lineThrough : null,
                      ),
                    ),
                  ),
                ),
              );
            }(),
        ],
      ),
    );
  }

  /// *Mute these* / *Unmute these*, acting on whatever the filter box matched.
  ///
  /// This is the general escape from any grouping the chips do not express:
  /// typing `cl_donate` and pressing *Mute these (4)* is prefix-family muting
  /// without a prefix-family concept.
  Widget _buildBulkButtons(
      BuildContext context, List<APIMechanosynthOp> matches) {
    final offered = matches.where((op) => !op.muted).toList();
    final muted = matches.where((op) => op.muted).toList();
    return Padding(
      padding: const EdgeInsets.only(top: 2.0),
      child: Row(
        children: [
          TextButton(
            key: const Key('mechanosynth_edit_mute_filtered'),
            onPressed: offered.isEmpty
                ? null
                : () => _setMuted(offered.map((op) => op.name), true),
            style: TextButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 6.0),
                minimumSize: Size.zero,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap),
            child: Text('Mute these (${offered.length})',
                style: const TextStyle(fontSize: 11)),
          ),
          TextButton(
            key: const Key('mechanosynth_edit_unmute_filtered'),
            onPressed: muted.isEmpty
                ? null
                : () => _setMuted(muted.map((op) => op.name), false),
            style: TextButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 6.0),
                minimumSize: Size.zero,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap),
            child: Text('Unmute these (${muted.length})',
                style: const TextStyle(fontSize: 11)),
          ),
        ],
      ),
    );
  }

  /// One operation: the checkbox that mutes it, its name, and its instrument.
  ///
  /// An **empty `method`** is a muted name the wired library does not define —
  /// kept on purpose, because the `ops` pin may be rewired back. It is greyed
  /// and says so, rather than being dropped, which would leave a mute nobody
  /// could find to undo.
  Widget _buildPaletteRow(BuildContext context, APIMechanosynthOp op) {
    final scheme = Theme.of(context).colorScheme;
    final unknown = op.method.isEmpty;
    final color = methodColor(op.method);

    return InkWell(
      key: Key('mechanosynth_edit_palette_${op.name}'),
      onTap: () => _setMuted([op.name], !op.muted),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 2.0, vertical: 2.0),
        child: Row(
          children: [
            Icon(
              op.muted ? Icons.check_box_outline_blank : Icons.check_box,
              key: Key('mechanosynth_edit_palette_check_${op.name}'),
              size: 16,
              color: op.muted ? scheme.onSurfaceVariant : scheme.primary,
            ),
            const SizedBox(width: 6),
            Expanded(
              child: Text(
                op.name,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 12,
                  color: op.muted || unknown
                      ? scheme.onSurfaceVariant
                      : scheme.onSurface,
                  fontStyle: unknown ? FontStyle.italic : null,
                ),
              ),
            ),
            if (unknown)
              Tooltip(
                message: 'Not in the wired library. The mute is kept — rewire '
                    'the ops pin and it applies again.',
                child: Icon(Icons.help_outline,
                    size: 14, color: scheme.onSurfaceVariant),
              )
            else ...[
              if (op.note.isNotEmpty) ...[
                Tooltip(
                  message: op.note,
                  child: Icon(Icons.info_outline,
                      size: 13, color: scheme.onSurfaceVariant),
                ),
                const SizedBox(width: 4),
              ],
              Text(
                instrumentOf(op),
                style: TextStyle(
                  fontSize: 10.5,
                  color: (color ?? scheme.onSurfaceVariant)
                      .withValues(alpha: op.muted ? 0.5 : 1.0),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  // ==========================================================================
  // The prefix, the cursor, the block
  // ==========================================================================

  Widget _buildPrefixRow(BuildContext context, APIMechanosynthEditData data) {
    if (!widget.stepsConnected && data.prefixCount == 0) {
      return const SizedBox.shrink();
    }
    final scheme = Theme.of(context).colorScheme;
    return Container(
      margin: const EdgeInsets.only(bottom: 6.0),
      padding: const EdgeInsets.symmetric(horizontal: 6.0, vertical: 4.0),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(4.0),
      ),
      child: Row(
        children: [
          Icon(Icons.link, size: 14, color: scheme.onSurfaceVariant),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              data.prefixCount == 1
                  ? '1 step from the steps pin'
                  : '${data.prefixCount} steps from the steps pin',
              style: TextStyle(fontSize: 11.5, color: scheme.onSurfaceVariant),
            ),
          ),
          if (data.prefixCount > 0)
            TextButton(
              key: const Key('mechanosynth_edit_adopt_prefix'),
              onPressed: _adoptPrefix,
              style: TextButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 8.0),
                minimumSize: Size.zero,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                textStyle: const TextStyle(fontSize: 11.5),
              ),
              child: const Text('Adopt these into the block'),
            ),
        ],
      ),
    );
  }

  /// One press: the wired steps become authored steps and the wire goes.
  ///
  /// The confirmation is said **once, here**, and not left as a standing label
  /// on the node. A node that needed permanent chrome explaining what it is
  /// *not* tracking would be the wrong affordance; what the user has just
  /// watched — rows moving out of the collapsed prefix row into the editable
  /// list, the wire disappearing — is most of the message already.
  void _adoptPrefix() {
    final result = widget.model.mechanosynthEditAdoptPrefix(widget.nodeId);
    _report(result.error);
    final adopted = result.value;
    if (adopted == null) return;
    showTransientSnackBar(
      context,
      adopted == 1
          ? '1 step copied into the block. It no longer follows the file.'
          : '$adopted steps copied into the block. They no longer follow '
              'the file.',
    );
  }

  /// The same one-shot import, from a file dialog, at the cursor.
  ///
  /// Not gated on the `steps` pin: what decides whether an imported step
  /// replays is the workpiece state where it lands, not how the steps ahead of
  /// it arrived.
  ///
  /// A file dialog and nothing else: no path is stored, so there is no field to
  /// show afterwards and nothing to reload.
  Future<void> _insertStepsFromFile(APIMechanosynthEditData data) async {
    String? path;
    try {
      final picked = await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['json'],
        dialogTitle: 'Insert build steps into the block',
        initialDirectory:
            initialDirectoryFor(APIFileDialogPurpose.structureImport),
      );
      path = picked?.files.single.path;
    } catch (e) {
      _report('Error browsing file: $e');
      return;
    }
    if (path == null) return;
    rememberPickedFile(APIFileDialogPurpose.structureImport, path);

    // At the cursor, so an import lands where the user is looking — the same
    // place a placed step lands.
    final result = widget.model
        .mechanosynthEditInsertStepsFromFile(widget.nodeId, path, data.applied);
    if (!mounted) return;
    _report(result.error);
    final inserted = result.value;
    if (inserted == null) return;
    showTransientSnackBar(
      context,
      inserted == 1
          ? '1 step inserted into the block. It has no link to the file.'
          : '$inserted steps inserted into the block. They have no link to '
              'the file.',
    );
  }

  /// The counts of steps whose fit was not exact. A design with neither line is
  /// exact to the file rounding a generator would have written.
  Widget _buildFitSummary(BuildContext context, APIMechanosynthEditData data) {
    if (data.inexactCount == 0 && data.approximateCount == 0) {
      return const SizedBox.shrink();
    }
    final scheme = Theme.of(context).colorScheme;
    final parts = <String>[
      if (data.inexactCount > 0) '${data.inexactCount} inexact',
      if (data.approximateCount > 0) '${data.approximateCount} approximate',
    ];
    return Padding(
      padding: const EdgeInsets.only(top: 6.0),
      child: Row(
        children: [
          Icon(Icons.warning_amber_rounded, size: 15, color: scheme.tertiary),
          const SizedBox(width: 6),
          Expanded(
            child: Text(parts.join(' · '),
                key: const Key('mechanosynth_edit_fit_summary'),
                style: TextStyle(fontSize: 11.5, color: scheme.tertiary)),
          ),
        ],
      ),
    );
  }

  /// Says what the viewport is drawing when the block fails at the cursor step.
  ///
  /// Without it the view is a lie by omission: the pins carry an error, so a
  /// user who knows the rules expects an empty viewport, and the structure in
  /// front of them is the state *before* the failing step rather than after it.
  /// The atom count is the kernel's, off the parked scene — `-1` means the
  /// block did not fail.
  Widget _buildLastGoodState(
      BuildContext context, APIMechanosynthEditData data) {
    if (data.lastGoodAtomCount < 0) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.only(top: 6.0),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.visibility_outlined, size: 14, color: scheme.tertiary),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              'Showing the last good state — ${data.lastGoodAtomCount} atoms, '
              'before step ${data.applied}.',
              key: const Key('mechanosynth_edit_last_good'),
              style: TextStyle(fontSize: 11.5, color: scheme.tertiary),
            ),
          ),
        ],
      ),
    );
  }

  /// The block's own file action, below the list where the block is.
  ///
  /// Deliberately **not** a path field with a Browse and a Reload beside it:
  /// that shape is the file nodes' (`build_script`, `ops_library`,
  /// `import_xyz`) and it promises a live link. This is a verb, it runs once,
  /// and it leaves nothing behind to reload.
  Widget _buildBlockActions(
      BuildContext context, APIMechanosynthEditData data) {
    return Padding(
      padding: const EdgeInsets.only(top: 6.0),
      child: Align(
        alignment: Alignment.centerLeft,
        child: TextButton.icon(
          key: const Key('mechanosynth_edit_insert_from_file'),
          onPressed: () => _insertStepsFromFile(data),
          icon: const Icon(Icons.playlist_add, size: 16),
          style: TextButton.styleFrom(
            padding: const EdgeInsets.symmetric(horizontal: 8.0),
            minimumSize: Size.zero,
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            textStyle: const TextStyle(fontSize: 11.5),
          ),
          label: const Text('Insert steps from file…'),
        ),
      ),
    );
  }

  Widget _buildStepList(BuildContext context, APIMechanosynthEditData data) {
    if (data.authored.isEmpty) {
      final scheme = Theme.of(context).colorScheme;
      return Padding(
        padding: const EdgeInsets.only(top: 8.0),
        child: Text(
          'No authored steps yet.',
          style: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.only(top: 8.0),
      child: ReorderableListView.builder(
        shrinkWrap: true,
        physics: const NeverScrollableScrollPhysics(),
        buildDefaultDragHandles: false,
        itemCount: data.authored.length,
        // ReorderableListView reports the destination as an index in the list
        // *before* the removal, so a downward move is one too far.
        onReorder: (from, to) => _report(widget.model.mechanosynthEditMoveStep(
            widget.nodeId, from, to > from ? to - 1 : to)),
        itemBuilder: (context, index) =>
            _buildStepRow(context, data, index, key: ValueKey('step_$index')),
      ),
    );
  }

  Widget _buildStepRow(
    BuildContext context,
    APIMechanosynthEditData data,
    int index, {
    required Key key,
  }) {
    final scheme = Theme.of(context).colorScheme;
    final step = data.authored[index];
    // Rows are numbered from 1, so row `k` is the step the cursor has just
    // applied.
    final isCursor = data.applied == index + 1;
    final color = methodColor(step.method);
    // The cursor step is the last one the block replayed, so a block failure is
    // *this* row's failure — there is no other row it could belong to.
    //
    // `lastGoodAtomCount >= 0` is what distinguishes a **block** failure from
    // an *input* one (a bad library, an erroring prefix): a block failure hands
    // back the state before the failing step, an input failure never ran a step
    // and hands back nothing. An input failure belongs to no row, and the
    // banner at the foot of the panel carries it.
    final blockError = data.lastGoodAtomCount >= 0 ? data.lastError : null;
    final failed = isCursor && blockError != null;

    return Container(
      key: key,
      margin: const EdgeInsets.only(bottom: 2.0),
      decoration: BoxDecoration(
        color: isCursor ? scheme.primary.withValues(alpha: 0.10) : null,
        borderRadius: BorderRadius.circular(3.0),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InkWell(
            onTap: () => widget.model
                .setMechanosynthEditCursor(widget.nodeId, index + 1),
            child: Padding(
              padding:
                  const EdgeInsets.symmetric(horizontal: 4.0, vertical: 3.0),
              child: Row(
                children: [
                  ReorderableDragStartListener(
                    index: index,
                    child: Icon(Icons.drag_indicator,
                        size: 15, color: scheme.onSurfaceVariant),
                  ),
                  SizedBox(
                    width: 26,
                    child: Text('${index + 1}',
                        style: TextStyle(
                            fontSize: 11, color: scheme.onSurfaceVariant)),
                  ),
                  if (color != null) ...[
                    Container(
                      width: 6,
                      height: 6,
                      decoration:
                          BoxDecoration(color: color, shape: BoxShape.circle),
                    ),
                    const SizedBox(width: 5),
                  ],
                  Expanded(
                    child: Text(
                      step.note.isEmpty ? step.op : '${step.op} — ${step.note}',
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 12,
                        color: scheme.onSurface,
                        fontWeight:
                            isCursor ? FontWeight.w600 : FontWeight.normal,
                      ),
                    ),
                  ),
                  if (failed)
                    Padding(
                      padding: const EdgeInsets.only(left: 4.0),
                      child: Icon(Icons.error_outline,
                          key: const Key('mechanosynth_edit_step_error_icon'),
                          size: 14,
                          color: scheme.error),
                    ),
                  if (!step.exact || step.approximate)
                    Padding(
                      padding: const EdgeInsets.only(left: 4.0),
                      child: Tooltip(
                        message: step.approximate
                            ? 'orientation derived from bonds'
                            : 'fit residual ${formatNatural(step.residual, 2)} Å',
                        child: Icon(Icons.warning_amber_rounded,
                            size: 14, color: scheme.tertiary),
                      ),
                    ),
                  IconButton(
                    icon: const Icon(Icons.copy, size: 15),
                    tooltip: 'Duplicate',
                    visualDensity: VisualDensity.compact,
                    constraints:
                        const BoxConstraints(minWidth: 26, minHeight: 26),
                    padding: EdgeInsets.zero,
                    onPressed: () => _report(widget.model
                        .mechanosynthEditDuplicateStep(widget.nodeId, index)),
                  ),
                  IconButton(
                    icon: const Icon(Icons.delete_outline, size: 15),
                    tooltip: 'Delete',
                    visualDensity: VisualDensity.compact,
                    constraints:
                        const BoxConstraints(minWidth: 26, minHeight: 26),
                    padding: EdgeInsets.zero,
                    onPressed: () => _report(widget.model
                        .mechanosynthEditDeleteStep(widget.nodeId, index)),
                  ),
                ],
              ),
            ),
          ),
          // The engine's message, in place on the row that produced it. The
          // banner at the foot of the panel says the same thing; this is what
          // says *which step*, which is the half the walk needs.
          if (failed)
            Padding(
              padding: const EdgeInsets.fromLTRB(30.0, 0.0, 4.0, 5.0),
              child: Container(
                key: const Key('mechanosynth_edit_step_error_chip'),
                padding:
                    const EdgeInsets.symmetric(horizontal: 6.0, vertical: 3.0),
                decoration: BoxDecoration(
                  color: scheme.errorContainer.withValues(alpha: 0.5),
                  borderRadius: BorderRadius.circular(3.0),
                ),
                child: SelectableText(
                  blockError,
                  style: TextStyle(fontSize: 11.0, color: scheme.onSurface),
                ),
              ),
            ),
          // Only the cursor row opens its chips: five fields on every row of a
          // fifty-step block would bury the list the chips describe.
          if (isCursor) _buildChips(context, index, step),
        ],
      ),
    );
  }

  Widget _buildChips(BuildContext context, int index, APIAuthoredStep step) {
    void write(String field, {String text = '', int number = -1}) =>
        _report(widget.model.setMechanosynthEditStepMetadata(
            widget.nodeId, index, field,
            text: text, number: number));

    return Padding(
      padding: const EdgeInsets.fromLTRB(30.0, 0.0, 4.0, 6.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // **The method is not a field.** It used to be one; it is the
          // operation's kind now, so the row states it and offers no way to
          // type it. The detail beside it is the instrument (`tip`) or the
          // agent (`bulk`) — also the operation's, also not a choice.
          if (step.method.isNotEmpty) ...[
            Align(
              alignment: Alignment.centerLeft,
              child: MechanosynthMethodBadge(
                key: const Key('mechanosynth_edit_method_badge'),
                method: step.method,
                detail:
                    methodDetail(toolType: step.toolType, agent: step.agent),
              ),
            ),
            const SizedBox(height: 6),
          ],
          StringInput(
            label: 'Note',
            value: step.note,
            onChanged: (text) => write('note', text: text),
          ),
          const SizedBox(height: 4),
          StringInput(
            label: 'Phase',
            value: step.phase,
            onChanged: (text) => write('phase', text: text),
          ),
          const SizedBox(height: 4),
          Row(
            children: [
              Expanded(
                child: IntInput(
                  label: 'Layer',
                  value: step.layer,
                  minimumValue: -1,
                  onChanged: (value) => write('layer', number: value),
                ),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: IntInput(
                  label: 'Site',
                  value: step.site,
                  minimumValue: -1,
                  onChanged: (value) => write('site', number: value),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final scheme = Theme.of(context).colorScheme;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Mechanosynth Edit',
            nodeTypeName: 'mechanosynth_edit',
          ),
          const SizedBox(height: 12),
          _buildPrompt(context, data),
          const SizedBox(height: 8),
          _buildPalette(context, data),
          _buildPrefixRow(context, data),
          MechanosynthScrubber.fromEditData(
            data,
            onChanged: (cursor) =>
                widget.model.setMechanosynthEditCursor(widget.nodeId, cursor),
            onPreview: (cursor) => setState(() => _previewCursor = cursor),
          ),
          if (_previewCursor == null && data.authored.isNotEmpty)
            Text(
              data.applied == 0
                  ? 'Cursor 0 of ${data.authored.length} — before the block.'
                  : 'Cursor ${data.applied} of ${data.authored.length}: '
                      '${data.authored[data.applied - 1].op}',
              style: TextStyle(fontSize: 11.5, color: scheme.onSurfaceVariant),
            ),
          _buildFitSummary(context, data),
          // What tells the user a recharge is due **before** the offers do: the
          // tool states at the cursor, refreshed by every cursor move and
          // commit because the whole panel is.
          MechanosynthToolsReadout(
            tools: data.tools,
            feedstocks: data.feedstocks,
          ),
          _buildLastGoodState(context, data),
          MechanosynthChapterList(
            chapters: data.chapters,
            applied: _previewCursor ?? data.applied,
            onJump: (step) =>
                widget.model.setMechanosynthEditCursor(widget.nodeId, step),
          ),
          _buildStepList(context, data),
          _buildBlockActions(context, data),
          if (data.lastError != null)
            Padding(
              padding: const EdgeInsets.only(top: 12.0),
              child: ErrorBanner(message: data.lastError!),
            ),
          if (_errorMessage != null)
            Padding(
              padding: const EdgeInsets.only(top: 12.0),
              child: ErrorBanner(message: _errorMessage!),
            ),
        ],
      ),
    );
  }
}
