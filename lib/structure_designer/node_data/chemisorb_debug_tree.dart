import 'package:flutter/material.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// The `chemisorb` node's **search tree** — the debug view's panel half
/// (`design_chemisorption_sequential.md` §6.5).
///
/// Every path the search took is a row: the root (the posed molecule), the
/// leg-1 foot, then one leg per level. Clicking a row selects it for the
/// node's `debug` and `debug_shapes` pins; the kernel builds the view (at
/// once when it is geometry alone, as a node job when a relaxed row has to be
/// replayed), so this widget only lists and asks.
///
/// Three rules worth keeping:
///
/// - **Children are loaded lazily**: a row's children are fetched when it is
///   expanded, so a tree of 10⁵ rows opens at once. What was fetched is
///   dropped when the tree changes ([treeKey], the input fingerprint, or
///   [searched]); row numbers hold while the fingerprint does — a Run only
///   appends the local rows — so the expansion survives a Run.
/// - **Duplicates are hidden by default.** A change set reached in several
///   binding orders is listed once, under the path the search kept; a parent
///   says how many it hides, and **Show duplicates** lists them as
///   "= duplicate of #N" rows that jump to the canonical one. A display option
///   only: not saved, not an undo step, and the tree is the same either way.
/// - **The selection is the kernel's** ([selectedRow] / [selectedForm], read
///   back from the report), so it survives this widget being rebuilt or
///   replaced; the expansion is this widget's own state.
///
/// Takes plain data and callbacks rather than the model, so
/// `test/chemisorb_debug_tree_test.dart` pumps it without the Rust library.
class ChemisorbDebugTree extends StatefulWidget {
  /// The input fingerprint of the tree; `null` before an evaluation.
  final BigInt? treeKey;

  /// The tree is a run's (it has relaxations and the local rows).
  final bool searched;

  /// The row the debug pins show, `null` for the root view.
  final int? selectedRow;
  final APIChemisorbDebugForm? selectedForm;

  /// Whether the `debug` / `debug_shapes` pins are displayed.
  final bool debugShown;
  final bool shapesShown;

  final APIChemisorbDebugRow? Function(int row) fetchRow;
  final List<APIChemisorbDebugRow> Function(int row, bool showDuplicates)
      fetchChildren;
  final List<int> Function(int row) fetchAncestors;

  /// Selects a row; `form` = `null` opens it on its default form.
  final void Function(int row, APIChemisorbDebugForm? form) onSelect;
  final VoidCallback onToggleDebug;
  final VoidCallback onToggleShapes;

  const ChemisorbDebugTree({
    super.key,
    required this.treeKey,
    required this.searched,
    required this.selectedRow,
    required this.selectedForm,
    required this.debugShown,
    required this.shapesShown,
    required this.fetchRow,
    required this.fetchChildren,
    required this.fetchAncestors,
    required this.onSelect,
    required this.onToggleDebug,
    required this.onToggleShapes,
  });

  @override
  State<ChemisorbDebugTree> createState() => _ChemisorbDebugTreeState();
}

/// The height of the scrolling row list.
const double DEBUG_TREE_HEIGHT = 300;

/// Indentation per tree level.
const double DEBUG_TREE_INDENT = 14;

/// The debug view's marking colours, as the kernel paints them
/// (`sequential::debug`), for the legend.
const List<(String, Color)> DEBUG_LEGEND = [
  ('bonded', Color.fromARGB(255, 255, 140, 0)),
  ('foot', Color.fromARGB(255, 191, 77, 255)),
  ('accepted', Color.fromARGB(255, 26, 204, 77)),
  ('near miss', Color.fromARGB(255, 255, 217, 26)),
  ('mirrored', Color.fromARGB(255, 77, 115, 255)),
  ('undecided', Color.fromARGB(255, 51, 217, 242)),
  ('clash', Color.fromARGB(255, 242, 26, 38)),
];

class _ChemisorbDebugTreeState extends State<ChemisorbDebugTree> {
  bool _showDuplicates = false;
  final Set<int> _expanded = {0};
  final Map<int, List<APIChemisorbDebugRow>> _children = {};
  APIChemisorbDebugRow? _root;

  @override
  void didUpdateWidget(ChemisorbDebugTree old) {
    super.didUpdateWidget(old);
    if (old.treeKey != widget.treeKey) {
      // Other inputs: other rows. Start from the root again.
      _expanded
        ..clear()
        ..add(0);
      _drop();
    } else if (old.searched != widget.searched) {
      // A run: the same rows, with relaxations and the local phase.
      _drop();
    }
  }

  void _drop() {
    _children.clear();
    _root = null;
  }

  List<APIChemisorbDebugRow> _childrenOf(int row) => _children.putIfAbsent(
      row, () => widget.fetchChildren(row, _showDuplicates));

  /// The rows on screen, depth first, with their depth.
  List<(APIChemisorbDebugRow, int)> _visibleRows() {
    final root = _root ??= widget.fetchRow(0);
    if (root == null) return const [];
    final out = <(APIChemisorbDebugRow, int)>[];
    void walk(APIChemisorbDebugRow row, int depth) {
      out.add((row, depth));
      if (_expanded.contains(row.row)) {
        for (final c in _childrenOf(row.row)) {
          walk(c, depth + 1);
        }
      }
    }

    walk(root, 0);
    return out;
  }

  bool _expandable(APIChemisorbDebugRow row) =>
      row.duplicateOf == null &&
      (row.children > 0 || (_showDuplicates && row.hiddenDuplicates > 0));

  void _toggle(APIChemisorbDebugRow row) {
    setState(() {
      if (!_expanded.remove(row.row)) _expanded.add(row.row);
    });
  }

  void _tap(APIChemisorbDebugRow row) {
    final canonical = row.duplicateOf;
    if (canonical == null) {
      widget.onSelect(row.row, null);
      return;
    }
    // A duplicate jumps to the path the search kept.
    final path = widget.fetchAncestors(canonical);
    setState(() {
      _expanded.addAll(path.take(path.length - 1));
    });
    widget.onSelect(canonical, null);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final small = theme.textTheme.bodySmall;
    final rows = widget.treeKey == null
        ? const <(APIChemisorbDebugRow, int)>[]
        : _visibleRows();
    final selected = widget.selectedRow;
    final selectedRow = selected == null ? null : widget.fetchRow(selected);
    return Card(
      elevation: 1,
      child: Padding(
        padding: const EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Search tree', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            Wrap(
              spacing: 6,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                FilterChip(
                  label: const Text('debug pin'),
                  selected: widget.debugShown,
                  onSelected: (_) => widget.onToggleDebug(),
                  visualDensity: VisualDensity.compact,
                ),
                FilterChip(
                  label: const Text('shapes pin'),
                  selected: widget.shapesShown,
                  onSelected: (_) => widget.onToggleShapes(),
                  visualDensity: VisualDensity.compact,
                ),
                FilterChip(
                  label: const Text('Show duplicates'),
                  selected: _showDuplicates,
                  onSelected: (value) => setState(() {
                    _showDuplicates = value;
                    _children.clear();
                  }),
                  visualDensity: VisualDensity.compact,
                ),
              ],
            ),
            const SizedBox(height: 6),
            if (rows.isEmpty)
              Text('No evaluation yet — display the node.', style: small)
            else
              SizedBox(
                height: DEBUG_TREE_HEIGHT,
                child: ListView.builder(
                  itemCount: rows.length,
                  itemExtent: 24,
                  itemBuilder: (context, i) {
                    final (row, depth) = rows[i];
                    return _RowTile(
                      row: row,
                      depth: depth,
                      expanded: _expanded.contains(row.row),
                      expandable: _expandable(row),
                      selected: row.row == selected,
                      showDuplicates: _showDuplicates,
                      onToggle: () => _toggle(row),
                      onTap: () => _tap(row),
                    );
                  },
                ),
              ),
            const SizedBox(height: 6),
            _SelectionLine(
              row: selectedRow,
              form: widget.selectedForm,
              onForm: (form) => widget.onSelect(selected!, form),
              onRoot: selected == null ? null : () => widget.onSelect(0, null),
            ),
            const SizedBox(height: 6),
            Wrap(
              spacing: 10,
              runSpacing: 2,
              children: [
                for (final (name, color) in DEBUG_LEGEND)
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        width: 9,
                        height: 9,
                        decoration:
                            BoxDecoration(color: color, shape: BoxShape.circle),
                      ),
                      const SizedBox(width: 3),
                      Text(name, style: small),
                    ],
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// What a row's children are called, by the level that finds them.
String _levelName(APIChemisorbDebugRow row) {
  switch (row.kind) {
    case APIChemisorbDebugRowKind.root:
      return 'feet';
    case APIChemisorbDebugRowKind.foot:
      return 'anchor';
    case APIChemisorbDebugRowKind.leg:
      return switch (row.legs) { 1 => 'sphere', 2 => 'ring', _ => 'reach' };
  }
}

/// The counts beside a row's label, most telling first.
String debugRowSummary(APIChemisorbDebugRow row,
    {bool showDuplicates = false}) {
  if (row.duplicateOf != null) return '= duplicate of #${row.duplicateOf}';
  final parts = <String>[];
  if (row.strain != null) {
    parts.add('strain ${formatNatural(row.strain!, 4)}'
        '${row.converged ? '' : ' unconv.'}');
  }
  if (row.mirror == APIChemisorbMirror.mirrored) parts.add('mirrored');
  if (row.prunedClash) {
    parts.add('clash, pruned');
  } else if (row.seatingClash) {
    parts.add('clash');
  }
  if (row.budgetCut) parts.add('budget cut');
  final accepted = row.candidates + row.mirrored + row.undecided + row.clashes;
  if (row.kind == APIChemisorbDebugRowKind.root) {
    parts.add('${row.children} feet');
  } else if (accepted > 0 || row.nearMisses.isNotEmpty) {
    final level = StringBuffer('${_levelName(row)}: $accepted');
    if (row.mirrored > 0) level.write(', ${row.mirrored} mirr.');
    if (row.undecided > 0) level.write(', ${row.undecided} undec.');
    if (row.clashes > 0) level.write(', ${row.clashes} clash');
    if (row.nearMisses.isNotEmpty) {
      level.write(', ${row.nearMisses.length} near');
    }
    parts.add(level.toString());
  }
  if (!showDuplicates && row.hiddenDuplicates > 0) {
    parts.add('${row.hiddenDuplicates} dup. hidden');
  }
  return parts.join(' · ');
}

/// Everything about a row, for its tooltip.
String debugRowDetails(APIChemisorbDebugRow row) {
  final lines = <String>[
    '#${row.row}  ${row.label}',
    'path: ${row.path}',
  ];
  if (row.candidate) lines.add('a candidate');
  if (row.localParent) lines.add('a parent of the local phase');
  if (row.mirror != null) lines.add('mirror check: ${row.mirror!.name}');
  final rejected = [
    if (row.rejectedValence > 0) '${row.rejectedValence} valence',
    if (row.rejectedNoAcceptor > 0) '${row.rejectedNoAcceptor} no H acceptor',
    if (row.rejectedFilter > 0) '${row.rejectedFilter} inventory',
  ];
  if (rejected.isNotEmpty) lines.add('rejected: ${rejected.join(', ')}');
  if (row.nearMisses.isNotEmpty) {
    lines.add('near misses (Å past the bound): ${row.nearMisses.join(', ')}');
  }
  return lines.join('\n');
}

class _RowTile extends StatelessWidget {
  final APIChemisorbDebugRow row;
  final int depth;
  final bool expanded;
  final bool expandable;
  final bool selected;
  final bool showDuplicates;
  final VoidCallback onToggle;
  final VoidCallback onTap;

  const _RowTile({
    required this.row,
    required this.depth,
    required this.expanded,
    required this.expandable,
    required this.selected,
    required this.showDuplicates,
    required this.onToggle,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final small = theme.textTheme.bodySmall;
    final dim = row.duplicateOf != null ||
        row.mirror == APIChemisorbMirror.mirrored ||
        row.prunedClash;
    return Tooltip(
      message: debugRowDetails(row),
      waitDuration: const Duration(milliseconds: 600),
      child: Material(
        color:
            selected ? theme.colorScheme.primaryContainer : Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: Padding(
            padding: EdgeInsets.only(left: depth * DEBUG_TREE_INDENT),
            child: Row(
              children: [
                SizedBox(
                  width: 20,
                  child: expandable
                      ? InkWell(
                          onTap: onToggle,
                          child: Icon(
                            expanded ? Icons.expand_more : Icons.chevron_right,
                            size: 16,
                          ),
                        )
                      : null,
                ),
                Text(
                  row.kind == APIChemisorbDebugRowKind.leg && depth > 2
                      ? '+ ${row.label}'
                      : row.label,
                  style: small?.copyWith(
                    fontFamily: 'monospace',
                    color: dim ? theme.disabledColor : null,
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    debugRowSummary(row, showDuplicates: showDuplicates),
                    style: small?.copyWith(color: theme.hintColor),
                    maxLines: 1,
                    softWrap: false,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The selected row under the tree, its seated / relaxed switch when it has
/// both forms, and a way back to the root view.
class _SelectionLine extends StatelessWidget {
  final APIChemisorbDebugRow? row;
  final APIChemisorbDebugForm? form;
  final ValueChanged<APIChemisorbDebugForm> onForm;
  final VoidCallback? onRoot;

  const _SelectionLine({
    required this.row,
    required this.form,
    required this.onForm,
    required this.onRoot,
  });

  @override
  Widget build(BuildContext context) {
    final small = Theme.of(context).textTheme.bodySmall;
    final row = this.row;
    if (row == null) {
      return Text('Showing the root: every foot and its anchor sites.',
          style: small);
    }
    return Row(
      children: [
        Expanded(
          child: Text(
            'Showing #${row.row} ${row.label}, ${form?.name ?? ''}',
            style: small,
            overflow: TextOverflow.ellipsis,
          ),
        ),
        if (row.canSeat && row.canRelax)
          SegmentedButton<APIChemisorbDebugForm>(
            segments: const [
              ButtonSegment(
                  value: APIChemisorbDebugForm.seated, label: Text('Seated')),
              ButtonSegment(
                  value: APIChemisorbDebugForm.relaxed, label: Text('Relaxed')),
            ],
            selected: {if (form != null) form!},
            emptySelectionAllowed: true,
            showSelectedIcon: false,
            style: const ButtonStyle(visualDensity: VisualDensity.compact),
            onSelectionChanged: (s) {
              if (s.isNotEmpty) onForm(s.first);
            },
          ),
        if (onRoot != null)
          IconButton(
            tooltip: 'Back to the root view',
            icon: const Icon(Icons.home_outlined, size: 18),
            visualDensity: VisualDensity.compact,
            onPressed: onRoot,
          ),
      ],
    );
  }
}
