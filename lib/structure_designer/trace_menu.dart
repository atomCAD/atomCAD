/// The contributing-nodes menu of a right click in the viewport
/// (`doc/design_output_driven_editing.md`, Flutter → Menu).
///
/// One row per node the kernel traced the thing under the cursor back to,
/// downstream first (the displayed node first, where it was made last), each
/// with what the node did to it
/// ("filled", "moved", "shape", "passed", "possibly contributed", "trace stops
/// here", "wired upstream"). The rows come pre-ordered and pre-labelled from
/// Rust (`APITraceResult`); nothing here re-derives a label or an order.
///
/// **The tree is custom-node instances only.** A row followed by rows one
/// `depth` deeper is an instance, and those are the nodes traced inside its
/// network; each instance starts collapsed and has a chevron. Clicking an
/// instance row lands on the instance where it is; clicking one of its
/// children lands inside the custom network (the caller's `onSelect` does the
/// jump — the row's `hostNetwork` already says which).
///
/// **Wired-upstream runs** (`doc/design_trace_coverage.md`, Menu). A maximal
/// run of consecutive `unreached` siblings (rows at one tree level under one
/// parent) is folded into one summary line, "N wired upstream", that starts
/// collapsed. The line is not a node: it highlights nothing and clicking it
/// only folds or unfolds the run. Unfolded, it stays as a header and the run's
/// rows follow at their own depth, not one step further in.
///
/// Every row highlights under the pointer, and [TraceMenu.onHover] reports
/// the row (null when the pointer leaves it) so the host can highlight the
/// node in the node network editor.
///
/// The widget takes plain data and callbacks and is unit-tested in
/// `test/trace_menu_test.dart`.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Whether row [index] is a custom-node instance with traced children.
bool traceRowHasChildren(List<APITraceRow> rows, int index) =>
    index + 1 < rows.length && rows[index + 1].depth > rows[index].depth;

/// One line of the menu: row [index], or the summary header of the
/// wired-upstream run whose first row is [index].
class TraceMenuLine {
  final int index;

  /// The run's header line rather than a row.
  final bool isRunHeader;

  /// For a header: the rows the run spans, its siblings and everything
  /// nested under them.
  final int runSize;

  const TraceMenuLine.row(this.index)
      : isRunHeader = false,
        runSize = 0;

  const TraceMenuLine.runHeader(this.index, this.runSize) : isRunHeader = true;
}

bool _isUnreached(APITraceRow row) =>
    row.contribution == APIContributionKind.unreached;

/// The end (exclusive) of the subtree of row [index]: the first later row no
/// deeper than it.
int _subtreeEnd(List<APITraceRow> rows, int index) {
  final depth = rows[index].depth;
  var end = index + 1;
  while (end < rows.length && rows[end].depth > depth) {
    end++;
  }
  return end;
}

/// The previous sibling of row [index] (same depth, same parent), or null.
int? _previousSibling(List<APITraceRow> rows, int index) {
  final depth = rows[index].depth;
  for (var j = index - 1; j >= 0; j--) {
    if (rows[j].depth < depth) return null;
    if (rows[j].depth == depth) return j;
  }
  return null;
}

/// Whether row [index] starts a wired-upstream run: it is `unreached` and its
/// previous sibling is not.
bool traceRowStartsRun(List<APITraceRow> rows, int index) {
  if (!_isUnreached(rows[index])) return false;
  final previous = _previousSibling(rows, index);
  return previous == null || !_isUnreached(rows[previous]);
}

/// The end (exclusive) of the run starting at row [start]: past its last
/// `unreached` sibling's subtree.
int traceRunEnd(List<APITraceRow> rows, int start) {
  final depth = rows[start].depth;
  var end = _subtreeEnd(rows, start);
  while (end < rows.length &&
      rows[end].depth == depth &&
      _isUnreached(rows[end])) {
    end = _subtreeEnd(rows, end);
  }
  return end;
}

/// The lines that show when the instance rows in [collapsed] are folded and
/// only the runs in [expandedRuns] (keyed by their first row) are unfolded.
/// A collapsed instance hides its descendants, header lines of inner runs
/// included; a collapsed run hides its rows, from its first to the end of its
/// last one's subtree.
List<TraceMenuLine> visibleTraceLines(
    List<APITraceRow> rows, Set<int> collapsed, Set<int> expandedRuns) {
  final lines = <TraceMenuLine>[];
  int? hideDeeperThan;
  var skipUntil = 0;
  for (var i = 0; i < rows.length; i++) {
    if (i < skipUntil) continue;
    final depth = rows[i].depth;
    if (hideDeeperThan != null) {
      if (depth > hideDeeperThan) continue;
      hideDeeperThan = null;
    }
    if (traceRowStartsRun(rows, i)) {
      final end = traceRunEnd(rows, i);
      lines.add(TraceMenuLine.runHeader(i, end - i));
      if (!expandedRuns.contains(i)) {
        skipUntil = end;
        continue;
      }
    }
    lines.add(TraceMenuLine.row(i));
    if (collapsed.contains(i) && traceRowHasChildren(rows, i)) {
      hideDeeperThan = depth;
    }
  }
  return lines;
}

/// The indices of [rows] that show (see [visibleTraceLines]; header lines
/// left out).
List<int> visibleTraceRows(List<APITraceRow> rows, Set<int> collapsed,
        [Set<int> expandedRuns = const {}]) =>
    [
      for (final line in visibleTraceLines(rows, collapsed, expandedRuns))
        if (!line.isRunHeader) line.index
    ];

/// `Passed` and `Unknown` rows are dimmed: the node only routed the thing, or
/// cannot say whether it touched it. `Unreached` rows are fainter still
/// ([isFaintContribution]).
bool isDimmedContribution(APIContributionKind kind) =>
    kind == APIContributionKind.passed || kind == APIContributionKind.unknown;

/// `Unreached` rows are listed only because of wiring: one step fainter than
/// the dimmed rows.
bool isFaintContribution(APIContributionKind kind) =>
    kind == APIContributionKind.unreached;

/// A full-overlay popup: a dismiss barrier plus the menu, placed at
/// [position] (global coordinates) and kept inside the overlay.
class TraceMenu extends StatefulWidget {
  final Offset position;
  final List<APITraceRow> rows;

  /// The trace stopped at its row cap: the menu ends with a "… more" line,
  /// since the rows cut off are the farthest upstream.
  final bool truncated;
  final void Function(APITraceRow row) onSelect;
  final VoidCallback onDismiss;

  /// The pointer entered [row] (null: it left the row it was on).
  final void Function(APITraceRow? row)? onHover;

  const TraceMenu({
    super.key,
    required this.position,
    required this.rows,
    required this.truncated,
    required this.onSelect,
    required this.onDismiss,
    this.onHover,
  });

  @override
  State<TraceMenu> createState() => _TraceMenuState();
}

class _TraceMenuState extends State<TraceMenu> {
  /// Folded instance rows. Every instance starts collapsed, so the menu reads
  /// as the chain at the level the user is looking at.
  final Set<int> _collapsed = {};

  /// Unfolded wired-upstream runs, keyed by their first row. Every run starts
  /// collapsed.
  final Set<int> _expandedRuns = {};

  /// The row under the pointer, or null.
  int? _hovered;

  static const double _indentPerDepth = 14.0;

  @override
  void initState() {
    super.initState();
    _resetFolding();
  }

  @override
  void didUpdateWidget(TraceMenu oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.rows, widget.rows)) _resetFolding();
  }

  void _resetFolding() {
    _collapsed
      ..clear()
      ..addAll([
        for (var i = 0; i < widget.rows.length; i++)
          if (traceRowHasChildren(widget.rows, i)) i
      ]);
    _expandedRuns.clear();
  }

  void _toggle(int index) {
    setState(() {
      if (!_collapsed.remove(index)) _collapsed.add(index);
    });
  }

  void _toggleRun(int index) {
    setState(() {
      if (!_expandedRuns.remove(index)) _expandedRuns.add(index);
    });
  }

  void _setHovered(int index, bool hovering) {
    if (hovering) {
      if (_hovered == index) return;
      setState(() => _hovered = index);
      widget.onHover?.call(widget.rows[index]);
    } else if (_hovered == index) {
      // Moving between rows, the old row's exit arrives before the new row's
      // enter, in the same dispatch. Report "left" only if no row was
      // entered by then, so the host does not see a null between two rows.
      setState(() => _hovered = null);
      scheduleMicrotask(() {
        if (mounted && _hovered == null) widget.onHover?.call(null);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Focus(
      autofocus: true,
      onKeyEvent: (node, event) {
        if (event is KeyDownEvent &&
            event.logicalKey == LogicalKeyboardKey.escape) {
          widget.onDismiss();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Stack(
        children: [
          // Barrier: clicking away (either button) dismisses with no change.
          Positioned.fill(
            child: GestureDetector(
              onTap: widget.onDismiss,
              onSecondaryTap: widget.onDismiss,
              behavior: HitTestBehavior.opaque,
              child: const SizedBox.expand(),
            ),
          ),
          CustomSingleChildLayout(
            delegate: _ClampedAnchorLayout(widget.position),
            child: _buildMenu(),
          ),
        ],
      ),
    );
  }

  Widget _buildMenu() {
    final rows = widget.rows;
    return Material(
      elevation: 8,
      borderRadius: BorderRadius.circular(6),
      color: const Color(0xFF2D2D30),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420, maxHeight: 420),
        child: IntrinsicWidth(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Padding(
                padding: EdgeInsets.fromLTRB(12, 8, 12, 4),
                child: Text(
                  'Contributing nodes',
                  style: TextStyle(color: Colors.white54, fontSize: 11),
                ),
              ),
              Flexible(
                child: SingleChildScrollView(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      for (final line
                          in visibleTraceLines(rows, _collapsed, _expandedRuns))
                        line.isRunHeader
                            ? _buildRunHeader(line)
                            : _buildRow(line.index),
                    ],
                  ),
                ),
              ),
              if (widget.truncated)
                const Padding(
                  padding: EdgeInsets.fromLTRB(12, 4, 12, 0),
                  child: Text(
                    '… more',
                    style: TextStyle(
                      color: Colors.white38,
                      fontSize: 12,
                      fontStyle: FontStyle.italic,
                    ),
                  ),
                ),
              const SizedBox(height: 4),
            ],
          ),
        ),
      ),
    );
  }

  /// The summary line of a wired-upstream run: not a node, so it reports no
  /// hover and a click only folds or unfolds the run.
  Widget _buildRunHeader(TraceMenuLine line) {
    final expanded = _expandedRuns.contains(line.index);
    return InkWell(
      key: ValueKey('trace_run_${line.index}'),
      onTap: () => _toggleRun(line.index),
      hoverColor: Colors.white.withValues(alpha: 0.06),
      child: Padding(
        padding: const EdgeInsets.only(left: 6, right: 12, top: 4, bottom: 4),
        child: Row(
          children: [
            SizedBox(width: widget.rows[line.index].depth * _indentPerDepth),
            SizedBox(
              width: 20,
              child: Icon(
                expanded ? Icons.expand_more : Icons.chevron_right,
                size: 16,
                color: Colors.white38,
              ),
            ),
            Expanded(
              child: Text(
                '${line.runSize} wired upstream',
                style: const TextStyle(
                  color: Colors.white38,
                  fontSize: 12,
                  fontStyle: FontStyle.italic,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildRow(int index) {
    final row = widget.rows[index];
    final dimmed = isDimmedContribution(row.contribution);
    final faint = isFaintContribution(row.contribution);
    final hasChildren = traceRowHasChildren(widget.rows, index);
    final collapsed = _collapsed.contains(index);
    return InkWell(
      key: ValueKey('trace_row_$index'),
      onTap: () => widget.onSelect(row),
      onHover: (hovering) => _setHovered(index, hovering),
      hoverColor: Colors.white.withValues(alpha: 0.12),
      child: Padding(
        padding: const EdgeInsets.only(left: 6, right: 12, top: 4, bottom: 4),
        child: Row(
          children: [
            SizedBox(width: row.depth * _indentPerDepth),
            // A fixed slot whether or not there is a chevron, so the names
            // line up and a row never changes height on hover.
            SizedBox(
              width: 20,
              child: hasChildren
                  ? GestureDetector(
                      key: ValueKey('trace_chevron_$index'),
                      onTap: () => _toggle(index),
                      behavior: HitTestBehavior.opaque,
                      child: Icon(
                        collapsed ? Icons.chevron_right : Icons.expand_more,
                        size: 16,
                        color: Colors.white70,
                      ),
                    )
                  : null,
            ),
            Expanded(
              child: Text(
                row.nodeLabel,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: faint
                      ? Colors.white38
                      : (dimmed ? Colors.white54 : Colors.white),
                  fontSize: 13,
                ),
              ),
            ),
            const SizedBox(width: 24),
            Text(
              row.label,
              style: TextStyle(
                color: faint
                    ? Colors.white24
                    : (dimmed ? Colors.white38 : Colors.lightBlueAccent),
                fontSize: 12,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Places the menu's top-left at the anchor, shifted back inside the overlay
/// when it would run off an edge.
class _ClampedAnchorLayout extends SingleChildLayoutDelegate {
  final Offset anchor;

  _ClampedAnchorLayout(this.anchor);

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      constraints.loosen();

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final maxX = (size.width - childSize.width).clamp(0.0, double.infinity);
    final maxY = (size.height - childSize.height).clamp(0.0, double.infinity);
    return Offset(anchor.dx.clamp(0.0, maxX), anchor.dy.clamp(0.0, maxY));
  }

  @override
  bool shouldRelayout(_ClampedAnchorLayout oldDelegate) =>
      oldDelegate.anchor != anchor;
}
