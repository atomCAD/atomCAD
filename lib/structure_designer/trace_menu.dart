/// The contributing-nodes menu of a right click in the viewport
/// (`doc/design_output_driven_editing.md`, Flutter → Menu).
///
/// One row per node the kernel traced the thing under the cursor back to, in
/// build-up order (where it was made first, the displayed node last), each
/// with what the node did to it
/// ("filled", "moved", "shape", "passed", "possibly contributed"). The rows
/// come pre-ordered and pre-labelled from Rust (`APITraceResult`); nothing here
/// re-derives a label or an order.
///
/// **The tree is custom-node instances only.** A row followed by rows one
/// `depth` deeper is an instance, and those are the nodes traced inside its
/// network; each instance starts expanded and has a chevron. Clicking an
/// instance row lands on the instance where it is; clicking one of its
/// children lands inside the custom network (the caller's `onSelect` does the
/// jump — the row's `hostNetwork` already says which).
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

/// The indices of [rows] that show when the instance rows in [collapsed] are
/// folded: a collapsed row's descendants (the rows after it that are deeper
/// than it) are hidden.
List<int> visibleTraceRows(List<APITraceRow> rows, Set<int> collapsed) {
  final visible = <int>[];
  int? hideDeeperThan;
  for (var i = 0; i < rows.length; i++) {
    final depth = rows[i].depth;
    if (hideDeeperThan != null) {
      if (depth > hideDeeperThan) continue;
      hideDeeperThan = null;
    }
    visible.add(i);
    if (collapsed.contains(i) && traceRowHasChildren(rows, i)) {
      hideDeeperThan = depth;
    }
  }
  return visible;
}

/// `Passed` and `Unknown` rows are dimmed: the node only routed the thing, or
/// cannot say whether it touched it.
bool isDimmedContribution(APIContributionKind kind) =>
    kind == APIContributionKind.passed || kind == APIContributionKind.unknown;

/// A full-overlay popup: a dismiss barrier plus the menu, placed at
/// [position] (global coordinates) and kept inside the overlay.
class TraceMenu extends StatefulWidget {
  final Offset position;
  final List<APITraceRow> rows;

  /// The trace stopped at its row cap: the menu starts with a "… more" line,
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
  /// Instance rows folded by the user. Every instance starts expanded: trace
  /// lists are short.
  final Set<int> _collapsed = {};

  /// The row under the pointer, or null.
  int? _hovered;

  static const double _indentPerDepth = 14.0;

  void _toggle(int index) {
    setState(() {
      if (!_collapsed.remove(index)) _collapsed.add(index);
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
              if (widget.truncated)
                const Padding(
                  padding: EdgeInsets.fromLTRB(12, 0, 12, 4),
                  child: Text(
                    '… more',
                    style: TextStyle(
                      color: Colors.white38,
                      fontSize: 12,
                      fontStyle: FontStyle.italic,
                    ),
                  ),
                ),
              Flexible(
                child: SingleChildScrollView(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      for (final i in visibleTraceRows(rows, _collapsed))
                        _buildRow(i),
                    ],
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

  Widget _buildRow(int index) {
    final row = widget.rows[index];
    final dimmed = isDimmedContribution(row.contribution);
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
                  color: dimmed ? Colors.white54 : Colors.white,
                  fontSize: 13,
                ),
              ),
            ),
            const SizedBox(width: 24),
            Text(
              row.label,
              style: TextStyle(
                color: dimmed ? Colors.white38 : Colors.lightBlueAccent,
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
