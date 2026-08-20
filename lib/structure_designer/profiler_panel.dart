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
/// Three columns' worth of vocabulary is deliberately absent until Phase 3:
/// `Lookups` and `Wasted` are defined per *evaluation environment*, and before
/// the memo lands `lookups == evaluations` exactly. A column that quietly
/// changes meaning once the memo ships is how a regression hides, so the memo's
/// business case gets new columns rather than a re-reading of this one.
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
  static const List<String> _tabs = ['Phases', 'By node type', 'By node'];

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
            width: 320,
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
        _modeLabel(profile.mode),
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
        profile.hasNodeStats ? '●' : '',
      ]));
    }
    return _Table(
      columns: const [
        _Column('Mode', 7),
        _Column('N', 3, numeric: true),
        _Column('Total', 5, numeric: true),
        _Column('Eval', 5, numeric: true),
        _Column('Scene', 5, numeric: true),
        _Column('Gadget', 5, numeric: true),
        _Column('Tess', 5, numeric: true),
        _Column('GPU', 5, numeric: true),
        _Column('Bkgnd', 5, numeric: true),
        _Column('CSG hit/lookup', 8, numeric: true),
        _Column('Prof', 3, numeric: true),
      ],
      rows: rows,
      footnote:
          'All times in ms. A “Light” row may coalesce a whole gadget drag — N '
          'is how many ticks it covers and the times are their means. CSG shows '
          'conversion-cache hits over lookups; that time is charged to the node '
          'that triggered it, not counted separately. “●” marks a refresh whose '
          'per-node table is the one shown in the other tabs.',
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
        _Column('Evals', 4, numeric: true),
        _Column('Self', 5, numeric: true),
        _Column('Total', 5, numeric: true),
      ],
      rows: [
        for (final row in sorted)
          _Row(
            [
              row.label,
              row.evaluations.toString(),
              _ms(row.selfMs),
              _ms(row.totalMs),
            ],
            dimmed: !row.navigable,
            onTap: row.navigable ? () => _jumpTo(row) : null,
          ),
      ],
      footnote:
          'Click a row to jump to the node — including into another network: a '
          'row named “other_net/materialize#8” opens that network and selects '
          'node 8 there. Two readings are expected and are not bugs: a '
          'custom-node instance shows ~zero self against a large total (it '
          'delegates to its network’s return node), and a lazy “map” shows a '
          'near-zero total with its body’s time nested under the “collect” '
          'that pulled it.',
    );
  }

  /// Reuses the scope-aware canvas navigation from Find Usages / error
  /// navigation, so the landing behaves identically from either entry point.
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
