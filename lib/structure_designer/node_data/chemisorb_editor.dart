import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/chemisorb_api.dart'
    as chemisorb_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor for the `chemisorb` node — every way a posed adsorbate can bond to
/// a substrate, relaxed and ranked.
///
/// Bond forming is always on; transfers are enabled by wiring the `transfers`
/// pin, and `max_transfers` stays visible but greyed while it is not — a value
/// nothing reads now, which a wire makes live again unchanged.
///
/// The search runs only when **Run** is pressed; until then, and whenever the
/// inputs or settings change after a run, the node outputs the *plan* and the
/// report says how many hypotheses a run would relax. The report (statistics,
/// warnings and the ranked candidates) lives in the selected node's eval cache
/// and is re-read on every model notification, as `proxy_editor.dart` does.
class ChemisorbEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIChemisorbData? data;
  final StructureDesignerModel model;

  /// Whether the `transfers` pin is wired; `max_transfers` is read only then.
  final bool transfersConnected;

  const ChemisorbEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
    required this.transfersConnected,
  });

  @override
  State<ChemisorbEditor> createState() => _ChemisorbEditorState();
}

class _ChemisorbEditorState extends State<ChemisorbEditor> {
  APIChemisorbReport? _report;

  @override
  void initState() {
    super.initState();
    _updateReport();
    widget.model.addListener(_updateReport);
  }

  @override
  void dispose() {
    widget.model.removeListener(_updateReport);
    super.dispose();
  }

  void _updateReport() {
    final report = chemisorb_api.getChemisorbReport();
    if (mounted) {
      setState(() {
        _report = report;
      });
    }
  }

  void _commit({
    String? adsorbateTag,
    String? substrateTag,
    double? reach,
    double? pairTolerance,
    int? maxFormedBonds,
    int? maxTransfers,
    int? topN,
    double? energyWindow,
    int? budget,
    int? maxIterations,
  }) {
    final data = widget.data;
    if (data == null) return;
    widget.model.setChemisorbData(
      widget.nodeId,
      APIChemisorbData(
        adsorbateTag: adsorbateTag ?? data.adsorbateTag,
        substrateTag: substrateTag ?? data.substrateTag,
        reach: reach ?? data.reach,
        pairTolerance: pairTolerance ?? data.pairTolerance,
        maxFormedBonds: maxFormedBonds ?? data.maxFormedBonds,
        maxTransfers: maxTransfers ?? data.maxTransfers,
        topN: topN ?? data.topN,
        energyWindow: energyWindow ?? data.energyWindow,
        budget: budget ?? data.budget,
        maxIterations: maxIterations ?? data.maxIterations,
      ),
    );
  }

  /// Runs the search behind a modal placard — `runExecuteWithPlacard`'s
  /// recipe: the FFI call is synchronous and blocks the UI thread, so the
  /// placard is painted first (`endOfFrame`), dismissed in `finally`, and the
  /// outcome reported after it is gone. No spinner: it would freeze mid-frame.
  Future<void> _run() async {
    // Both captured before the await: the refresh the run ends with may
    // rebuild this panel, and the placard must be dismissed regardless.
    final messenger = ScaffoldMessenger.maybeOf(context);
    final navigator = Navigator.of(context);
    final hypotheses = _report?.stats.toRelax;
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (_) => DraggableDialog(
        width: 360,
        dismissible: false,
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.hourglass_empty),
              const SizedBox(width: 16),
              Flexible(
                child: Text(hypotheses == null
                    ? 'Searching…'
                    : 'Relaxing $hypotheses hypotheses…'),
              ),
            ],
          ),
        ),
      ),
    );
    await SchedulerBinding.instance.endOfFrame;

    APIChemisorbRunResult? result;
    Object? thrown;
    try {
      result = widget.model.runChemisorb(widget.nodeId);
    } catch (e) {
      thrown = e;
    } finally {
      navigator.pop();
    }

    if (thrown != null) {
      if (messenger != null) {
        showErrorSnackBarOn(messenger, 'Chemisorption search failed: $thrown');
      }
    } else if (result != null && mounted) {
      final best = result.bestScore;
      showTransientSnackBar(
          context,
          best == null
              ? 'Search done: no bonding pattern found.'
              : 'Search done: ${result.relaxed} relaxed, best '
                  '${formatNatural(best, 4)} kcal/mol.');
    }
  }

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Chemisorption Search',
            nodeTypeName: 'chemisorb',
          ),
          const SizedBox(height: 12),

          // ---- Run, first: it is what the panel is for --------------------
          _RunRow(report: _report, onRun: _run),
          const SizedBox(height: 12),

          // ---- reactive atoms ---------------------------------------------
          StringInput(
            key: ValueKey('chemisorb_ads_tag_${data.adsorbateTag}'),
            label: 'Adsorbate tag (empty = all atoms)',
            value: data.adsorbateTag,
            onChanged: (v) => _commit(adsorbateTag: v.trim()),
          ),
          const SizedBox(height: 8),
          StringInput(
            key: ValueKey('chemisorb_sub_tag_${data.substrateTag}'),
            label: 'Substrate tag (empty = all atoms)',
            value: data.substrateTag,
            onChanged: (v) => _commit(substrateTag: v.trim()),
          ),
          const SizedBox(height: 12),

          // ---- geometry ---------------------------------------------------
          FloatInput(
            label: 'Reach (Å)',
            value: data.reach,
            onChanged: (v) => _commit(reach: v),
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Pair tolerance (Å, 0 = off)',
            value: data.pairTolerance,
            onChanged: (v) => _commit(pairTolerance: v),
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Max formed bonds (0 = no cap)',
            value: data.maxFormedBonds,
            minimumValue: 0,
            onChanged: (v) => _commit(maxFormedBonds: v),
          ),
          const SizedBox(height: 8),
          Opacity(
            opacity: widget.transfersConnected ? 1.0 : 0.5,
            child: IgnorePointer(
              ignoring: !widget.transfersConnected,
              child: IntInput(
                label: 'Max transfers',
                value: data.maxTransfers,
                minimumValue: 1,
                onChanged: (v) => _commit(maxTransfers: v),
              ),
            ),
          ),
          if (!widget.transfersConnected)
            Padding(
              padding: const EdgeInsets.only(top: 4.0),
              child: Text(
                'Wire `transfers` to enable H / halogen transfers.',
                style: Theme.of(context)
                    .textTheme
                    .bodySmall
                    ?.copyWith(fontStyle: FontStyle.italic),
              ),
            ),
          const SizedBox(height: 12),

          // ---- cost -------------------------------------------------------
          IntInput(
            label: 'Budget (relaxations)',
            value: data.budget,
            minimumValue: 1,
            onChanged: (v) => _commit(budget: v),
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Max UFF iterations',
            value: data.maxIterations,
            minimumValue: 1,
            onChanged: (v) => _commit(maxIterations: v),
          ),
          const SizedBox(height: 12),

          // ---- listing (re-lists a result, never makes it stale) ----------
          IntInput(
            label: 'Top N',
            value: data.topN,
            minimumValue: 1,
            onChanged: (v) => _commit(topN: v),
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Energy window (kcal/mol above best)',
            value: data.energyWindow,
            onChanged: (v) => _commit(energyWindow: v),
          ),
          const SizedBox(height: 16),

          // ---- the report -------------------------------------------------
          _StatsCard(report: _report),
          const SizedBox(height: 12),
          if (_report != null && _report!.rows.isNotEmpty)
            _CandidatesCard(rows: _report!.rows),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}

/// The Run button, with what a run would cost next to it (or what the last
/// one did), and the stale / not-run state in words.
class _RunRow extends StatelessWidget {
  final APIChemisorbReport? report;
  final VoidCallback onRun;

  const _RunRow({required this.report, required this.onRun});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final stats = report?.stats;
    final String status;
    if (stats == null) {
      status = 'Display the node to see the plan.';
    } else if (stats.searched) {
      status = 'Searched: ${stats.relaxed} relaxed '
          'in ${formatNatural(stats.seconds, 3)} s.';
    } else {
      status = '${stats.toRelax} hypotheses to relax'
          '${stats.truncated ? ' (budget hit)' : ''}.';
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            ElevatedButton.icon(
              onPressed: onRun,
              icon: const Icon(Icons.play_arrow),
              label: const Text('Run'),
            ),
            const SizedBox(width: 12),
            Expanded(child: Text(status, style: theme.textTheme.bodySmall)),
          ],
        ),
        if (stats != null && stats.stale)
          Padding(
            padding: const EdgeInsets.only(top: 6.0),
            child: Text(
              'Inputs changed since the last run — Run again.',
              style: theme.textTheme.bodySmall
                  ?.copyWith(color: theme.colorScheme.error),
            ),
          ),
        if (stats != null && !stats.searched && !stats.stale)
          Padding(
            padding: const EdgeInsets.only(top: 6.0),
            child: Text(
              'Not run: the outputs show the unrelaxed pose. Results are not '
              'saved with the file.',
              style: theme.textTheme.bodySmall,
            ),
          ),
      ],
    );
  }
}

class _StatsCard extends StatelessWidget {
  final APIChemisorbReport? report;

  const _StatsCard({required this.report});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final stats = report?.stats;
    return Card(
      elevation: 1,
      child: Padding(
        padding: const EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Search statistics', style: theme.textTheme.titleSmall),
            const SizedBox(height: 8),
            if (stats == null)
              Text('No evaluation yet — display the node.',
                  style: theme.textTheme.bodySmall)
            else ...[
              if (stats.truncated)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6.0),
                  child: Text(
                    'NOT EXHAUSTIVE — the budget was hit.',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.error,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ),
              if (stats.estimatedPairs.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6.0),
                  child: Text(
                    'Bond energies estimated (Pauling) for: '
                    '${stats.estimatedPairs}',
                    style: theme.textTheme.bodySmall
                        ?.copyWith(color: theme.colorScheme.error),
                  ),
                ),
              _Row('Feet / sites in reach',
                  '${stats.feet} / ${stats.sitesInReach}'),
              if (stats.transferCandidates > BigInt.zero)
                _Row('Transfer candidates', '${stats.transferCandidates}'),
              _Row('Assignments considered', '${stats.considered}'),
              _Row('Pruned: valence', '${stats.prunedValence}'),
              _Row('Pruned: pair tolerance', '${stats.prunedPairTolerance}'),
              _Row('Duplicates', '${stats.duplicates}'),
              _Row('To relax', '${stats.toRelax}'),
              _Row('Relaxed', '${stats.relaxed}'),
              _Row('Unconverged', '${stats.unconverged}'),
              _Row('Listed', '${stats.listed}'),
              _Row(stats.searched ? 'Search time' : 'Plan time',
                  '${formatNatural(stats.seconds, 3)} s'),
            ],
          ],
        ),
      ),
    );
  }
}

/// The listed candidates in rank order. The strain is shown beside the score
/// on purpose: the bond-energy term dominates the score, so rank 1 can be a
/// heavily strained binding with more bonds than a clean one.
class _CandidatesCard extends StatelessWidget {
  final List<APIChemisorbRow> rows;

  const _CandidatesCard({required this.rows});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mono = theme.textTheme.bodySmall?.copyWith(fontFamily: 'monospace');
    return Card(
      elevation: 1,
      child: Padding(
        padding: const EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Candidates (kcal/mol)', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            Text('rank  score  strain  bond', style: mono),
            const Divider(height: 8),
            for (final row in rows)
              Tooltip(
                message: 'sites: ${row.sites}\n'
                    'worst bond ratio: ${formatNatural(row.worstBondRatio, 3)}\n'
                    'stretch ${formatNatural(row.stretch, 3)}, '
                    'bend ${formatNatural(row.bend, 3)}, '
                    'torsion ${formatNatural(row.torsion, 3)}, '
                    'inversion ${formatNatural(row.inversion, 3)}, '
                    'vdW ${formatNatural(row.vdw, 3)}',
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 3.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '#${row.rank}  ${formatNatural(row.score, 4)}  '
                        '${formatNatural(row.strain, 3)}  '
                        '${formatNatural(row.bondEnergy, 4)}'
                        '${row.estimated ? '  est.' : ''}'
                        '${row.converged ? '' : '  unconv.'}',
                        style: mono?.copyWith(
                          color: row.converged ? null : theme.colorScheme.error,
                        ),
                      ),
                      Text(row.bonds, style: theme.textTheme.bodySmall),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _Row extends StatelessWidget {
  final String label;
  final String value;

  const _Row(this.label, this.value);

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1.5),
      child: Row(
        children: [
          Expanded(
            child: Text(label, style: Theme.of(context).textTheme.bodySmall),
          ),
          Text(
            value,
            style: Theme.of(context)
                .textTheme
                .bodySmall
                ?.copyWith(fontFamily: 'monospace'),
          ),
        ],
      ),
    );
  }
}
