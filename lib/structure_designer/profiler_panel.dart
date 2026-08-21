/// The Profiler panel: a docked-bottom panel that breaks a refresh down by
/// phase and — when the opt-in per-node profiler is armed — by node type and
/// by node (`doc/design_eval_profiling.md` D8b, Phase 2).
///
/// Built on the `console_panel.dart` template: state lives on
/// `StructureDesignerModel` (`profilerPanelVisible`, `evalProfilingEnabled`),
/// it is toggled from a *View* menu entry, and it collapses to zero height when
/// hidden.
///
/// **The data is pulled here, not pushed from `refreshFromKernel`.** Adding two
/// synchronous FFI calls to every refresh would tax the drag path that D8a
/// exists to protect, and the tables are of no interest while the panel is
/// closed. So the getters run inside `build`, which happens only when the panel
/// is visible and only when the model notifies — never on a gadget drag tick.
///
/// Phase 3 adds the **Redundancy** tab and two columns to *By node* —
/// `Lookups` and `Wasted` — as new fields rather than a re-reading of
/// `Evals`. Before the evaluation memo lands `lookups == evaluations` exactly;
/// afterwards the difference is the memo's hit count, and a column that quietly
/// changed meaning at that point is how a regression would hide.
library;

import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/profiling_api.dart';
import 'package:provider/provider.dart';
import 'structure_designer_model.dart';

/// Bottom-docked refresh/evaluation profiler.
class ProfilerPanel extends StatefulWidget {
  const ProfilerPanel({super.key});

  @override
  State<ProfilerPanel> createState() => _ProfilerPanelState();
}

class _ProfilerPanelState extends State<ProfilerPanel>
    with SingleTickerProviderStateMixin {
  static const double _panelHeight = 260;
  static const List<String> _tabs = [
    'Phases',
    'By node type',
    'By node',
    'Redundancy',
  ];

  late final TabController _tabController =
      TabController(length: _tabs.length, vsync: this);

  @override
  void dispose() {
    _tabController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Consumer<StructureDesignerModel>(
      builder: (context, model, _) {
        if (!model.profilerPanelVisible) {
          return const SizedBox.shrink();
        }
        // Pulled here rather than mirrored on the model — see the library doc.
        final history = getRefreshProfileHistory();
        final nodeStats = getLastEvalProfile();
        return Container(
          height: _panelHeight,
          decoration: const BoxDecoration(
            color: Color(0xFF1E1E1E),
            border: Border(top: BorderSide(color: Colors.black54, width: 1)),
          ),
          child: Column(
            children: [
              _buildHeader(model),
              Expanded(
                child: TabBarView(
                  controller: _tabController,
                  children: [
                    _PhasesTab(history: history),
                    _ByNodeTypeTab(profile: nodeStats),
                    _ByNodeTab(profile: nodeStats, model: model),
                    _RedundancyTab(profile: nodeStats, model: model),
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
      height: 30,
      padding: const EdgeInsets.only(left: 8),
      color: const Color(0xFF2A2A2A),
      child: Row(
        children: [
          const Text(
            'Profiler',
            style: TextStyle(
              color: Colors.white70,
              fontWeight: FontWeight.w600,
              fontSize: 12,
            ),
          ),
          const SizedBox(width: 12),
          SizedBox(
            width: 400,
            child: TabBar(
              controller: _tabController,
              isScrollable: true,
              tabAlignment: TabAlignment.start,
              labelColor: Colors.white,
              unselectedLabelColor: Colors.white38,
              labelStyle: const TextStyle(fontSize: 11),
              indicatorColor: const Color(0xFF6CA0DC),
              dividerColor: Colors.transparent,
              tabs: [for (final name in _tabs) Tab(height: 30, text: name)],
            ),
          ),
          const Spacer(),
          // The button that makes two readings comparable: without it the panel
          // shows whatever partial refresh happened to run last.
          TextButton.icon(
            key: const Key('profile_full_refresh_button'),
            onPressed: () => model.profileFullRefresh(),
            icon: const Icon(Icons.play_arrow, size: 14),
            label: const Text('Profile full refresh',
                style: TextStyle(fontSize: 11)),
            style: TextButton.styleFrom(foregroundColor: Colors.white70),
          ),
          _PerNodeToggle(model: model),
          _MemoToggle(model: model),
          InkWell(
            onTap: () => model.toggleProfilerPanel(),
            child: const Tooltip(
              message: 'Hide profiler',
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
}

/// The per-node profiler's arm switch, mirrored from the *View* menu entry.
class _PerNodeToggle extends StatelessWidget {
  const _PerNodeToggle({required this.model});

  final StructureDesignerModel model;

  @override
  Widget build(BuildContext context) {
    final on = model.evalProfilingEnabled;
    return Tooltip(
      message: on
          ? 'Per-node profiling is on — it costs two clock reads per node '
              'evaluation, so switch it off when not measuring'
          : 'Per-node profiling is off; phase timing is always on',
      child: InkWell(
        onTap: () => model.setEvalProfilingEnabled(!on),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(on ? Icons.toggle_on : Icons.toggle_off,
                  size: 18,
                  color: on ? const Color(0xFF7BC67B) : Colors.white38),
              const SizedBox(width: 4),
              Text('Per-node',
                  style: TextStyle(
                      fontSize: 11,
                      color: on ? Colors.white70 : Colors.white38)),
            ],
          ),
        ),
      ),
    );
  }
}

/// The evaluation memo's off switch, mirrored from the *View* menu entry.
///
/// It sits beside *Per-node* because the two are used together, but it is the
/// opposite kind of switch: the profiler is a diagnostic that defaults off, and
/// the memo is the product's behaviour that defaults **on**. The colouring says
/// so — an *off* memo is amber, not grey, because it is the abnormal state and
/// the one that makes every later measurement 8x slower.
class _MemoToggle extends StatelessWidget {
  const _MemoToggle({required this.model});

  final StructureDesignerModel model;

  @override
  Widget build(BuildContext context) {
    final on = model.evalMemoEnabled;
    return Tooltip(
      message: on
          ? 'The evaluation memo is on (the normal state): a node feeding '
              'several others is computed once per refresh.\n'
              'Switch it off to recompute the same design without sharing — '
              'the way to tell a memo bug from a wrong network.'
          : 'The evaluation memo is OFF. Every shared node is recomputed once '
              'per consumer, which can be many times slower.\n'
              'This is a diagnostic state; switch it back on when you are '
              'done comparing.',
      child: InkWell(
        key: const Key('eval_memo_toggle'),
        onTap: () => model.setEvalMemoEnabled(!on),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(on ? Icons.toggle_on : Icons.toggle_off,
                  size: 18,
                  color:
                      on ? const Color(0xFF7BC67B) : const Color(0xFFD8A05A)),
              const SizedBox(width: 4),
              Text('Memo',
                  style: TextStyle(
                      fontSize: 11,
                      color: on ? Colors.white70 : const Color(0xFFD8A05A))),
            ],
          ),
        ),
      ),
    );
  }
}

// ============================================================================
// Shared table plumbing
// ============================================================================

const TextStyle _cellStyle = TextStyle(
  fontFamily: 'monospace',
  fontSize: 11,
  color: Colors.white70,
  fontFeatures: [FontFeature.tabularFigures()],
);

const TextStyle _headerCellStyle = TextStyle(
  fontFamily: 'monospace',
  fontSize: 11,
  color: Colors.white38,
  fontWeight: FontWeight.w600,
);

/// A simple flex-column table with an optional footnote strip.
///
/// The footnote is not decoration: two of the three tabs report numbers that
/// read as broken without it (a lazy `map` with a near-zero total, a custom
/// instance with ~zero self time), and D4 requires the panel to explain them
/// rather than the design to "fix" them.
class _Table extends StatelessWidget {
  const _Table({
    required this.columns,
    required this.rows,
    this.footnote,
  });

  final List<_Column> columns;
  final List<_Row> rows;
  final String? footnote;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
          color: const Color(0xFF252525),
          child: Row(
            children: [
              for (final column in columns)
                Expanded(
                  flex: column.flex,
                  child: Text(
                    column.label,
                    textAlign:
                        column.numeric ? TextAlign.right : TextAlign.left,
                    style: _headerCellStyle,
                  ),
                ),
            ],
          ),
        ),
        Expanded(
          child: rows.isEmpty
              ? const SizedBox.shrink()
              : Scrollbar(
                  child: ListView.builder(
                    padding: const EdgeInsets.symmetric(vertical: 2),
                    itemCount: rows.length,
                    itemBuilder: (context, index) =>
                        _buildRow(context, rows[index]),
                  ),
                ),
        ),
        if (footnote != null)
          Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            color: const Color(0xFF252525),
            child: Text(
              footnote!,
              style: const TextStyle(fontSize: 10, color: Colors.white38),
            ),
          ),
      ],
    );
  }

  Widget _buildRow(BuildContext context, _Row row) {
    final content = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      child: Row(
        children: [
          for (var i = 0; i < columns.length; i++)
            Expanded(
              flex: columns[i].flex,
              child: Text(
                row.cells[i],
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign:
                    columns[i].numeric ? TextAlign.right : TextAlign.left,
                style: row.dimmed
                    ? _cellStyle.copyWith(color: Colors.white38)
                    : _cellStyle,
              ),
            ),
        ],
      ),
    );
    if (row.onTap == null) return content;
    return InkWell(onTap: row.onTap, child: content);
  }
}

class _Column {
  const _Column(this.label, this.flex, {this.numeric = false});
  final String label;
  final int flex;
  final bool numeric;
}

class _Row {
  const _Row(this.cells, {this.onTap, this.dimmed = false});
  final List<String> cells;
  final VoidCallback? onTap;
  final bool dimmed;
}

/// Millisecond formatting shared by every table. Sub-millisecond values still
/// read as numbers rather than collapsing to `0.0`.
String _ms(double value) {
  if (value >= 100) return value.toStringAsFixed(0);
  if (value >= 1) return value.toStringAsFixed(1);
  return value.toStringAsFixed(2);
}

/// Whether the evaluation memo declined — or failed — to serve this row, i.e.
/// whether its repeated evaluations are excused.
///
/// Four reasons, and none of them is optional: an unexcused row that
/// re-evaluates within one environment is a memo bug, and that reading is only
/// meaningful if every legitimate reason is marked.
bool _rowFlagged(APINodeProfileRecord row) =>
    row.producedIterator ||
    row.underReentrancyBackstop ||
    row.subnetwork ||
    row.evicted;

/// `Wasted` for one row — **“—” when the memo would not cache the node.**
///
/// A flagged row's projected saving is not money on the table: iterator
/// producers are excluded from the memo for memory reasons, a result produced
/// under the re-entrancy backstop must never be stored at all, a subnetwork
/// instance's single-pin arm cannot store a complete output, and an evicted
/// row's saving was available but the budget could not hold it. Printing a
/// number there would invite chasing a saving that cannot be collected.
String _wasted(APINodeProfileRecord row) {
  if (_rowFlagged(row)) return '—';
  return _ms(row.wastedMs);
}

/// Why a row is not cacheable, for the Redundancy tab's Note column.
String _flagNote(APINodeProfileRecord row) {
  if (row.underReentrancyBackstop) return 'cycle';
  if (row.producedIterator) return 'iterator';
  if (row.evicted) return 'evicted';
  if (row.subnetwork) return 'subnetwork';
  return '';
}

String _pct(double numerator, double denominator) {
  if (denominator <= 0) return '—';
  return '${(100 * numerator / denominator).toStringAsFixed(1)}%';
}

String _modeLabel(APIRefreshMode mode) => switch (mode) {
      APIRefreshMode.full => 'Full',
      APIRefreshMode.partial => 'Partial',
      APIRefreshMode.lightweight => 'Light',
    };

const String _emptyProfileMessage =
    'No per-node measurements yet. Switch “Per-node” on and press “Profile '
    'full refresh”.';

// ============================================================================
// Phases tab
// ============================================================================

/// The refresh history ring, newest first. Answers "is it evaluation or
/// something else?" for the last ~20 refreshes rather than only the last one,
/// which is what makes a 40 ms drag tick and a 1.8 s node activation
/// comparable at a glance.
class _PhasesTab extends StatelessWidget {
  const _PhasesTab({required this.history});

  final List<APIRefreshProfile> history;

  @override
  Widget build(BuildContext context) {
    if (history.isEmpty) {
      return const Center(
        child: Text('No refreshes recorded yet.',
            style: TextStyle(color: Colors.white38, fontSize: 12)),
      );
    }
    final rows = <_Row>[];
    for (final profile in history.reversed) {
      final cache = profile.csgCache;
      final lookups = cache.meshHits +
          cache.meshMisses +
          cache.sketchHits +
          cache.sketchMisses;
      rows.add(_Row([
        // Only an evaluating refresh can have had the memo switched off. A
        // Lightweight row runs no pass at all and so carries default
        // (disabled) counters — tagging it "memo off" would blame the switch
        // for a row that never consulted the memo either way.
        _modeLabel(profile.mode) +
            (profile.evalMs != null && !profile.memo.enabled
                ? ' ·memo off'
                : ''),
        profile.count > 1 ? '×${profile.count}' : '',
        _ms(profile.totalMs),
        // `—`, never `0.00`: a lightweight refresh runs no evaluation pass at
        // all, and a zero there would read as "evaluation is free".
        profile.evalMs == null ? '—' : _ms(profile.evalMs!),
        _ms(profile.sceneDependentMs),
        _ms(profile.gadgetMs),
        _ms(profile.tessellateMs),
        _ms(profile.gpuUploadMs),
        profile.backgroundMs == null ? '—' : _ms(profile.backgroundMs!),
        lookups == BigInt.zero
            ? '—'
            : '${cache.meshHits + cache.sketchHits}/$lookups',
        _memoCell(profile.memo),
        profile.hasNodeStats ? '●' : '',
      ]));
    }
    return Column(
      children: [
        // `history` is oldest-first, so the newest row is the last one.
        _MemoStatsBar(profile: history.last),
        Expanded(child: _phasesTable(rows)),
      ],
    );
  }

  Widget _phasesTable(List<_Row> rows) {
    return _Table(
      columns: const [
        _Column('Mode', 9),
        _Column('N', 3, numeric: true),
        _Column('Total', 5, numeric: true),
        _Column('Eval', 5, numeric: true),
        _Column('Scene', 5, numeric: true),
        _Column('Gadget', 5, numeric: true),
        _Column('Tess', 5, numeric: true),
        _Column('GPU', 5, numeric: true),
        _Column('Bkgnd', 5, numeric: true),
        _Column('CSG hit/lookup', 8, numeric: true),
        _Column('Memo hit/req', 8, numeric: true),
        _Column('Prof', 3, numeric: true),
      ],
      rows: rows,
      footnote:
          'All times in ms. A “Light” row may coalesce a whole gadget drag — N '
          'is how many ticks it covers and the times are their means. CSG and '
          'Memo show cache hits over requests; that time is charged to the node '
          'that triggered the work, not counted separately. A row tagged '
          '“·memo off” was taken with the evaluation memo disabled — that is '
          'what makes an A/B pair readable side by side. “●” marks a refresh '
          'whose per-node table is the one shown in the other tabs.',
    );
  }
}

/// One refresh's memo cell for the Phases ring: hits over requests, or `—` when
/// no pass ran, or `off` when the memo was disabled for it.
///
/// `off` and `—` must stay distinguishable: the first is a switch someone
/// flipped, the second is a lightweight refresh that ran no evaluation at all.
String _memoCell(APIMemoCounts memo) {
  if (!memo.enabled) return 'off';
  final requests = memo.hits + memo.misses;
  if (requests == BigInt.zero) return '—';
  return '${memo.hits}/$requests';
}

/// Bytes as a human figure. Megabytes throughout, matching the unit the Memory
/// preferences are expressed in, so a peak can be read straight against the
/// budget the user set.
String _mb(BigInt bytes) =>
    '${(bytes.toDouble() / (1024 * 1024)).toStringAsFixed(1)} MB';

/// The evaluation memo's numbers for the most recent refresh, above the Phases
/// ring and beside the CSG counters it is the sibling of.
///
/// Always on, like the phase clock and unlike the per-node profiler: a few
/// increments and one `max` per insert are unmeasurable, and someone chasing a
/// memory number should not have to switch on something that distorts the time
/// numbers to see it.
class _MemoStatsBar extends StatelessWidget {
  const _MemoStatsBar({required this.profile});

  final APIRefreshProfile profile;

  @override
  Widget build(BuildContext context) {
    final memo = profile.memo;
    final String text;
    Color color = Colors.white54;

    if (profile.evalMs == null) {
      // A lightweight refresh consulted the memo neither way, so its default
      // counters say nothing about the switch.
      text = 'Evaluation memo: the last refresh ran no evaluation pass (a '
          'lightweight refresh), so there is nothing to report.';
    } else if (!memo.enabled) {
      text =
          'Evaluation memo: OFF for the last refresh — every shared node was '
          'recomputed once per consumer. Switch “Memo” back on above when you '
          'are done comparing.';
      color = const Color(0xFFD8A05A);
    } else if (memo.hits + memo.misses == BigInt.zero) {
      text = 'Evaluation memo: on, but the last refresh made no result '
          'requests at all.';
    } else {
      final buffer = StringBuffer()
        ..write('Evaluation memo: ${memo.hits} hits / '
            '${memo.hits + memo.misses} requests · peak '
            '${memo.peakEntries} entries, ${_mb(memo.peakBytes)} of '
            '${_mb(memo.budgetBytes)} · ended at ${memo.endEntries} entries, '
            '${_mb(memo.endBytes)}');
      if (memo.epochDrops > BigInt.zero) {
        buffer.write(' · ${memo.epochDrops} entries retired with their loop '
            'iteration');
      }
      if (memo.declinedInserts > BigInt.zero) {
        buffer.write(' · ${memo.declinedInserts} deliberately not stored');
      }
      if (memo.insertMs >= 1.0) {
        buffer.write(' · ${_ms(memo.insertMs)} ms measuring entry sizes');
      }
      if (memo.lruEvictions > BigInt.zero) {
        // The one number here that is a *problem* rather than a reading, and
        // the trigger the design's Phase 5 fires on.
        buffer.write(' · ⚠ ${memo.lruEvictions} entries evicted for space '
            '(${memo.evictedMisses} recomputed as a result) — raise '
            '“Evaluation memo (MB)” in Preferences > Memory');
        color = const Color(0xFFD8A05A);
      }
      if (memo.insertedTrackingTruncated) {
        buffer.write(' · eviction tracking hit its ceiling, so the recomputed '
            'count is a floor');
      }
      text = buffer.toString();
    }

    return Container(
      key: const Key('memo_stats_bar'),
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      color: const Color(0xFF232323),
      child: Text(
        text,
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(fontSize: 10, color: color),
      ),
    );
  }
}

// ============================================================================
// By node type tab
// ============================================================================

class _ByNodeTypeTab extends StatelessWidget {
  const _ByNodeTypeTab({required this.profile});

  final APIEvalProfile? profile;

  @override
  Widget build(BuildContext context) {
    final profile = this.profile;
    if (profile == null || profile.byNodeType.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.symmetric(horizontal: 24),
          child: Text(_emptyProfileMessage,
              textAlign: TextAlign.center,
              style: TextStyle(color: Colors.white38, fontSize: 12)),
        ),
      );
    }
    final sorted = [...profile.byNodeType]
      ..sort((a, b) => b.selfMs.compareTo(a.selfMs));
    final totalSelf = profile.totalSelfMs;
    return _Table(
      columns: const [
        _Column('Type', 12),
        _Column('Nodes', 4, numeric: true),
        _Column('Evals', 4, numeric: true),
        _Column('Self', 5, numeric: true),
        _Column('Total', 5, numeric: true),
        _Column('% self', 5, numeric: true),
      ],
      rows: [
        for (final row in sorted)
          _Row([
            row.nodeTypeName,
            row.nodes.toString(),
            row.evaluations.toString(),
            _ms(row.selfMs),
            _ms(row.totalMs),
            _pct(row.selfMs, totalSelf),
          ]),
      ],
      footnote: 'Self time excludes everything a node pulled from upstream; '
          '% self is a share of the pass’s ${_ms(totalSelf)} ms total. '
          '${profile.totalEvaluations} evaluations in this pass.',
    );
  }
}

// ============================================================================
// By node tab
// ============================================================================

class _ByNodeTab extends StatelessWidget {
  const _ByNodeTab({required this.profile, required this.model});

  final APIEvalProfile? profile;
  final StructureDesignerModel model;

  @override
  Widget build(BuildContext context) {
    final profile = this.profile;
    if (profile == null || profile.byNode.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.symmetric(horizontal: 24),
          child: Text(_emptyProfileMessage,
              textAlign: TextAlign.center,
              style: TextStyle(color: Colors.white38, fontSize: 12)),
        ),
      );
    }
    final sorted = [...profile.byNode]
      ..sort((a, b) => b.selfMs.compareTo(a.selfMs));
    return _Table(
      columns: const [
        _Column('Node', 16),
        _Column('Lookups', 4, numeric: true),
        _Column('Evals', 4, numeric: true),
        _Column('Self', 5, numeric: true),
        _Column('Total', 5, numeric: true),
        _Column('Wasted', 5, numeric: true),
      ],
      rows: [
        for (final row in sorted)
          _Row(
            [
              row.label,
              row.lookups.toString(),
              row.evaluations.toString(),
              _ms(row.selfMs),
              _ms(row.totalMs),
              _wasted(row),
            ],
            dimmed: !row.navigable,
            onTap: row.navigable ? () => _jumpTo(row) : null,
          ),
      ],
      footnote:
          'Click a row to jump to the node — including into another network: a '
          'row named “other_net/materialize#8” opens that network and selects '
          'node 8 there. “Lookups” counts requests and “Evals” counts runs of '
          'the node; with the evaluation memo on, the difference between them '
          'is what the memo served rather than recomputed. “Wasted” is the '
          'self time a perfect memo would avoid; “—” marks a node the memo '
          'would not cache (see Redundancy). Two readings are expected and are '
          'not bugs: a custom-node instance shows ~zero self against a large '
          'total (it delegates to its network’s return node), and a lazy “map” '
          'shows a near-zero total with its body’s time nested under the '
          '“collect” that pulled it.',
    );
  }

  /// Reuses the scope-aware canvas navigation from Find Usages / error
  /// navigation, so the landing behaves identically from either entry point.
  static void _jumpToRecord(
      StructureDesignerModel model, APINodeProfileRecord record) {
    // `scopePath` is FRB's `Uint64List` (BigInt elements), not
    // `dart:typed_data`'s — `toList()` is what the other jump call sites use.
    model.jumpToNode(
      record.hostNetwork,
      record.scopePath.toList(),
      record.nodeId,
    );
  }

  void _jumpTo(APINodeProfileRecord record) {
    // `scopePath` is FRB's `Uint64List` (BigInt elements), not
    // `dart:typed_data`'s — `toList()` is what the other jump call sites use.
    model.jumpToNode(
      record.hostNetwork,
      record.scopePath.toList(),
      record.nodeId,
    );
  }
}

// ============================================================================
// Redundancy tab (Phase 3)
// ============================================================================

/// **The memo's business case, and afterwards its regression test.**
///
/// The number that matters is not how often a node was evaluated but how often
/// it was evaluated *in an environment it had already been evaluated in*. A
/// `map` body run once per element runs in a different environment each time
/// and is not redundant at all; a diamond apex pulled twice is one environment
/// and one avoidable evaluation. Only the second kind is something a memo could
/// remove, which is why every row here is `Lookups` against `Envs`.
class _RedundancyTab extends StatelessWidget {
  const _RedundancyTab({required this.profile, required this.model});

  final APIEvalProfile? profile;
  final StructureDesignerModel model;

  @override
  Widget build(BuildContext context) {
    final profile = this.profile;
    if (profile == null || profile.byNode.isEmpty) {
      return Column(
        children: [
          _SelfCheckBar(model: model, profile: profile),
          const Expanded(
            child: Center(
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: 24),
                child: Text(_emptyProfileMessage,
                    textAlign: TextAlign.center,
                    style: TextStyle(color: Colors.white38, fontSize: 12)),
              ),
            ),
          ),
        ],
      );
    }
    // Ranked by Wasted — the projected saving — with the rows the memo would
    // not cache sorted last rather than hidden: they are still measurements.
    final sorted = [...profile.byNode]..sort((a, b) {
        final aFlagged = _rowFlagged(a);
        final bFlagged = _rowFlagged(b);
        if (aFlagged != bFlagged) return aFlagged ? 1 : -1;
        return b.wastedMs.compareTo(a.wastedMs);
      });
    return Column(
      children: [
        _SelfCheckBar(model: model, profile: profile),
        Expanded(
          child: _Table(
            columns: const [
              _Column('Node', 14),
              _Column('Lookups', 4, numeric: true),
              _Column('Envs', 4, numeric: true),
              _Column('Factor', 4, numeric: true),
              _Column('Self', 4, numeric: true),
              _Column('Wasted', 4, numeric: true),
              _Column('Note', 4),
            ],
            rows: [
              for (final row in sorted)
                _Row(
                  [
                    row.label,
                    row.lookups.toString(),
                    row.distinctEnvs.toString(),
                    '${row.redundancyFactor.toStringAsFixed(1)}×',
                    _ms(row.selfMs),
                    _wasted(row),
                    _flagNote(row),
                  ],
                  dimmed: !row.navigable || _rowFlagged(row),
                  onTap: row.navigable
                      ? () => _ByNodeTab._jumpToRecord(model, row)
                      : null,
                ),
            ],
            footnote: _footnote(profile, model.evalMemoEnabled),
          ),
        ),
      ],
    );
  }

  String _footnote(APIEvalProfile profile, bool memoEnabled) {
    final buffer = StringBuffer()
      ..write('${profile.totalLookups} lookups over '
          '${profile.totalDistinctEnvs} distinct environments '
          '(${profile.redundancyFactor.toStringAsFixed(2)}× overall). An '
          'environment is the call stack extended with the iteration of each '
          'enclosing loop, so a body node run once per element reads 1.0× — '
          'that is not redundancy. Rows marked “iterator”, “cycle”, '
          '“subnetwork” or “evicted” were not served from the memo, so their '
          'Wasted shows “—”.');
    // The acceptance criterion, as one number the reader does not have to
    // derive by diffing two columns.
    if (memoEnabled) {
      buffer.write(profile.unmemoizedOffenders == BigInt.zero
          ? ' No unexplained repeats: every row either ran once per environment '
              'or carries a reason why it could not.'
          : ' ⚠ ${profile.unmemoizedOffenders} row(s) were recomputed within a '
              'single environment with no reason given — that is a memo bug '
              'worth reporting.');
    } else {
      buffer.write(' The memo is off, so a perfect one would save about '
          '${_ms(profile.projectedSavingMs)} ms of this pass.');
    }
    if (profile.envsTruncated) {
      buffer.write(' ⚠ Environment tracking hit its ceiling: the environment '
          'counts are floors and the factors are upper bounds.');
    }
    return buffer.toString();
  }
}

/// The D11 self-check's state, above the table.
///
/// It says two things the numbers cannot: whether the check actually ran (and
/// over the whole pass, or only up to its sampling ceiling), and — the part
/// that keeps a green result meaningful later — that it is only conclusive
/// while no evaluation memo is serving second requests from first results.
class _SelfCheckBar extends StatelessWidget {
  const _SelfCheckBar({required this.model, required this.profile});

  final StructureDesignerModel model;
  final APIEvalProfile? profile;

  @override
  Widget build(BuildContext context) {
    final violations = profile?.selfCheckViolations ?? const [];
    final ran = profile?.selfCheckRan ?? false;
    final truncated = profile?.selfCheckTruncated ?? false;

    final String status;
    final Color color;
    if (violations.isNotEmpty) {
      status = 'Self-check: ${violations.length} equal-key/different-result '
          'violation(s) — first: ${violations.first.label}';
      color = const Color(0xFFE08A8A);
    } else if (ran && truncated) {
      status = 'Self-check: clean so far, but sampling hit its ceiling — the '
          'environments after that point were not checked.';
      color = const Color(0xFFD8C07A);
    } else if (ran) {
      status = 'Self-check: clean — every pair of evaluations sharing an '
          'environment produced the same result (the check can only be armed '
          'with the memo off, so this is a real test rather than a vacuous one)';
      color = const Color(0xFF7BC67B);
    } else if (model.evalSelfCheckEnabled) {
      status = 'Self-check: armed — it runs on the next profiled pass.';
      color = Colors.white38;
    } else if (model.evalMemoEnabled) {
      // The D10 hard gate. Arming is refused rather than silently forcing the
      // memo off for the pass: that would make one switch have two effects,
      // and the second one — a profile 8x slower than the product, sitting in
      // the same history ring as comparable ones — would be invisible.
      status = 'Self-check: unavailable while the evaluation memo is on. The '
          'check compares two computations of one environment, and the memo '
          'serves the second from the first — so it would pass vacuously. '
          'Switch “Memo” off above to arm it.';
      color = const Color(0xFFD8A05A);
    } else {
      status = 'Self-check: off. Arming it validates the environment key '
          'against real designs; it costs a result summary per evaluation and '
          'one retained summary per distinct environment, so leave it off when '
          'you are measuring time rather than correctness.';
      color = Colors.white38;
    }

    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      color: const Color(0xFF232323),
      child: Row(
        children: [
          InkWell(
            key: const Key('eval_self_check_toggle'),
            // Inert while the memo is on: the kernel refuses the arm, and an
            // affordance that silently does nothing is worse than one that is
            // visibly unavailable with the reason spelled out beside it.
            onTap: (model.evalMemoEnabled && !model.evalSelfCheckEnabled)
                ? null
                : () =>
                    model.setEvalSelfCheckEnabled(!model.evalSelfCheckEnabled),
            child: Padding(
              padding: const EdgeInsets.only(right: 6),
              child: Icon(
                model.evalSelfCheckEnabled ? Icons.toggle_on : Icons.toggle_off,
                size: 18,
                color: model.evalSelfCheckEnabled
                    ? const Color(0xFF7BC67B)
                    : Colors.white38,
              ),
            ),
          ),
          Expanded(
            child: Text(
              status,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 10, color: color),
            ),
          ),
        ],
      ),
    );
  }
}
