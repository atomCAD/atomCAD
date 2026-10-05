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
/// The settings are two groups, and the panel keeps them visibly apart:
/// **search settings** (changing one needs a new Run) and **filters after
/// search** (formed bonds, bond inventory, top N, energy window), which only
/// choose what a finished search lists and apply at once. The split is a
/// Rust-side rule — the filters are not fingerprinted — that the layout makes
/// legible; it is not something the panel decides.
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

  /// Writes the settings with the given fields replaced. The two filters are
  /// nullable — `null` means "no filter" — so they take [_keep] as their
  /// "leave unchanged" default instead.
  void _commit({
    String? adsorbateTag,
    String? substrateTag,
    double? reach,
    int? maxFormedBonds,
    int? maxTransfers,
    Object? filterFormedBonds = _keep,
    Object? filterBonds = _keep,
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
        maxFormedBonds: maxFormedBonds ?? data.maxFormedBonds,
        maxTransfers: maxTransfers ?? data.maxTransfers,
        filterFormedBonds: identical(filterFormedBonds, _keep)
            ? data.filterFormedBonds
            : filterFormedBonds as int?,
        filterBonds: identical(filterBonds, _keep)
            ? data.filterBonds
            : filterBonds as String?,
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
      final best = result.bestStrain;
      showTransientSnackBar(
          context,
          best == null
              ? 'Search done: ${result.relaxed} relaxed, none listed.'
              : 'Search done: ${result.relaxed} relaxed, best listed '
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

          // ==== search settings: a change needs a new Run ===================
          const _SectionHeader(
            title: 'Search settings',
            note: 'Changing these needs a new Run.',
          ),
          const SizedBox(height: 8),
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
          const SizedBox(height: 8),
          FloatInput(
            label: 'Reach (Å)',
            value: data.reach,
            onChanged: (v) => _commit(reach: v),
          ),
          const SizedBox(height: 8),
          _CapField(
            checkboxLabel: 'Limit formed bonds',
            fieldLabel: 'Max formed bonds (0 = none)',
            value: data.maxFormedBonds,
            onChanged: (v) => _commit(maxFormedBonds: v),
          ),
          Opacity(
            opacity: widget.transfersConnected ? 1.0 : 0.5,
            child: IgnorePointer(
              ignoring: !widget.transfersConnected,
              child: _CapField(
                checkboxLabel: 'Limit transfers',
                fieldLabel: 'Max transfers (0 = none)',
                value: data.maxTransfers,
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
          const SizedBox(height: 8),
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
          const SizedBox(height: 20),

          // ==== filters after search: re-list, never re-run ===============
          const _SectionHeader(
            title: 'Filters after search',
            note: 'These only choose what is listed from the last search and '
                'apply at once — no new Run needed.',
          ),
          const SizedBox(height: 4),
          CheckboxListTile(
            title: const Text('Only a number of formed bonds'),
            value: data.filterFormedBonds != null,
            onChanged: (value) => _commit(
              filterFormedBonds:
                  (value ?? false) ? _DEFAULT_FORMED_BONDS_FILTER : null,
            ),
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
          if (data.filterFormedBonds != null)
            Padding(
              padding: const EdgeInsets.only(left: 16.0, bottom: 8.0),
              child: IntInput(
                label: 'Formed bonds (exactly)',
                value: data.filterFormedBonds!,
                minimumValue: 1,
                onChanged: (v) => _commit(filterFormedBonds: v),
              ),
            ),
          _InventoryDropdown(
            value: data.filterBonds,
            options: _report?.inventoryOptions ?? const [],
            onChanged: (v) => _commit(filterBonds: v),
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Top N',
            value: data.topN,
            minimumValue: 1,
            onChanged: (v) => _commit(topN: v),
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Energy window (kcal/mol above the best listed)',
            value: data.energyWindow,
            onChanged: (v) => _commit(energyWindow: v),
          ),
          const SizedBox(height: 16),

          // ---- the report -------------------------------------------------
          _StatsCard(report: _report),
          const SizedBox(height: 12),
          if (_report != null && _report!.stats.searched)
            _CandidatesCard(report: _report!),
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
              _Row('Feet / sites in reach',
                  '${stats.feet} / ${stats.sitesInReach}'),
              if (stats.transferCandidates > BigInt.zero)
                _Row('Transfer candidates', '${stats.transferCandidates}'),
              _Row('Assignments considered', '${stats.considered}'),
              _Row('Pruned: valence', '${stats.prunedValence}'),
              _Row('Duplicates', '${stats.duplicates}'),
              _Row('To relax', '${stats.toRelax}'),
              _Row('Relaxed', '${stats.relaxed}'),
              _Row('Unconverged', '${stats.unconverged}'),
              if (stats.searched) ...[
                _Row('Match formed bonds / inventory', '${stats.matching}'),
                _Row('Listed (after top N / window)', '${stats.listed}'),
              ],
              _Row(stats.searched ? 'Search time' : 'Plan time',
                  '${formatNatural(stats.seconds, 3)} s'),
            ],
          ],
        ),
      ),
    );
  }
}

/// The listed candidates in rank order, by strain (UFF energy against the
/// same pose relaxed with no bonds formed). The bond inventory is shown under
/// every row because strains of different inventories do not compare cleanly.
/// `rank` is the place in the whole ranking, so a filtered list may skip
/// numbers; the line under the title says what the filters left out.
class _CandidatesCard extends StatelessWidget {
  final APIChemisorbReport report;

  const _CandidatesCard({required this.report});

  /// What the filters and the window/top N leave out, in one line.
  static String _hiddenLine(APIChemisorbStats s) {
    final filtered = s.relaxed - s.matching;
    final beyond = s.matching - s.listed;
    final parts = <String>[
      if (filtered > BigInt.zero) '$filtered filtered out',
      if (beyond > BigInt.zero) '$beyond past top N / energy window',
    ];
    return parts.isEmpty ? '' : 'Not listed: ${parts.join(', ')}.';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mono = theme.textTheme.bodySmall?.copyWith(fontFamily: 'monospace');
    final rows = report.rows;
    final hidden = _hiddenLine(report.stats);
    return Card(
      elevation: 1,
      child: Padding(
        padding: const EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Candidates (kcal/mol)', style: theme.textTheme.titleSmall),
            if (hidden.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 2.0),
                child: Text(
                  hidden,
                  style: theme.textTheme.bodySmall
                      ?.copyWith(fontStyle: FontStyle.italic),
                ),
              ),
            const SizedBox(height: 4),
            if (rows.isEmpty)
              Text('No candidate passes the filters.',
                  style: theme.textTheme.bodySmall)
            else ...[
              Text('rank  strain', style: mono),
              const Divider(height: 8),
            ],
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
                        '#${row.rank}  ${formatNatural(row.strain, 4)}'
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

/// A cap that can be switched off: a checkbox, and the value only while it
/// is ticked. The node stores "no cap" as [_NO_CAP] (`-1`), so `0` can mean
/// what it says; that encoding stays out of the panel. Ticking seeds
/// [_DEFAULT_CAP].
class _CapField extends StatelessWidget {
  final String checkboxLabel;
  final String fieldLabel;
  final int value;
  final ValueChanged<int> onChanged;

  const _CapField({
    required this.checkboxLabel,
    required this.fieldLabel,
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    final capped = value != _NO_CAP;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        CheckboxListTile(
          title: Text(checkboxLabel),
          value: capped,
          onChanged: (ticked) =>
              onChanged((ticked ?? false) ? _DEFAULT_CAP : _NO_CAP),
          controlAffinity: ListTileControlAffinity.leading,
          contentPadding: EdgeInsets.zero,
          dense: true,
        ),
        if (capped)
          Padding(
            padding: const EdgeInsets.only(left: 16.0, bottom: 8.0),
            child: IntInput(
              label: fieldLabel,
              value: value,
              minimumValue: 0,
              onChanged: onChanged,
            ),
          ),
      ],
    );
  }
}

/// How the node stores "no cap" for `max_formed_bonds` and `max_transfers`.
const int _NO_CAP = -1;

/// The value a cap takes when its checkbox is first ticked.
const int _DEFAULT_CAP = 1;

/// The `formed bonds` filter's value when it is first ticked.
const int _DEFAULT_FORMED_BONDS_FILTER = 1;

/// The "leave unchanged" default of [_ChemisorbEditorState._commit]'s two
/// nullable filter arguments, where `null` already means "no filter".
const Object _keep = Object();

/// A section title with a one-line note under it.
class _SectionHeader extends StatelessWidget {
  final String title;
  final String note;

  const _SectionHeader({required this.title, required this.note});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: theme.textTheme.titleSmall),
        const SizedBox(height: 2),
        Text(
          note,
          style:
              theme.textTheme.bodySmall?.copyWith(fontStyle: FontStyle.italic),
        ),
        const Divider(height: 10),
      ],
    );
  }
}

/// The bond-inventory filter: "any", or one of the inventories the current
/// result (or, before a run, the plan) contains, each with its count. A stored
/// choice the current result does not contain stays selectable, with a count
/// of 0, so the dropdown never silently drops it.
class _InventoryDropdown extends StatelessWidget {
  final String? value;
  final List<APIChemisorbInventoryOption> options;
  final ValueChanged<String?> onChanged;

  const _InventoryDropdown({
    required this.value,
    required this.options,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    final entries = <(String, BigInt)>[
      for (final o in options) (o.label, o.count),
      if (value != null && !options.any((o) => o.label == value))
        (value!, BigInt.zero),
    ];
    return DropdownButtonFormField<String?>(
      key: ValueKey('chemisorb_bonds_filter_$value'),
      decoration: const InputDecoration(
        labelText: 'Bond inventory',
        border: OutlineInputBorder(),
        isDense: true,
        contentPadding: EdgeInsets.symmetric(horizontal: 8, vertical: 8),
      ),
      isExpanded: true,
      value: value,
      items: [
        const DropdownMenuItem<String?>(
          value: null,
          child: Text('Any'),
        ),
        for (final (label, count) in entries)
          DropdownMenuItem<String?>(
            value: label,
            child: Text('$label  ($count)', overflow: TextOverflow.ellipsis),
          ),
      ],
      onChanged: onChanged,
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
