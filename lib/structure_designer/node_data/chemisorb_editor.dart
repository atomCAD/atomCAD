import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/chemisorb_api.dart'
    as chemisorb_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/chemisorb_debug_tree.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_jobs.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor for the `chemisorb` node — the ways a posed adsorbate can bond to a
/// substrate, found leg by leg, relaxed and ranked
/// (`design_chemisorption_sequential.md`).
///
/// Every setting is a search setting: changing any of them makes a stored
/// result stale. The formed-bond and bond-inventory filters choose which
/// hypotheses are candidates, so a run relaxes only that group; `top_n` and
/// the energy window choose what is kept. (A split into "filters after
/// search" existed and was removed — it was arbitrary and forced every
/// relaxed structure to be kept.)
///
/// The three distances are one per phase of the search: `anchor_reach` for
/// leg 1, `tolerance` for legs 2 and 3, `reach` for legs 4 and later and for
/// the H transfer rule. `reach` stays visible but greyed while nothing reads
/// it (the report's `reachUsed`, decided Rust-side) — a value a fourth foot or
/// a transfer record makes live again unchanged.
///
/// The search runs only when **Run** is pressed — in the background, as a
/// node job (`doc/design_background_node_jobs.md`), with progress and Cancel
/// in [ChemisorbRunRow]; until then, and whenever the inputs or settings
/// change after a run, the node outputs the *plan* and the report says how
/// many hypotheses a run would relax. The report (statistics,
/// warnings and the ranked candidates) lives in the selected node's eval cache
/// and is re-read on every model notification, as `proxy_editor.dart` does.
///
/// The **search tree** under the statistics ([ChemisorbDebugTree]) is the
/// debug view (design §6.5): a click selects a row for the node's `debug` and
/// `debug_shapes` pins, through `model.chemisorbDebugSelect`; the chips beside
/// it show and hide those two pins.
class ChemisorbEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIChemisorbData? data;
  final StructureDesignerModel model;

  /// Whether the `transfers` pin is wired; only the hint under the settings
  /// reads it.
  final bool transfersConnected;

  /// Whether the `debug` (pin 2) and `debug_shapes` (pin 3) outputs are
  /// displayed.
  final bool debugShown;
  final bool shapesShown;

  const ChemisorbEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
    required this.transfersConnected,
    this.debugShown = false,
    this.shapesShown = false,
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
    double? anchorReach,
    double? tolerance,
    double? reach,
    bool? clashFilter,
    int? maxFormedBonds,
    Object? formedBonds = _keep,
    Object? bondInventory = _keep,
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
        anchorReach: anchorReach ?? data.anchorReach,
        tolerance: tolerance ?? data.tolerance,
        reach: reach ?? data.reach,
        clashFilter: clashFilter ?? data.clashFilter,
        maxFormedBonds: maxFormedBonds ?? data.maxFormedBonds,
        formedBonds: identical(formedBonds, _keep)
            ? data.formedBonds
            : formedBonds as int?,
        bondInventory: identical(bondInventory, _keep)
            ? data.bondInventory
            : bondInventory as String?,
        topN: topN ?? data.topN,
        energyWindow: energyWindow ?? data.energyWindow,
        budget: budget ?? data.budget,
        maxIterations: maxIterations ?? data.maxIterations,
      ),
    );
  }

  /// **Run**: starts the search as a node job on a worker thread
  /// (`doc/design_background_node_jobs.md`). The UI stays live; progress
  /// arrives on `model.nodeJobs`, and the outcome is reported by the host
  /// (`structure_designer.dart`) — this panel may be gone by then.
  void _run() {
    final error = widget.model.startNodeJob(widget.nodeId);
    if (error != null && mounted) {
      showErrorSnackBar(context, 'Chemisorption search did not start: $error');
    }
  }

  /// Selects a search-tree item for the debug pins; a refusal (a job already
  /// runs on the node, a row with no relaxation) is reported here.
  void _select(APIChemisorbDebugRef item, APIChemisorbDebugForm? form) {
    final error =
        widget.model.chemisorbDebugSelect(widget.nodeId, item, form: form);
    if (error != null && mounted) {
      showErrorSnackBar(context, 'Debug view: $error');
    }
  }

  void _togglePin(int pin) {
    widget.model.toggleOutputPinDisplay(widget.nodeId, pin,
        scopeChain: widget.model.propertyEditorScopeChain);
  }

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }
    // Greyed only once a report says nothing reads it.
    final reachUsed = _report?.reachUsed ?? true;

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
          // Rebuilt by the 10 Hz job poll alone, not by the model.
          ValueListenableBuilder<List<APINodeJobStatus>>(
            valueListenable: widget.model.nodeJobs,
            builder: (context, _, __) {
              final job = widget.model
                  .jobFor(widget.model.propertyEditorScopeChain, widget.nodeId);
              return ChemisorbRunRow(
                report: _report,
                job: job,
                onRun: _run,
                onCancel: job == null
                    ? null
                    : () => widget.model.cancelNodeJob(job.jobId),
              );
            },
          ),
          const SizedBox(height: 12),

          const _SectionHeader(
            title: 'Search settings',
            note: 'Changing any of these needs a new Run.',
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
            label: 'Anchor reach, leg 1 (Å)',
            value: data.anchorReach,
            onChanged: (v) => _commit(anchorReach: v),
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Tolerance, legs 2–3 (Å)',
            value: data.tolerance,
            onChanged: (v) => _commit(tolerance: v),
          ),
          const SizedBox(height: 8),
          Opacity(
            opacity: reachUsed ? 1.0 : 0.5,
            child: IgnorePointer(
              ignoring: !reachUsed,
              child: FloatInput(
                label: 'Reach, legs 4+ and H transfer (Å)',
                value: data.reach,
                onChanged: (v) => _commit(reach: v),
              ),
            ),
          ),
          CheckboxListTile(
            title: const Text('Prune seatings that clash'),
            value: data.clashFilter,
            onChanged: (value) => _commit(clashFilter: value ?? true),
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
          _CapField(
            checkboxLabel: 'Limit legs',
            fieldLabel: 'Max legs (bonds formed)',
            value: data.maxFormedBonds,
            onChanged: (v) => _commit(maxFormedBonds: v),
          ),
          if (!widget.transfersConnected)
            Padding(
              padding: const EdgeInsets.only(top: 4.0),
              child: Text(
                'Wire `transfers` to let an OH foot give its H to a site.',
                style: Theme.of(context)
                    .textTheme
                    .bodySmall
                    ?.copyWith(fontStyle: FontStyle.italic),
              ),
            ),
          const SizedBox(height: 12),

          // ---- what to search for: pruned before anything is relaxed -----
          CheckboxListTile(
            title: const Text('Only an exact number of legs'),
            value: data.formedBonds != null,
            onChanged: (value) => _commit(
              formedBonds: (value ?? false) ? _DEFAULT_FORMED_BONDS : null,
            ),
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
          if (data.formedBonds != null)
            Padding(
              padding: const EdgeInsets.only(left: 16.0, bottom: 8.0),
              child: IntInput(
                label: 'Legs (exactly)',
                value: data.formedBonds!,
                minimumValue: 1,
                onChanged: (v) => _commit(formedBonds: v),
              ),
            ),
          _InventoryDropdown(
            value: data.bondInventory,
            options: _report?.inventoryOptions ?? const [],
            onChanged: (v) => _commit(bondInventory: v),
          ),
          const SizedBox(height: 12),

          // ---- what to keep: applied while relaxing ----------------------
          IntInput(
            label: 'Top N (candidates kept)',
            value: data.topN,
            minimumValue: 1,
            onChanged: (v) => _commit(topN: v),
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Energy window (kcal/mol above the best)',
            value: data.energyWindow,
            onChanged: (v) => _commit(energyWindow: v),
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
          const SizedBox(height: 16),

          // ---- the report -------------------------------------------------
          _StatsCard(report: _report),
          const SizedBox(height: 12),
          ChemisorbDebugTree(
            treeKey: _report?.debugTreeKey,
            searched: _report?.stats.searched ?? false,
            selected: _report?.debugSelected,
            selectedForm: _report?.debugSelectedForm,
            debugShown: widget.debugShown,
            shapesShown: widget.shapesShown,
            fetchRow: (item) => chemisorb_api.getChemisorbDebugRow(item: item),
            fetchChildren: (item, showDuplicates, showMirrored) =>
                chemisorb_api.getChemisorbDebugChildren(
                    item: item,
                    showDuplicates: showDuplicates,
                    showMirrored: showMirrored),
            fetchAncestors: (item) =>
                chemisorb_api.getChemisorbDebugAncestors(item: item),
            onSelect: _select,
            onToggleDebug: () => _togglePin(_DEBUG_PIN),
            onToggleShapes: () => _togglePin(_DEBUG_SHAPES_PIN),
          ),
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
///
/// While a search runs ([job] is set) it is a progress bar — indeterminate
/// until the plan has told how many relaxations there are — the progress in
/// words and **Cancel** in place of Run; once cancel was asked for, the
/// button is disabled and the text says "Cancelling…" until the worker
/// returns. The settings above stay editable meanwhile (D3): a result that
/// lands after one changed simply shows as stale.
///
/// Takes the job status and callbacks rather than the model, so
/// `test/node_jobs_test.dart` pumps it without the Rust library.
class ChemisorbRunRow extends StatelessWidget {
  final APIChemisorbReport? report;

  /// The job running on this node, or `null`.
  final APINodeJobStatus? job;
  final VoidCallback onRun;

  /// Cancels [job]; unused while no job runs.
  final VoidCallback? onCancel;

  const ChemisorbRunRow({
    super.key,
    required this.report,
    required this.job,
    required this.onRun,
    required this.onCancel,
  });

  @override
  Widget build(BuildContext context) {
    final job = this.job;
    if (job != null) return _buildRunning(context, job);

    final theme = Theme.of(context);
    final stats = report?.stats;
    final String status;
    if (stats == null) {
      status = 'Display the node to see the plan.';
    } else if (stats.searched) {
      status = 'Searched: ${stats.relaxed} relaxed '
          'in ${formatNatural(stats.seconds, 3)} s.';
    } else {
      status = '${stats.toRelax} relaxations'
          '${stats.localPhase ? ' + local phase' : ''}'
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
              'Not run: no candidates yet. Results are not saved with the '
              'file.',
              style: theme.textTheme.bodySmall,
            ),
          ),
      ],
    );
  }

  Widget _buildRunning(BuildContext context, APINodeJobStatus job) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            OutlinedButton.icon(
              onPressed: job.cancelling ? null : onCancel,
              icon: const Icon(Icons.stop),
              label: const Text('Cancel'),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Text(nodeJobProgressText(job),
                  style: theme.textTheme.bodySmall),
            ),
          ],
        ),
        const SizedBox(height: 6),
        LinearProgressIndicator(value: nodeJobFraction(job)),
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
              _Row('Feet / sites', '${stats.feet} / ${stats.sites}'),
              _Row('Legs 1 / 2 / 3',
                  '${stats.anchors} / ${stats.spherePairs} / ${stats.torusTriples}'),
              _Row('Candidates (legs 1–3)', '${stats.candidates}'),
              if (stats.parents > BigInt.zero)
                _Row('Parents of the local phase', '${stats.parents}'),
              _Row('Duplicates', '${stats.duplicates}'),
              _Row('Pruned: valence', '${stats.prunedValence}'),
              if (stats.prunedNoAcceptor > BigInt.zero)
                _Row('Pruned: no H acceptor', '${stats.prunedNoAcceptor}'),
              if (stats.prunedFilter > BigInt.zero)
                _Row('Pruned: inventory', '${stats.prunedFilter}'),
              _Row('Mirror: pruned / undecided',
                  '${stats.prunedMirror} / ${stats.mirrorUndecided}'),
              _Row('Seating clashes / pruned',
                  '${stats.seatingClashes} / ${stats.prunedClash}'),
              _Row(
                  'To relax',
                  '${stats.toRelax}'
                      '${stats.localPhase ? ' + local phase' : ''}'),
              for (final level in stats.local)
                _Row(
                    'Leg ${level.legs}: hypotheses / relaxed',
                    '${level.hypotheses} / ${level.relaxed}'
                        '${level.truncated ? ' (cut)' : ''}'),
              _Row('Relaxed', '${stats.relaxed}'),
              _Row('Unconverged', '${stats.unconverged}'),
              if (stats.searched)
                _Row('Kept (top N / window)', '${stats.listed}'),
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
/// separated state: the adsorbate and the substrate each relaxed alone). The bond inventory is shown under
/// every row because strains of different inventories do not compare cleanly.
/// The line under the title says how many relaxed candidates top N and the
/// window dropped.
class _CandidatesCard extends StatelessWidget {
  final APIChemisorbReport report;

  const _CandidatesCard({required this.report});

  /// What top N and the window dropped, in one line.
  static String _hiddenLine(APIChemisorbStats s) {
    final dropped = s.relaxed - s.listed;
    return dropped > BigInt.zero
        ? 'Not kept: $dropped past top N / energy window.'
        : '';
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
              Text('Nothing was relaxed.', style: theme.textTheme.bodySmall)
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
                        '${row.converged ? '' : '  unconv.'}'
                        '${row.seatingClash ? '  clash' : ''}',
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
/// is ticked. The node stores "no cap" as [_NO_CAP] (`-1`); that encoding
/// stays out of the panel. Ticking seeds [_DEFAULT_CAP].
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
              minimumValue: 1,
              onChanged: onChanged,
            ),
          ),
      ],
    );
  }
}

/// The `debug` and `debug_shapes` output pins.
const int _DEBUG_PIN = 2;
const int _DEBUG_SHAPES_PIN = 3;

/// How the node stores "no cap" for `max_formed_bonds`.
const int _NO_CAP = -1;

/// The value a cap takes when its checkbox is first ticked: a binding of fewer
/// than two legs is listed only for a one-foot adsorbate.
const int _DEFAULT_CAP = 3;

/// The `formed bonds` filter's value when it is first ticked.
const int _DEFAULT_FORMED_BONDS = 2;

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

/// The bond-inventory filter: "any", or one of the inventories the search can
/// produce (from the plan without this filter), each with how many relaxations
/// it would take. A stored choice the plan does not contain stays selectable,
/// with a count of 0, so the dropdown never silently drops it.
///
/// Inventory labels are long ("formed 3× H–Si, 2× O–Si; broken 3× H–O") and
/// the menu cannot be wider than the panel, so an open-menu row is
/// [ChemisorbInventoryOptionRow]: the label wrapped at its `;`, the count in a
/// column of its own that is never cut. The closed field stays one line.
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
    final small = Theme.of(context).textTheme.bodySmall;
    return DropdownButtonFormField<String?>(
      key: ValueKey('chemisorb_bonds_filter_$value'),
      decoration: const InputDecoration(
        labelText: 'Bond inventory',
        border: OutlineInputBorder(),
        isDense: true,
        contentPadding: EdgeInsets.symmetric(horizontal: 8, vertical: 8),
      ),
      isExpanded: true,
      // Rows as tall as their content: a wrapped label takes two lines.
      itemHeight: null,
      value: value,
      // The closed field: one line, the count still outside the ellipsis.
      selectedItemBuilder: (context) => [
        const Align(alignment: Alignment.centerLeft, child: Text('Any')),
        for (final (label, count) in entries)
          Row(
            children: [
              Expanded(
                child: Text(label,
                    maxLines: 1,
                    softWrap: false,
                    overflow: TextOverflow.ellipsis),
              ),
              const SizedBox(width: 8),
              Text('$count to relax', style: small),
            ],
          ),
      ],
      items: [
        const DropdownMenuItem<String?>(
          value: null,
          child: Text('Any'),
        ),
        for (final (label, count) in entries)
          DropdownMenuItem<String?>(
            value: label,
            child: ChemisorbInventoryOptionRow(label: label, count: count),
          ),
      ],
      onChanged: onChanged,
    );
  }
}

/// A bond inventory label with a line break after each `;` — the formed and
/// the broken bonds on lines of their own.
String wrapInventoryLabel(String label) => label.replaceAll('; ', ';\n');

/// The most lines an inventory label takes in the open menu.
const int INVENTORY_LABEL_MAX_LINES = 2;

/// One row of the open bond-inventory menu: the label (wrapped by
/// [wrapInventoryLabel], at most [INVENTORY_LABEL_MAX_LINES] lines) and, in a
/// right-aligned column of its own, the number of relaxations it would take —
/// the figure the choice is made on, so it is never what gets cut. A label
/// too long even for that ends in "…", and only then carries a tooltip with
/// the whole text.
class ChemisorbInventoryOptionRow extends StatelessWidget {
  final String label;
  final BigInt count;

  const ChemisorbInventoryOptionRow({
    super.key,
    required this.label,
    required this.count,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final small = theme.textTheme.bodySmall;
    final wrapped = wrapInventoryLabel(label);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          Expanded(
            child: LayoutBuilder(builder: (context, constraints) {
              final style = DefaultTextStyle.of(context).style;
              final text = Text(
                wrapped,
                maxLines: INVENTORY_LABEL_MAX_LINES,
                overflow: TextOverflow.ellipsis,
              );
              final painter = TextPainter(
                text: TextSpan(text: wrapped, style: style),
                maxLines: INVENTORY_LABEL_MAX_LINES,
                textDirection: Directionality.of(context),
                textScaler: MediaQuery.textScalerOf(context),
              )..layout(maxWidth: constraints.maxWidth);
              final cut = painter.didExceedMaxLines;
              painter.dispose();
              return cut ? Tooltip(message: label, child: text) : text;
            }),
          ),
          const SizedBox(width: 12),
          Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('$count',
                  style: theme.textTheme.bodyMedium
                      ?.copyWith(fontWeight: FontWeight.w600)),
              Text('to relax', style: small),
            ],
          ),
        ],
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
