/// The derived readouts the two mechanosynthesis panels share: the method
/// badge, the *Tools* block and the *Feedstocks* line.
///
/// All three are **derived**, never editable. A step does not state its method
/// — the operation does (`doc/design_mechanosynth_tools.md` §Operations declare
/// their method) — and a tool is not chosen either: the operation names its
/// type and the atom tags on the wired molecule name the instance. So there is
/// nothing here for a user to set, and everything here is read off the last
/// evaluation.
///
/// **The two panels want different shapes of the same facts.** The replayer has
/// room for a block — one row per tool with its fit residual — because its
/// panel is a scrubber and a phase list. The editor's panel is a step list that
/// wants every pixel for steps, so its readout is one line
/// (*habst_tool · spent*) whose whole job is to say a recharge is due before
/// the offer popup does. Hence [MechanosynthToolsBlock] and
/// [MechanosynthToolsReadout] rather than one widget with a `dense` flag.
library;

import 'package:flutter/material.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Colours a method kind. The method is the **operation's** kind, not something
/// a step types — see `doc/design_mechanosynth_tools.md`.
///
/// The vocabulary is closed and three words long, so the colours are literal
/// rather than hashed: the same kind is the same colour in every project, and a
/// library that says something else gets no colour rather than a colour that
/// implies it is one of the three.
Color? methodColor(String method) {
  switch (method) {
    case 'tip':
      return const Color(0xFF7E9CD8);
    case 'bulk':
      return const Color(0xFF98BB6C);
    case 'spontaneous':
      return const Color(0xFFE6C384);
    default:
      return null;
  }
}

/// The read-only kind badge, with the instrument or the agent beside it.
///
/// [detail] is the operation's tool type (a `tip` step) or its agent (a `bulk`
/// one); a `spontaneous` step has neither and shows the badge alone. An empty
/// [method] renders nothing at all — a step whose operation the wired library
/// does not have has no kind to state, and a placeholder would be a lie.
class MechanosynthMethodBadge extends StatelessWidget {
  final String method;
  final String detail;

  const MechanosynthMethodBadge({
    super.key,
    required this.method,
    this.detail = '',
  });

  @override
  Widget build(BuildContext context) {
    if (method.isEmpty) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;
    final color = methodColor(method) ?? scheme.onSurfaceVariant;

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 6.0, vertical: 2.0),
          decoration: BoxDecoration(
            color: color.withValues(alpha: 0.18),
            border: Border.all(color: color.withValues(alpha: 0.55)),
            borderRadius: BorderRadius.circular(4.0),
          ),
          child: Text(
            method,
            style: TextStyle(
                fontSize: 11.0, color: color, fontWeight: FontWeight.w600),
          ),
        ),
        if (detail.isNotEmpty) ...[
          const SizedBox(width: 5.0),
          Flexible(
            child: Text(
              detail,
              softWrap: false,
              overflow: TextOverflow.fade,
              style: TextStyle(fontSize: 11.0, color: scheme.onSurfaceVariant),
            ),
          ),
        ],
      ],
    );
  }
}

/// What a step's badge says beside its kind: the instrument for a `tip` step,
/// the agent for a `bulk` one, nothing for a `spontaneous` one.
///
/// Shared so the replayer's readout and the editor's row cannot disagree about
/// which of the two fields belongs to which kind.
String methodDetail({required String toolType, required String agent}) =>
    toolType.isNotEmpty ? toolType : agent;

/// The **instrument** an operation names — what the editor panel's palette
/// groups by.
///
/// [methodDetail] for a `tip` or `bulk` operation, and the method itself
/// otherwise, so a `spontaneous` group still has a label to wear. The schema's
/// own rule is *one operation, one instrument*, so this is a fact the library
/// already states rather than a grouping invented in the panel — there is
/// deliberately no `family` key (`doc/design_mechanosynth_editor.md`
/// §Considered and rejected), and this stands in for one.
///
/// An operation the wired library does not define has no method and therefore
/// no instrument; it returns the empty string and belongs to no group.
String instrumentOf(APIMechanosynthOp op) {
  final detail = methodDetail(toolType: op.toolType, agent: op.agent);
  return detail.isNotEmpty ? detail : op.method;
}

/// The instrument groups of `ops`, in the order the library first mentions
/// each one.
///
/// Insertion order rather than alphabetical: a library lists its operations in
/// the order its author thought about them, and a chip row that follows it
/// reads like the library's own table of contents.
///
/// Operations with no method — muted names the wired library does not define —
/// join no group, because they have nothing to say about what performs them.
List<MapEntry<String, List<APIMechanosynthOp>>> groupOperationsByInstrument(
    List<APIMechanosynthOp> ops) {
  final groups = <String, List<APIMechanosynthOp>>{};
  for (final op in ops) {
    if (op.method.isEmpty) continue;
    groups.putIfAbsent(instrumentOf(op), () => []).add(op);
  }
  return groups.entries.toList();
}

/// The replayer's *Tools* block and *Feedstocks* line.
///
/// Both lists are read off the node's **last evaluation** — the kernel never
/// forces one to answer a panel rebuild — so an empty block means either that
/// nothing is wired to the pins or that the node has not been evaluated yet,
/// and in both cases saying nothing is right.
class MechanosynthToolsBlock extends StatelessWidget {
  final List<APIMechanosynthToolRow> tools;
  final List<APIMechanosynthFeedstockRow> feedstocks;

  const MechanosynthToolsBlock({
    super.key,
    required this.tools,
    required this.feedstocks,
  });

  @override
  Widget build(BuildContext context) {
    if (tools.isEmpty && feedstocks.isEmpty) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;
    final captionStyle = Theme.of(context).textTheme.bodySmall;

    return Padding(
      padding: const EdgeInsets.only(top: 10.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (tools.isNotEmpty) ...[
            Text('Tools',
                key: const Key('mechanosynth_tools_heading'),
                style: captionStyle?.copyWith(color: scheme.onSurfaceVariant)),
            const SizedBox(height: 3.0),
            for (final tool in tools) _buildToolRow(context, scheme, tool),
          ],
          if (feedstocks.isNotEmpty)
            Padding(
              padding: EdgeInsets.only(top: tools.isEmpty ? 0.0 : 6.0),
              child: Text(
                'Feedstocks: ${_feedstockEntries(feedstocks)}',
                key: const Key('mechanosynth_feedstocks_line'),
                style: captionStyle?.copyWith(color: scheme.onSurfaceVariant),
              ),
            ),
        ],
      ),
    );
  }

  /// Index, the type its tag names, the pose residual and the state at the
  /// step. The residual is the evidence that the four tagged atoms really are
  /// the frame the library describes: a tool whose legs were tagged on the
  /// wrong atoms binds with a residual nobody would call a fit.
  Widget _buildToolRow(
      BuildContext context, ColorScheme scheme, APIMechanosynthToolRow tool) {
    return Padding(
      key: Key('mechanosynth_tool_row_${tool.instance}'),
      padding: const EdgeInsets.only(bottom: 2.0),
      child: Row(
        children: [
          SizedBox(
            width: 20.0,
            child: Text('${tool.instance}',
                style:
                    TextStyle(fontSize: 11.0, color: scheme.onSurfaceVariant)),
          ),
          Expanded(
            child: Text(
              tool.toolType,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 12.0, color: scheme.onSurface),
            ),
          ),
          if (tool.state.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(left: 6.0),
              child: Text(
                tool.state,
                style: TextStyle(
                  fontSize: 11.0,
                  color: scheme.onSurface,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          Padding(
            padding: const EdgeInsets.only(left: 8.0),
            child: Text(
              '${formatNatural(tool.residual, 2)} Å',
              style: TextStyle(fontSize: 11.0, color: scheme.onSurfaceVariant),
            ),
          ),
        ],
      ),
    );
  }
}

/// The editor's one-line *Tools* readout, above the steps list.
///
/// One line per panel rather than one per tool: the thing it exists to say is
/// *habst_tool is spent*, and a build has one or two instruments, so the
/// states fit on a line and a block would push the steps down for no gain.
class MechanosynthToolsReadout extends StatelessWidget {
  final List<APIMechanosynthToolRow> tools;
  final List<APIMechanosynthFeedstockRow> feedstocks;

  const MechanosynthToolsReadout({
    super.key,
    required this.tools,
    required this.feedstocks,
  });

  @override
  Widget build(BuildContext context) {
    if (tools.isEmpty && feedstocks.isEmpty) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;
    final parts = <String>[
      for (final tool in tools)
        tool.state.isEmpty ? tool.toolType : '${tool.toolType} · ${tool.state}',
    ];
    if (feedstocks.isNotEmpty) {
      parts.add(_feedstockSummary(feedstocks));
    }

    return Padding(
      padding: const EdgeInsets.only(top: 6.0),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.build_outlined, size: 14, color: scheme.onSurfaceVariant),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              parts.join('   '),
              key: const Key('mechanosynth_edit_tools_readout'),
              style: TextStyle(fontSize: 11.5, color: scheme.onSurfaceVariant),
            ),
          ),
        ],
      ),
    );
  }
}

/// `0: 12 atoms · 1: 30 atoms` — **one entry per wired reservoir**, in pin
/// order, because a build that draws on two of them is exactly the case where
/// the number matters and a total would hide which one moved. The count is what
/// a scrub across a dump step visibly changes, which is why it is the number on
/// the line; the index is the pin position, so it matches the `feedstocks`
/// wires in the network.
String _feedstockEntries(List<APIMechanosynthFeedstockRow> feedstocks) =>
    feedstocks
        .map((row) => '${row.instance}: ${row.atomCount} atoms')
        .join(' · ');

/// `2 reservoirs · 48 atoms` — the **aggregate**, for the editor's one-liner,
/// which is already carrying the tool states and has no room for a per-pin
/// list. The editor's question is "is a recharge due", not "which reservoir";
/// the replayer's block answers the second.
String _feedstockSummary(List<APIMechanosynthFeedstockRow> feedstocks) {
  final atoms = feedstocks.fold<int>(0, (total, row) => total + row.atomCount);
  final noun = feedstocks.length == 1 ? 'reservoir' : 'reservoirs';
  return '${feedstocks.length} $noun · $atoms atoms';
}
