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
/// fetched here, in `build`, and memoised on `seq`. A diff is computed on
/// demand Rust-side (D5), so re-fetching it on every rebuild would re-run
/// `similar` over two whole snapshots for nothing.
///
/// **One diff view, not two.** D4 specified a *By node* mode alongside the
/// literal *Text* one and made it the default; in use the per-node blocks
/// turned out to say less than the plain unified diff, so the panel shows
/// *Text* only. `diff_by_node` and the `by_node` flag on `ai_history_diff`
/// stay in the kernel — the engine is tested and an export could still want
/// it — but nothing in the UI reaches them.
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
/// **The timeline carries two kinds of row** (Phase 5). Edits come from
/// `model.aiHistory`; every other CLI request — `query`, `screenshot`,
/// `networks/*`, `load`, `save` — comes from `model.aiActivity`, and the two
/// are merged here by `seq`, which both kernel-side rings draw from. An
/// activity row is a marker, not an entry: it is dim, it is not selectable, and
/// there is no detail pane behind it. Its job is to say what the AI was
/// *looking at* around an edit, which is exactly the context a bare list of
/// edits cannot give. The `⇄` toolbar button hides them when the edits are all
/// one wants to see.
///
/// **PlatformInt64 gotcha**, as in `console_panel.dart`: `timestampMs` is
/// FRB's `PlatformInt64`, `int` on native and `BigInt` on web. Desktop is this
/// project's target and the code below uses `int` directly.
library;

import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/ai_history_api.dart';
import 'package:provider/provider.dart';
import 'ai_history_export.dart';
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
  static const List<String> _tabs = [
    'Diff',
    'Network',
    'Request',
    'Result',
    'Layout',
  ];

  late final TabController _tabController =
      TabController(length: _tabs.length, vsync: this);

  /// Whether the non-edit CLI requests share the list with the edits.
  ///
  /// On by default — showing what the AI looked at is the whole point of
  /// recording it — but a session that queried forty times between two edits is
  /// easier to read with them folded away, so it is one click.
  bool _showActivity = true;

  // Memoised payloads for the selected row. An entry is immutable once
  // pushed, so the sequence number is the whole key.
  BigInt? _payloadSeq;
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
      _detail = null;
      _diff = null;
      return;
    }
    if (seq != _payloadSeq) {
      _payloadSeq = seq;
      _detail = aiHistoryDetail(seq: seq);
      _diff = aiHistoryDiff(seq: seq, byNode: false);
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
                      child: _EntryList(
                        model: model,
                        showActivity: _showActivity,
                      ),
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
    // Export and Clear act on the whole log, so a session that has only
    // looked at the network — no edits yet, a dozen requests — is not empty.
    final isEmpty = model.aiHistory.isEmpty && model.aiActivity.isEmpty;
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
            'edit${model.aiHistory.length == 1 ? "" : "s"}'
            '${model.aiActivity.isEmpty ? "" : ", "
                "${model.aiActivity.length} request"
                "${model.aiActivity.length == 1 ? "" : "s"}"}',
            style: const TextStyle(color: Colors.white38, fontSize: 11),
          ),
          const SizedBox(width: 8),
          InkWell(
            key: const Key('ai_history_activity_toggle'),
            onTap: () => setState(() => _showActivity = !_showActivity),
            child: Tooltip(
              message: _showActivity
                  ? 'Hide the non-edit CLI requests'
                  : 'Show the non-edit CLI requests (query, screenshot, '
                      'networks, load, save)',
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                child: Icon(
                  Icons.swap_horiz,
                  size: 16,
                  color: _showActivity ? _accent : Colors.white38,
                ),
              ),
            ),
          ),
          const Spacer(),
          const _SessionLabelField(),
          const SizedBox(width: 8),
          // Export is how a session leaves the process (D10): the log is
          // memory-only, so an unexported session is gone when the application
          // closes. Both forms sit in one menu rather than two buttons because
          // the choice is occasional and the header is narrow.
          PopupMenuButton<AiHistoryExportFormat>(
            key: const Key('ai_history_export_button'),
            enabled: !isEmpty,
            tooltip: isEmpty ? 'Nothing to export yet' : 'Export the session',
            padding: EdgeInsets.zero,
            position: PopupMenuPosition.under,
            onSelected: (format) => exportAiHistory(context, format),
            itemBuilder: (context) => const [
              PopupMenuItem(
                value: AiHistoryExportFormat.json,
                height: 32,
                child: Text('Export as JSON…', style: TextStyle(fontSize: 12)),
              ),
              PopupMenuItem(
                value: AiHistoryExportFormat.markdown,
                height: 32,
                child:
                    Text('Export as Markdown…', style: TextStyle(fontSize: 12)),
              ),
            ],
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              child: Icon(
                Icons.file_download_outlined,
                size: 16,
                color: isEmpty ? Colors.white24 : Colors.white70,
              ),
            ),
          ),
          // Clearing is unrecoverable — the log is persisted nowhere and undo
          // does not touch it (D6) — so it asks first, unlike the Console
          // panel's Clear, whose entries the next evaluation reproduces.
          InkWell(
            key: const Key('ai_history_clear_button'),
            onTap: isEmpty ? null : () => _confirmClear(context, model),
            child: Tooltip(
              message: isEmpty ? 'Nothing to clear' : 'Clear the AI history',
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                child: Icon(
                  Icons.delete_outline,
                  size: 16,
                  color: isEmpty ? Colors.white24 : Colors.white70,
                ),
              ),
            ),
          ),
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

  Future<void> _confirmClear(
    BuildContext context,
    StructureDesignerModel model,
  ) async {
    final count = model.aiHistory.length;
    final requests = model.aiActivity.length;
    final confirmed = await showDraggableAlertDialog<bool>(
      context: context,
      title: const Text('Clear AI history?'),
      content: Text(
        'Discards $count recorded edit${count == 1 ? "" : "s"} and $requests '
        'other CLI request${requests == 1 ? "" : "s"}. The log is kept in '
        'memory only and undo does not restore it, so export first if you want '
        'to keep the session.',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancel'),
        ),
        TextButton(
          key: const Key('ai_history_clear_confirm_button'),
          onPressed: () => Navigator.of(context).pop(true),
          child: const Text('Clear'),
        ),
      ],
    );
    if (confirmed != true) return;
    model.clearAiHistory();
    // The selection went with the entries; drop the memoised payloads so the
    // detail pane stops rendering an entry that no longer exists.
    if (!mounted) return;
    setState(() {
      _payloadSeq = null;
      _detail = null;
      _diff = null;
    });
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
                onExpand: () => _showDiffDialog(detail),
              ),
              _NetworkTab(detail: detail),
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
            // Five scrollable tabs at 11pt; sized to fit them without
            // scrolling, since a tab strip that scrolls hides tabs.
            width: 380,
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
          // Who submitted this edit, when the CLI said so (Phase 5). Shown only
          // when present: the manual session label covers the case where it is
          // not, and an empty chip would just be noise.
          if (detail.clientLabel.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: Tooltip(
                message: 'Submitted by a client identifying itself as '
                    '"${detail.clientLabel}" (X-Client-Label)',
                child: Text(
                  detail.clientLabel,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: _monoStyle.copyWith(color: _accent),
                ),
              ),
            ),
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
        final diff = aiHistoryDiff(seq: detail.seq, byNode: false);
        return Dialog(
          backgroundColor: _panelBackground,
          child: SizedBox(
            width: size.width * 0.8,
            height: size.height * 0.8,
            child: Column(
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
                          padding:
                              EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                          child: Icon(Icons.close,
                              size: 16, color: Colors.white70),
                        ),
                      ),
                    ],
                  ),
                ),
                if (!detail.beforeComplete || !detail.afterComplete)
                  const _IncompleteSnapshotBanner(),
                Expanded(child: _DiffTab(diff: diff)),
              ],
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
  const _EntryList({required this.model, required this.showActivity});

  final StructureDesignerModel model;

  /// Whether the non-edit CLI requests share the list (Phase 5).
  final bool showActivity;

  @override
  Widget build(BuildContext context) {
    // `aiHistory` and `aiActivity` both arrive oldest-first; the panel reads
    // newest-first. Merging on `seq` is exact because the kernel's two rings
    // share one counter — a timestamp merge would tie on the sub-millisecond
    // gaps a scripted session routinely produces.
    final edits = model.aiHistory.reversed.toList();
    final rows = <Object>[
      ...edits,
      if (showActivity) ...model.aiActivity,
    ]..sort((a, b) => _seqOf(b).compareTo(_seqOf(a)));

    if (rows.isEmpty) {
      return const _Placeholder('No entries yet.');
    }
    return Scrollbar(
      child: ListView.builder(
        padding: const EdgeInsets.symmetric(vertical: 2),
        itemCount: rows.length,
        itemBuilder: (context, index) {
          final row = rows[index];
          if (row is APIAiActivitySummary) {
            return _ActivityRow(entry: row);
          }
          final entry = row as APIAiEditSummary;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _EntryRow(
                entry: entry,
                selected: entry.seq == model.selectedAiHistorySeq,
                onTap: () => model.selectAiHistoryEntry(entry.seq),
              ),
              // The marker belongs in the gap the divergence happened in, which
              // is between two *edits* — an activity row that happens to sort
              // in between is not what changed the network, and the comparison
              // that set the flag never saw it.
              if (entry.diverged)
                _DivergenceMarker(
                  entry: entry,
                  previousSeq: _previousSeqForNetwork(edits, entry),
                ),
            ],
          );
        },
      ),
    );
  }

  static BigInt _seqOf(Object row) =>
      row is APIAiActivitySummary ? row.seq : (row as APIAiEditSummary).seq;

  /// The seq of the previous entry *for the same network* — the one the
  /// divergence was measured against, and the one "edit #N undone" names.
  BigInt? _previousSeqForNetwork(
    List<APIAiEditSummary> edits,
    APIAiEditSummary entry,
  ) {
    final index = edits.indexOf(entry);
    if (index < 0) return null;
    for (var i = index + 1; i < edits.length; i++) {
      if (edits[i].networkName == entry.networkName) {
        return edits[i].seq;
      }
    }
    return null;
  }
}

/// One non-edit CLI request (Phase 5) — `GET /query`, `POST /networks/rename`.
///
/// Dimmer and quieter than an edit row on purpose: these are context, not the
/// subject. Not selectable either, because there is nothing behind one — the
/// whole record is the line you are reading.
class _ActivityRow extends StatelessWidget {
  const _ActivityRow({required this.entry});

  final APIAiActivitySummary entry;

  @override
  Widget build(BuildContext context) {
    final color = entry.ok ? Colors.white38 : _bad;
    return Tooltip(
      message: [
        '${entry.requestLine} \u2192 ${entry.status} (${entry.durationMs} ms)',
        if (entry.detail.isNotEmpty) entry.detail,
        if (entry.clientLabel.isNotEmpty) 'client: ${entry.clientLabel}',
      ].join('\n'),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        child: Row(
          children: [
            SizedBox(
              width: 34,
              child: Text('#${entry.seq}',
                  style: _monoStyle.copyWith(color: Colors.white24)),
            ),
            Text(_formatTimestamp(entry.timestampMs),
                style: _monoStyle.copyWith(color: Colors.white24)),
            const SizedBox(width: 6),
            Text(entry.ok ? '\u00b7' : '\u2717',
                style: TextStyle(fontSize: 12, color: color)),
            const SizedBox(width: 6),
            Expanded(
              child: Text(
                entry.detail.isEmpty
                    ? entry.requestLine
                    : '${entry.requestLine}  ${entry.detail}',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: _monoStyle.copyWith(color: color),
              ),
            ),
            const SizedBox(width: 6),
            Text('${entry.durationMs} ms',
                style: _monoStyle.copyWith(color: Colors.white24)),
          ],
        ),
      ),
    );
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
/// The free-text session label stamped into exports (D13).
///
/// The application cannot know by itself which model is driving the CLI, so it
/// is asked. One line of UI, and it is what turns a pile of exported sessions
/// into a comparable set.
///
/// Since Phase 5 the CLI *can* identify itself — `atomcad-cli --label` sends an
/// `X-Client-Label` header — and when it has, that label becomes this field's
/// placeholder: there is then nothing left to type, and the export carries
/// every distinct label regardless of what is in here.
///
/// Stateful, with its own controller, because the model deliberately does not
/// notify on a label change: a rebuild per keystroke would fight the caret, and
/// the field already shows the value it just sent.
class _SessionLabelField extends StatefulWidget {
  const _SessionLabelField();

  @override
  State<_SessionLabelField> createState() => _SessionLabelFieldState();
}

class _SessionLabelFieldState extends State<_SessionLabelField> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    // Seeded from the kernel rather than from the model's default: a label may
    // have been set before this panel was first opened.
    final model = context.read<StructureDesignerModel>();
    model.initAiHistorySessionLabel();
    _controller = TextEditingController(text: model.aiHistorySessionLabel);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final detected =
        context.read<StructureDesignerModel>().aiHistoryClientLabel;
    return Tooltip(
      message: detected == null
          ? 'Session label — stamped into exports, e.g. "Opus 5 / skill v3"'
          : 'Session label — stamped into exports. The CLI is identifying '
              'itself as "$detected", which exports carry anyway.',
      child: SizedBox(
        width: 180,
        height: 20,
        child: TextField(
          key: const Key('ai_history_session_label_field'),
          controller: _controller,
          style: const TextStyle(color: Colors.white70, fontSize: 11),
          cursorColor: _accent,
          decoration: InputDecoration(
            isDense: true,
            contentPadding:
                const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
            hintText: detected ?? 'Session label',
            hintStyle: const TextStyle(color: Colors.white24, fontSize: 11),
            filled: true,
            fillColor: _stripBackground,
            border: const OutlineInputBorder(
              borderSide: BorderSide(color: Colors.black54),
            ),
            enabledBorder: const OutlineInputBorder(
              borderSide: BorderSide(color: Colors.black54),
            ),
            focusedBorder: const OutlineInputBorder(
              borderSide: BorderSide(color: _accent),
            ),
          ),
          onChanged: (value) => context
              .read<StructureDesignerModel>()
              .setAiHistorySessionLabel(value),
        ),
      ),
    );
  }
}

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

/// The literal unified diff of the two snapshots.
///
/// D4 also specified a *By node* view and made it the default. It is gone from
/// the UI: per-node blocks read as a rearrangement of the same lines the text
/// diff already shows, and the mode switch cost more attention than the second
/// view returned. The kernel still computes it — `ai_history_diff` keeps its
/// `by_node` flag — so this is a UI decision, not a deletion.
class _DiffTab extends StatelessWidget {
  const _DiffTab({required this.diff, this.onExpand});

  final APIAiDiff? diff;
  final VoidCallback? onExpand;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // Only the docked pane carries the strip; in the expanded dialog there
        // is nothing left to put in it.
        if (onExpand != null) _buildExpandStrip(),
        Expanded(child: _buildBody()),
      ],
    );
  }

  Widget _buildExpandStrip() {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      color: _stripBackground,
      child: Row(
        children: [
          const Spacer(),
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
                _DiffHunkView(hunk: current.hunks[index]),
          ),
        ),
      ),
    );
  }
}

class _DiffHunkView extends StatelessWidget {
  const _DiffHunkView({required this.hunk});

  final APIDiffHunk hunk;

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

  String _title() => hunk.nodePath.isNotEmpty ? hunk.nodePath : '(hunk)';
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

/// The whole network as it stood once this edit landed — `after_text`, the
/// same AI text-format snapshot D2 records and the diff is computed from.
///
/// The diff answers "what changed"; often the question is the other one, "what
/// did the network actually look like at that point", and reconstructing it by
/// replaying diffs in one's head is exactly the work the log exists to save.
/// It is also the state the *next* entry is compared against for divergence
/// (D7), so a surprising marker downstream is read here.
///
/// Unless flagged incomplete, the text is valid `edit --replace` input: select
/// it, and any moment of the session can be restored.
class _NetworkTab extends StatelessWidget {
  const _NetworkTab({required this.detail});

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
            'after edit #${detail.seq} · ${_networkLabel(detail.networkName)}'
            '${detail.afterComplete ? "" : " · truncated"}',
            style: TextStyle(
              fontSize: 10,
              color: detail.afterComplete ? Colors.white38 : _warn,
            ),
          ),
        ),
        Expanded(
          child: detail.afterText.isEmpty
              // The three rejection paths of D1 — no active network, network
              // not found, write-locked — never reached a network to
              // serialize, so there is no snapshot rather than an empty one.
              ? const _Placeholder(
                  'No snapshot for this entry: the edit was rejected before it '
                  'reached a network.',
                )
              : Scrollbar(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.all(8),
                    child: SelectableText(detail.afterText, style: _monoStyle),
                  ),
                ),
        ),
      ],
    );
  }
}

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
            'Incremental layout fits an edit’s own new nodes in and repairs '
            'what they broke, in every scope including bodies — it does not '
            'reflow the network. An empty moved list is the normal, good '
            'result. Only Edit ▸ Auto-Layout Network rearranges everything.',
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
