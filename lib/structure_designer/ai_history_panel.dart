/// The AI History panel: a docked-bottom master–detail view of the AI edit log
/// (`doc/design_ai_edit_history.md`, Phase 3).
///
/// Built on the `console_panel.dart` / `profiler_panel.dart` template — state
/// lives on `StructureDesignerModel` (`aiHistoryPanelVisible`, `aiHistory`,
/// `selectedAiHistorySeq`, `unreadAiEditCount`), a *View* menu entry toggles
/// it, and it collapses to zero height when hidden.
///
/// **The list is pushed, the payloads are pulled** (D12). `refreshFromKernel`
/// keeps `model.aiHistory` current behind an `ai_history_version()` compare and
/// only while the panel is open; the selected entry's detail and diff are
/// fetched here, in `build`, and memoised on `(seq, byNode)`. A diff is
/// computed on demand Rust-side (D5), so re-fetching it on every rebuild would
/// re-run `similar` over two whole snapshots for nothing.
///
/// **Two verdicts, not one** (D8). A row's primary glyph is `applied` — the
/// editor's own verdict, "did this edit land" — and `success`, the
/// *whole-network* validity afterwards, is a secondary badge. The combination
/// `applied && !success` is ordinary rather than exceptional: creating a
/// higher-order node before filling its body produces it every time, and a
/// single glyph would report that clean edit as a failure.
///
/// **`LayoutPath.none` does not mean "nothing moved"** (D9). `layout_network`
/// never descends into a zone body, so a body node is re-placed at creation
/// time rather than reflowed. The Layout tab therefore groups moved nodes by
/// scope and says so in a footnote, instead of letting `none` read as "nothing
/// was disturbed".
///
/// **PlatformInt64 gotcha**, as in `console_panel.dart`: `timestampMs` is
/// FRB's `PlatformInt64`, `int` on native and `BigInt` on web. Desktop is this
/// project's target and the code below uses `int` directly.
library;

import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/ai_history_api.dart';
import 'package:provider/provider.dart';
import 'structure_designer_model.dart';

// Shared with the Console and Profiler panels so the three read as one dock.
const Color _panelBackground = Color(0xFF1E1E1E);
const Color _headerBackground = Color(0xFF2A2A2A);
const Color _stripBackground = Color(0xFF252525);
const Color _selectedRowBackground = Color(0xFF33405A);
const Color _accent = Color(0xFF6CA0DC);
const Color _good = Color(0xFF7BC67B);
const Color _warn = Color(0xFFD8A05A);
const Color _bad = Color(0xFFD87A7A);

const TextStyle _monoStyle = TextStyle(
  fontFamily: 'monospace',
  fontSize: 11,
  color: Colors.white70,
  height: 1.35,
);

/// Bottom-docked panel showing every AI edit of this session.
class AiHistoryPanel extends StatefulWidget {
  const AiHistoryPanel({super.key});

  @override
  State<AiHistoryPanel> createState() => _AiHistoryPanelState();
}

class _AiHistoryPanelState extends State<AiHistoryPanel>
    with SingleTickerProviderStateMixin {
  static const double _panelHeight = 280;
  static const double _listWidth = 360;
  static const List<String> _tabs = ['Diff', 'Request', 'Result', 'Layout'];

  late final TabController _tabController =
      TabController(length: _tabs.length, vsync: this);

  /// Diff mode — *By node* (D4's default) over the literal *Text* diff.
  bool _byNode = true;

  // Memoised payloads for the selected row. An entry is immutable once
  // pushed, so the key needs nothing beyond the sequence number and the mode.
  BigInt? _payloadSeq;
  bool? _payloadByNode;
  APIAiEditDetail? _detail;
  APIAiDiff? _diff;

  @override
  void dispose() {
    _tabController.dispose();
    super.dispose();
  }

  /// Pulls the selected entry's detail and diff when the key changed. Called
  /// from `build`, which only runs while the panel is visible.
  void _syncPayloads(BigInt? seq) {
    if (seq == null) {
      _payloadSeq = null;
      _payloadByNode = null;
      _detail = null;
      _diff = null;
      return;
    }
    if (seq != _payloadSeq) {
      _payloadSeq = seq;
      _detail = aiHistoryDetail(seq: seq);
      _payloadByNode = null;
    }
    if (_payloadByNode != _byNode) {
      _payloadByNode = _byNode;
      _diff = aiHistoryDiff(seq: seq, byNode: _byNode);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Consumer<StructureDesignerModel>(
      builder: (context, model, _) {
        if (!model.aiHistoryPanelVisible) {
          return const SizedBox.shrink();
        }
        _syncPayloads(model.selectedAiHistorySeq);
        // Capped against the window: this is the third fixed-height panel in
        // the same Column, and three open at once on a short screen would
        // squeeze the main content area to nothing and overflow.
        final maxHeight = MediaQuery.of(context).size.height * 0.35;
        return Container(
          height: _panelHeight < maxHeight ? _panelHeight : maxHeight,
          decoration: const BoxDecoration(
            color: _panelBackground,
            border: Border(top: BorderSide(color: Colors.black54, width: 1)),
          ),
          child: Column(
            children: [
              _buildHeader(model),
              Expanded(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    SizedBox(
                      width: _listWidth,
                      child: _EntryList(model: model),
                    ),
                    const VerticalDivider(
                        width: 1, thickness: 1, color: Colors.black54),
                    Expanded(child: _buildDetail()),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  Widget _buildHeader(StructureDesignerModel model) {
    return Container(
      height: 28,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      color: _headerBackground,
      child: Row(
        children: [
          const Text(
            'AI History',
            style: TextStyle(
              color: Colors.white70,
              fontWeight: FontWeight.w600,
              fontSize: 12,
            ),
          ),
          const SizedBox(width: 12),
          Text(
            '${model.aiHistory.length} '
            'edit${model.aiHistory.length == 1 ? "" : "s"}',
            style: const TextStyle(color: Colors.white38, fontSize: 11),
          ),
          const Spacer(),
          InkWell(
            key: const Key('ai_history_close_button'),
            onTap: () => model.toggleAiHistoryPanel(),
            child: const Tooltip(
              message: 'Hide AI history',
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                child: Icon(Icons.close, size: 16, color: Colors.white70),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildDetail() {
    final detail = _detail;
    if (detail == null) {
      return const _Placeholder(
        'No AI edits recorded yet. They arrive from `atomcad-cli edit` and '
        'from the assistant’s HTTP endpoint.',
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _buildDetailHeader(detail),
        if (!detail.beforeComplete || !detail.afterComplete)
          const _IncompleteSnapshotBanner(),
        Expanded(
          child: TabBarView(
            controller: _tabController,
            children: [
              _DiffTab(
                diff: _diff,
                byNode: _byNode,
                onModeChanged: (byNode) => setState(() => _byNode = byNode),
                onExpand: () => _showDiffDialog(detail),
              ),
              _RequestTab(detail: detail),
              _ResultTab(detail: detail),
              _LayoutTab(detail: detail),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildDetailHeader(APIAiEditDetail detail) {
    return Container(
      height: 30,
      padding: const EdgeInsets.only(left: 8),
      color: _headerBackground,
      child: Row(
        children: [
          Text(
            '#${detail.seq}',
            style: const TextStyle(
              color: Colors.white70,
              fontWeight: FontWeight.w600,
              fontSize: 12,
            ),
          ),
          const SizedBox(width: 12),
          SizedBox(
            width: 300,
            child: TabBar(
              controller: _tabController,
              isScrollable: true,
              tabAlignment: TabAlignment.start,
              labelColor: Colors.white,
              unselectedLabelColor: Colors.white38,
              labelStyle: const TextStyle(fontSize: 11),
              indicatorColor: _accent,
              dividerColor: Colors.transparent,
              tabs: [for (final name in _tabs) Tab(height: 30, text: name)],
            ),
          ),
          const Spacer(),
        ],
      ),
    );
  }

  /// The `⤢` expansion of D11: height is the scarce dimension in a docked
  /// panel, and a long diff is what one most wants to read in full.
  void _showDiffDialog(APIAiEditDetail detail) {
    final size = MediaQuery.of(context).size;
    showDialog<void>(
      context: context,
      builder: (dialogContext) {
        var byNode = _byNode;
        var diff = aiHistoryDiff(seq: detail.seq, byNode: byNode);
        return Dialog(
          backgroundColor: _panelBackground,
          child: SizedBox(
            width: size.width * 0.8,
            height: size.height * 0.8,
            child: StatefulBuilder(
              builder: (builderContext, setDialogState) {
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Container(
                      height: 30,
                      padding: const EdgeInsets.symmetric(horizontal: 8),
                      color: _headerBackground,
                      child: Row(
                        children: [
                          Text(
                            'Edit #${detail.seq} — '
                            '${_networkLabel(detail.networkName)}',
                            style: const TextStyle(
                              color: Colors.white70,
                              fontWeight: FontWeight.w600,
                              fontSize: 12,
                            ),
                          ),
                          const Spacer(),
                          InkWell(
                            onTap: () => Navigator.of(dialogContext).pop(),
                            child: const Padding(
                              padding: EdgeInsets.symmetric(
                                  horizontal: 6, vertical: 2),
                              child: Icon(Icons.close,
                                  size: 16, color: Colors.white70),
                            ),
                          ),
                        ],
                      ),
                    ),
                    if (!detail.beforeComplete || !detail.afterComplete)
                      const _IncompleteSnapshotBanner(),
                    Expanded(
                      child: _DiffTab(
                        diff: diff,
                        byNode: byNode,
                        onModeChanged: (next) => setDialogState(() {
                          byNode = next;
                          diff = aiHistoryDiff(seq: detail.seq, byNode: byNode);
                        }),
                      ),
                    ),
                  ],
                );
              },
            ),
          ),
        );
      },
    );
  }
}

String _networkLabel(String name) =>
    name.isEmpty ? '(no active network)' : name;

String _formatTimestamp(int epochMillis) {
  final dt = DateTime.fromMillisecondsSinceEpoch(epochMillis);
  final h = dt.hour.toString().padLeft(2, '0');
  final m = dt.minute.toString().padLeft(2, '0');
  final s = dt.second.toString().padLeft(2, '0');
  return '$h:$m:$s';
}

// ============================================================================
// Master pane
// ============================================================================

/// The entry list, newest first, with a divergence marker drawn in the gap the
/// divergence actually happened in — i.e. below a flagged entry, between it and
/// the older one it diverged from (D7).
class _EntryList extends StatelessWidget {
  const _EntryList({required this.model});

  final StructureDesignerModel model;

  @override
  Widget build(BuildContext context) {
    if (model.aiHistory.isEmpty) {
      return const _Placeholder('No entries yet.');
    }
    // `aiHistory` arrives oldest-first; the panel reads newest-first.
    final entries = model.aiHistory.reversed.toList();
    return Scrollbar(
      child: ListView.builder(
        padding: const EdgeInsets.symmetric(vertical: 2),
        itemCount: entries.length,
        itemBuilder: (context, index) {
          final entry = entries[index];
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _EntryRow(
                entry: entry,
                selected: entry.seq == model.selectedAiHistorySeq,
                onTap: () => model.selectAiHistoryEntry(entry.seq),
              ),
              if (entry.diverged)
                _DivergenceMarker(
                  entry: entry,
                  previousSeq: _previousSeqForNetwork(entries, index),
                ),
            ],
          );
        },
      ),
    );
  }

  /// The seq of the previous entry *for the same network* — the one the
  /// divergence was measured against, and the one "edit #N undone" names.
  BigInt? _previousSeqForNetwork(List<APIAiEditSummary> entries, int index) {
    for (var i = index + 1; i < entries.length; i++) {
      if (entries[i].networkName == entries[index].networkName) {
        return entries[i].seq;
      }
    }
    return null;
  }
}

class _EntryRow extends StatelessWidget {
  const _EntryRow({
    required this.entry,
    required this.selected,
    required this.onTap,
  });

  final APIAiEditSummary entry;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    // "Entries whose edit did not apply are tinted" — the row is about the
    // AI's own performance, so `applied` is what dims it, never `success`.
    final textColor = entry.applied ? Colors.white70 : Colors.white38;
    return InkWell(
      onTap: onTap,
      child: Container(
        color: selected ? _selectedRowBackground : Colors.transparent,
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        child: Row(
          children: [
            SizedBox(
              width: 34,
              child: Text('#${entry.seq}',
                  style: _monoStyle.copyWith(color: textColor)),
            ),
            Text(_formatTimestamp(entry.timestampMs),
                style: _monoStyle.copyWith(color: Colors.white38)),
            const SizedBox(width: 6),
            Tooltip(
              message: entry.applied
                  ? 'The submitted statements parsed and were applied'
                  : 'The edit did not apply',
              child: Text(entry.applied ? '✔' : '✖',
                  style: TextStyle(
                      fontSize: 12, color: entry.applied ? _good : _bad)),
            ),
            if (entry.applied && !entry.success) ...[
              const SizedBox(width: 2),
              const Tooltip(
                message: 'The edit landed, but the network does not validate. '
                    'That verdict is network-wide, so this is ordinary '
                    'mid-repair — and expected right after creating a '
                    'higher-order node whose body is still empty.',
                child: Text('⚠', style: TextStyle(fontSize: 12, color: _warn)),
              ),
            ],
            const SizedBox(width: 6),
            Expanded(
              child: Text(
                _networkLabel(entry.networkName),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: _monoStyle.copyWith(color: textColor),
              ),
            ),
            if (entry.replace) ...[
              const _Badge(
                text: 'REPL',
                color: _warn,
                tooltip: '--replace: the whole network was rebuilt from the '
                    'submitted script, not merged into',
              ),
              const SizedBox(width: 4),
            ],
            Text(
              '+${entry.createdCount} ~${entry.updatedCount} '
              '−${entry.deletedCount}',
              style: _monoStyle.copyWith(color: textColor),
            ),
            const SizedBox(width: 6),
            Tooltip(
              message: '${entry.movedCount} node(s) changed position',
              child: Text('⌂${entry.movedCount}',
                  style: _monoStyle.copyWith(
                      color: entry.movedCount > 0
                          ? Colors.white70
                          : Colors.white24)),
            ),
            if (entry.errorCount > 0) ...[
              const SizedBox(width: 4),
              _Badge(
                text: '✖${entry.errorCount}',
                color: _bad,
                tooltip: '${entry.errorCount} error(s) — see the Result tab',
              ),
            ],
            if (entry.warningCount > 0) ...[
              const SizedBox(width: 4),
              _Badge(
                text: '⚠${entry.warningCount}',
                color: _warn,
                tooltip:
                    '${entry.warningCount} warning(s) — see the Result tab',
              ),
            ],
            if (!entry.snapshotsComplete) ...[
              const SizedBox(width: 4),
              const _Badge(
                text: '⌁',
                color: _bad,
                tooltip: 'A snapshot was truncated by a wire cycle; this '
                    'entry’s diff is not trustworthy',
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _Badge extends StatelessWidget {
  const _Badge({required this.text, required this.color, this.tooltip});

  final String text;
  final Color color;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final badge = Container(
      padding: const EdgeInsets.symmetric(horizontal: 3, vertical: 1),
      decoration: BoxDecoration(
        border: Border.all(color: color, width: 1),
        borderRadius: BorderRadius.circular(2),
      ),
      child: Text(text,
          style: TextStyle(
              fontFamily: 'monospace',
              fontSize: 9,
              fontWeight: FontWeight.w600,
              color: color)),
    );
    if (tooltip == null) return badge;
    return Tooltip(message: tooltip!, child: badge);
  }
}

/// The gap marker of D7. It claims only what the one string comparison proves —
/// that the network's text changed between two `edit` calls — and names an undo
/// only when the undo stack said so.
class _DivergenceMarker extends StatelessWidget {
  const _DivergenceMarker({required this.entry, required this.previousSeq});

  final APIAiEditSummary entry;
  final BigInt? previousSeq;

  @override
  Widget build(BuildContext context) {
    final label = entry.divergedByUndo
        ? (previousSeq == null
            ? 'an AI edit was undone'
            : 'edit #$previousSeq undone')
        : 'changed outside the CLI';
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      child: Row(
        children: [
          const Expanded(child: Divider(color: Colors.white24, height: 1)),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6),
            child: Tooltip(
              message: entry.divergedByUndo
                  ? 'This network was in a different state than the previous '
                      'entry left it in, and the undo stack explains it.'
                  : 'This network was in a different state than the previous '
                      'entry left it in — a GUI edit, a file load, or a '
                      'display-policy change.',
              child: Text(label,
                  style: const TextStyle(fontSize: 10, color: Colors.white38)),
            ),
          ),
          const Expanded(child: Divider(color: Colors.white24, height: 1)),
        ],
      ),
    );
  }
}

// ============================================================================
// Detail tabs
// ============================================================================

/// Shown whenever either snapshot was truncated by a wire cycle (D2). It is not
/// decoration: a truncated `after_text` fills the *removed* column with nodes
/// that still exist, so the diff below must not be read as a record of what the
/// edit did.
class _IncompleteSnapshotBanner extends StatelessWidget {
  const _IncompleteSnapshotBanner();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      color: const Color(0xFF3A2A20),
      child: const Text(
        'Incomplete snapshot: the serializer aborted on a wire cycle, so the '
        'text below is truncated and its diff is not trustworthy. It is not '
        'valid `edit --replace` input either.',
        style: TextStyle(fontSize: 10, color: _warn),
      ),
    );
  }
}

/// *By node* (default) or *Text*, per D4.
class _DiffTab extends StatelessWidget {
  const _DiffTab({
    required this.diff,
    required this.byNode,
    required this.onModeChanged,
    this.onExpand,
  });

  final APIAiDiff? diff;
  final bool byNode;
  final ValueChanged<bool> onModeChanged;
  final VoidCallback? onExpand;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _buildModeStrip(),
        Expanded(child: _buildBody()),
      ],
    );
  }

  Widget _buildModeStrip() {
    final unchanged = diff?.unchangedCount ?? 0;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      color: _stripBackground,
      child: Row(
        children: [
          _ModeButton(
            label: 'By node',
            selected: byNode,
            tooltip: 'One block per node, keyed by scoped path (`m1/d`). '
                'Stable against the reordering a topological sort produces.',
            onTap: () => onModeChanged(true),
          ),
          const SizedBox(width: 8),
          _ModeButton(
            label: 'Text',
            selected: !byNode,
            tooltip: 'The literal unified diff of the two snapshots.',
            onTap: () => onModeChanged(false),
          ),
          const SizedBox(width: 12),
          if (byNode && unchanged > 0)
            Text('$unchanged unchanged block(s) collapsed',
                style: const TextStyle(fontSize: 10, color: Colors.white38)),
          const Spacer(),
          if (onExpand != null)
            InkWell(
              key: const Key('ai_history_expand_diff_button'),
              onTap: onExpand,
              child: const Tooltip(
                message: 'Open this diff in a large window',
                child: Padding(
                  padding: EdgeInsets.symmetric(horizontal: 4, vertical: 2),
                  child:
                      Icon(Icons.open_in_full, size: 14, color: Colors.white70),
                ),
              ),
            ),
        ],
      ),
    );
  }

  Widget _buildBody() {
    final current = diff;
    if (current == null) {
      return const _Placeholder('No diff for this entry.');
    }
    if (current.hunks.isEmpty) {
      return const _Placeholder(
        'No difference between the snapshots taken before and after this '
        'edit.',
      );
    }
    return Opacity(
      // Greyed rather than hidden: the diff is still the best available
      // evidence, it just cannot be trusted as complete (D2).
      opacity: current.snapshotsComplete ? 1.0 : 0.5,
      child: SelectionArea(
        child: Scrollbar(
          child: ListView.builder(
            padding: const EdgeInsets.symmetric(vertical: 4),
            itemCount: current.hunks.length,
            itemBuilder: (context, index) =>
                _DiffHunkView(hunk: current.hunks[index], byNode: byNode),
          ),
        ),
      ),
    );
  }
}

class _ModeButton extends StatelessWidget {
  const _ModeButton({
    required this.label,
    required this.selected,
    required this.onTap,
    required this.tooltip,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;
  final String tooltip;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: tooltip,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                  selected
                      ? Icons.radio_button_checked
                      : Icons.radio_button_unchecked,
                  size: 12,
                  color: selected ? _accent : Colors.white38),
              const SizedBox(width: 4),
              Text(label,
                  style: TextStyle(
                      fontSize: 11,
                      color: selected ? Colors.white : Colors.white38)),
            ],
          ),
        ),
      ),
    );
  }
}

class _DiffHunkView extends StatelessWidget {
  const _DiffHunkView({required this.hunk, required this.byNode});

  final APIDiffHunk hunk;
  final bool byNode;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
            color: _stripBackground,
            child: Row(
              children: [
                Text(_kindGlyph(hunk.kind),
                    style: TextStyle(
                        fontFamily: 'monospace',
                        fontSize: 11,
                        fontWeight: FontWeight.w600,
                        color: _kindColor(hunk.kind))),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    _title(),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                        fontFamily: 'monospace',
                        fontSize: 11,
                        color: _kindColor(hunk.kind)),
                  ),
                ),
              ],
            ),
          ),
          for (final line in hunk.lines)
            Container(
              width: double.infinity,
              color: _lineBackground(line.tag),
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Text('${_lineGlyph(line.tag)} ${line.text}',
                  style: _monoStyle.copyWith(color: _lineColor(line.tag))),
            ),
        ],
      ),
    );
  }

  String _title() {
    if (hunk.nodePath.isNotEmpty) return hunk.nodePath;
    // *By node* files the header, `description`, `summary` and `output`
    // statements — everything that is not a node block — under an empty path.
    return byNode ? '(network header / output)' : '(hunk)';
  }
}

String _kindGlyph(APIDiffHunkKind kind) => switch (kind) {
      APIDiffHunkKind.added => '+',
      APIDiffHunkKind.removed => '−',
      APIDiffHunkKind.changed => '~',
    };

Color _kindColor(APIDiffHunkKind kind) => switch (kind) {
      APIDiffHunkKind.added => _good,
      APIDiffHunkKind.removed => _bad,
      APIDiffHunkKind.changed => _accent,
    };

String _lineGlyph(APIDiffLineTag tag) => switch (tag) {
      APIDiffLineTag.same => ' ',
      APIDiffLineTag.add => '+',
      APIDiffLineTag.remove => '−',
    };

Color _lineColor(APIDiffLineTag tag) => switch (tag) {
      APIDiffLineTag.same => Colors.white54,
      APIDiffLineTag.add => _good,
      APIDiffLineTag.remove => _bad,
    };

Color _lineBackground(APIDiffLineTag tag) => switch (tag) {
      APIDiffLineTag.same => Colors.transparent,
      APIDiffLineTag.add => const Color(0xFF1E2A1E),
      APIDiffLineTag.remove => const Color(0xFF2A1E1E),
    };

/// Exactly what the AI submitted (D8). Verbatim and selectable: this is the raw
/// material for refining the skill document and the text format.
class _RequestTab extends StatelessWidget {
  const _RequestTab({required this.detail});

  final APIAiEditDetail detail;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
          color: _stripBackground,
          child: Text(
            '${detail.replace ? "edit --replace" : "edit"} · '
            '${_networkLabel(detail.networkName)} · '
            '${_formatTimestamp(detail.timestampMs)}',
            style: const TextStyle(fontSize: 10, color: Colors.white38),
          ),
        ),
        Expanded(
          child: detail.code.isEmpty
              ? const _Placeholder('The request carried no script.')
              : Scrollbar(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.all(8),
                    child: SelectableText(detail.code, style: _monoStyle),
                  ),
                ),
        ),
      ],
    );
  }
}

/// The whole `EditResult` (D8) — including `description_set`, `summary_set` and
/// `output_set`, which are the only place a re-pointed network output shows up
/// as an intent rather than as a diff line.
class _ResultTab extends StatelessWidget {
  const _ResultTab({required this.detail});

  final APIAiEditDetail detail;

  @override
  Widget build(BuildContext context) {
    return SelectionArea(
      child: Scrollbar(
        child: ListView(
          padding: const EdgeInsets.symmetric(vertical: 6, horizontal: 8),
          children: [
            _verdicts(),
            if (detail.errors.isNotEmpty)
              _section('Errors', detail.errors, color: _bad),
            if (detail.warnings.isNotEmpty)
              _section('Warnings', detail.warnings, color: _warn),
            if (detail.nodesCreated.isNotEmpty)
              _section('Created', detail.nodesCreated),
            if (detail.nodesUpdated.isNotEmpty)
              _section('Updated', detail.nodesUpdated),
            if (detail.nodesDeleted.isNotEmpty)
              _section('Deleted', detail.nodesDeleted),
            if (detail.connectionsMade.isNotEmpty)
              _section('Connections', detail.connectionsMade),
            if (detail.descriptionSet != null)
              _section('Description set', [detail.descriptionSet!]),
            if (detail.summarySet != null)
              _section('Summary set', [detail.summarySet!]),
            if (detail.outputSet != null)
              _section('Output set', [detail.outputSet!]),
          ],
        ),
      ),
    );
  }

  Widget _verdicts() {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Wrap(
        spacing: 12,
        runSpacing: 4,
        children: [
          Text(
            detail.applied ? '✔ applied' : '✖ not applied',
            style:
                TextStyle(fontSize: 11, color: detail.applied ? _good : _bad),
          ),
          Text(
            detail.success ? '✔ network validates' : '⚠ network invalid',
            style:
                TextStyle(fontSize: 11, color: detail.success ? _good : _warn),
          ),
          if (detail.replace)
            const Text('--replace',
                style: TextStyle(fontSize: 11, color: _warn)),
        ],
      ),
    );
  }

  Widget _section(String title, List<String> lines, {Color? color}) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title,
              style: const TextStyle(
                  fontSize: 10,
                  color: Colors.white38,
                  fontWeight: FontWeight.w600)),
          const SizedBox(height: 2),
          for (final line in lines)
            Padding(
              padding: const EdgeInsets.only(top: 1),
              child: Text(line,
                  style: _monoStyle.copyWith(color: color ?? Colors.white70)),
            ),
        ],
      ),
    );
  }
}

/// What layout did to the drawing (D9) — the one thing neither the text diff
/// nor the `EditResult` can say.
class _LayoutTab extends StatelessWidget {
  const _LayoutTab({required this.detail});

  final APIAiEditDetail detail;

  @override
  Widget build(BuildContext context) {
    final layout = detail.layout;
    // Grouped by scope: a root-scope move came from the layout pass, a body
    // move from creation-time placement, and the two must not be read as the
    // same event.
    final groups = <String, List<APIMovedNode>>{};
    for (final moved in layout.moved) {
      final cut = moved.path.lastIndexOf('/');
      final scope = cut < 0 ? '' : moved.path.substring(0, cut);
      groups.putIfAbsent(scope, () => []).add(moved);
    }
    final scopes = groups.keys.toList()..sort();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
          color: _stripBackground,
          child: Text(
            '${_pathLabel(layout.path)} · '
            '${layout.moved.length} of ${layout.nodeCount} nodes moved · '
            'max displacement ${layout.maxDisplacement.toStringAsFixed(1)}'
            '${_deltaLabel(layout.delta)}',
            style: const TextStyle(fontSize: 10, color: Colors.white38),
          ),
        ),
        Expanded(
          child: layout.moved.isEmpty
              ? const _Placeholder('No node changed position.')
              : SelectionArea(
                  child: Scrollbar(
                    child: ListView(
                      padding: const EdgeInsets.symmetric(vertical: 4),
                      children: [
                        for (final scope in scopes) ...[
                          Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 8, vertical: 2),
                            color: _stripBackground,
                            child: Text(
                              scope.isEmpty ? 'root scope' : '$scope (body)',
                              style: const TextStyle(
                                  fontSize: 10, color: Colors.white54),
                            ),
                          ),
                          for (final moved in groups[scope]!)
                            _MovedNodeRow(moved: moved),
                        ],
                      ],
                    ),
                  ),
                ),
        ),
        Container(
          width: double.infinity,
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
          color: _stripBackground,
          child: const Text(
            'The layout path describes the root scope only: body nodes are '
            'never reflowed, so a move inside a body comes from creation-time '
            'placement rather than from the layout pass. “No layout pass” with '
            'a non-empty list is therefore correct, not a contradiction.',
            style: TextStyle(fontSize: 10, color: Colors.white38),
          ),
        ),
      ],
    );
  }
}

class _MovedNodeRow extends StatelessWidget {
  const _MovedNodeRow({required this.moved});

  final APIMovedNode moved;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 1),
      child: Row(
        children: [
          Expanded(
            flex: 4,
            child: Text(moved.path,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: _monoStyle),
          ),
          Expanded(
            flex: 5,
            child: Text(
              '${_point(moved.before.x, moved.before.y)} → '
              '${_point(moved.after.x, moved.after.y)}',
              style: _monoStyle.copyWith(color: Colors.white54),
            ),
          ),
          Expanded(
            flex: 2,
            child: Text(
              moved.displacement.toStringAsFixed(1),
              textAlign: TextAlign.right,
              style: _monoStyle,
            ),
          ),
        ],
      ),
    );
  }
}

String _point(double x, double y) =>
    '(${x.toStringAsFixed(0)}, ${y.toStringAsFixed(0)})';

String _pathLabel(APILayoutPath path) => switch (path) {
      APILayoutPath.none => 'No layout pass',
      APILayoutPath.fullReflow => 'Full reflow',
      APILayoutPath.incremental => 'Incremental layout',
    };

/// Empty until `doc/design_incremental_layout.md` Phase 1 lands, which is what
/// turns "7 nodes moved" into "7 moved, of which 2 were in the delta".
String _deltaLabel(APIDeltaCounts? delta) {
  if (delta == null) return '';
  return ' · delta +${delta.nodesAdded} ~${delta.nodesModified} '
      '−${delta.nodesRemoved}, wires +${delta.wiresAdded} '
      '−${delta.wiresRemoved}';
}

class _Placeholder extends StatelessWidget {
  const _Placeholder(this.message);

  final String message;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Text(message,
            textAlign: TextAlign.center,
            style: const TextStyle(color: Colors.white38, fontSize: 12)),
      ),
    );
  }
}
