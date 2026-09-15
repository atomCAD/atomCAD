/// Editor widget for `mechanosynth_edit` nodes — the authored build block.
///
/// The node's job is authoring, so this panel is a **step list with a cursor**,
/// not a file form: the library and the prefix arrive on wires (`ops`,
/// `steps`), the prefix is summarised as one collapsed row because a generated
/// block is hundreds of steps nobody edits here, and everything below it is the
/// block the user wrote. See `doc/design_mechanosynth_editor.md`.
///
/// **Placement happens in the viewport, not here.** The panel's palette exists
/// only for the op-first repeat flow — arm an operation, then click hosts — and
/// the primary flow is the other way round: click an atom and the library
/// answers with what fits it, in a popup anchored to that atom
/// (`mechanosynth_offer_popup.dart`). So the prompt line is the panel's main
/// contribution to the tool: it says what a click will do next.
///
/// Three things about the list are worth knowing before changing it:
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
library;

import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/mechanosynth_scrubber.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Colours the method badge. The method is the **operation's** kind, not
/// something a step types — see `doc/design_mechanosynth_tools.md`.
///
/// A build's `method` values are a small vocabulary
/// ("probe", "relax", …) the user reads down a long list, so they get a stable
/// colour rather than a legend: the same method is the same colour in every
/// project, and an unnamed method has none.
Color? methodColor(String method, ColorScheme scheme) {
  if (method.isEmpty) return null;
  const palette = <Color>[
    Color(0xFF7E9CD8),
    Color(0xFF98BB6C),
    Color(0xFFE6C384),
    Color(0xFFD27E99),
    Color(0xFF7AA89F),
    Color(0xFFC4746E),
  ];
  var hash = 0;
  for (final unit in method.codeUnits) {
    hash = (hash * 31 + unit) & 0x7fffffff;
  }
  return palette[hash % palette.length];
}

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
  // Palette: what the wired library contains
  // ==========================================================================
  //
  // A **reference list**, not a tool. Clicking a name used to arm the
  // operation, so that the next viewport click placed it; that mode is gone
  // (see `mechanosynth_edit_ops.rs::commit_candidate`) because the library
  // splits one reaction into one operation per host environment, which makes
  // "the same operation again" the wrong default at the next site. Placement is
  // atom-first: click an atom, and the list answers with what fits *there*.

  Widget _buildPalette(BuildContext context, APIMechanosynthEditData data) {
    if (!widget.opsConnected || data.opNames.isEmpty) {
      return const SizedBox.shrink();
    }
    final scheme = Theme.of(context).colorScheme;
    final needle = _paletteFilter.trim().toLowerCase();
    final matches = data.opNames
        .where((name) => name.toLowerCase().contains(needle))
        .toList();

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
                Text('Operations (${data.opNames.length})',
                    style: Theme.of(context).textTheme.bodySmall),
              ],
            ),
          ),
        ),
        if (_paletteOpen) ...[
          StringInput(
            label: 'Filter',
            value: _paletteFilter,
            onChanged: (text) => setState(() => _paletteFilter = text),
          ),
          const SizedBox(height: 4),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 160),
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final name in matches)
                    Padding(
                      key: Key('mechanosynth_edit_palette_$name'),
                      padding: const EdgeInsets.symmetric(
                          horizontal: 4.0, vertical: 3.0),
                      child: Text(name,
                          style:
                              TextStyle(fontSize: 12, color: scheme.onSurface)),
                    ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 8),
        ],
      ],
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
        ],
      ),
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
    final color = methodColor(step.method, scheme);

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
          MechanosynthChapterList(
            chapters: data.chapters,
            applied: _previewCursor ?? data.applied,
            onJump: (step) =>
                widget.model.setMechanosynthEditCursor(widget.nodeId, step),
          ),
          _buildStepList(context, data),
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
