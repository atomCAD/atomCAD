import 'package:flutter/material.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/common/passivant_dropdown.dart';
import 'package:flutter_cad/inputs/int_spin_field.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/proxy_api.dart'
    as proxy_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/tag_name_suggestions.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Input pin indices on `proxy` (`doc/design_proxy_node.md` §3.1). Every one of
/// pins 1–8 *replaces* the stored property of the same name at eval, so each
/// gets the standard "disable on wired input" treatment.
const int _FOCUS_PIN = 1;
const int _HOPS_PIN = 2;
const int _RIM_PIN = 3;
const int _RM_SINGLE_PIN = 4;
const int _PASSIVATE_PIN = 5;
const int _PASSIV_ELEM_PIN = 6;
const int _CORE_PIN = 7;
const int _FILL_PIN = 8;

/// The `core` value a freshly ticked "ONIOM core" checkbox takes. Zero is legal
/// but tags only the focus atoms, which makes every layer-boundary bond touch
/// one — a partition the future ONIOM exporter refuses (§4.7), so one is the
/// smallest useful starting point.
const int _DEFAULT_CORE = 1;

/// Editor widget for the `proxy` node — cuts a simulation proxy around the
/// atoms carrying the `focus` tag: bond-hop distance in, capped severed bonds
/// and a frozen rim out.
///
/// Stateful for one reason: the **report**. `ProxyStats` lives in the selected
/// node's eval cache, not on the node data, so it cannot ride in on
/// `APIProxyData`; it is re-read on every model notification, the way
/// `relax_editor.dart` re-reads its minimization message.
///
/// The eight properties themselves are written straight through — `data` is
/// re-fetched by the router on every rebuild, so there is no local copy to
/// drift.
class ProxyEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIProxyData? data;
  final StructureDesignerModel model;

  const ProxyEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ProxyEditor> createState() => _ProxyEditorState();
}

class _ProxyEditorState extends State<ProxyEditor> {
  APIProxyStats? _stats;

  @override
  void initState() {
    super.initState();
    _updateStats();
    widget.model.addListener(_updateStats);
  }

  @override
  void dispose() {
    widget.model.removeListener(_updateStats);
    super.dispose();
  }

  void _updateStats() {
    final stats = proxy_api.getProxyStats();
    if (mounted) {
      setState(() {
        _stats = stats;
      });
    }
  }

  /// True when the pin at [pinIndex] is wired — the wired value then replaces
  /// the stored property at eval, so the control renders disabled (but keeps
  /// its value, for re-activation on disconnect).
  bool _isPinConnected(int pinIndex) {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId &&
          wire.destParamIndex == BigInt.from(pinIndex)) {
        return true;
      }
    }
    return false;
  }

  /// Writes the eight persisted fields back with one of them replaced.
  /// `availableTags` is deliberately sent empty — it is an eval-time snapshot
  /// the node rewrites, and the Rust setter drops whatever arrives here.
  void _commit({
    String? focus,
    int? hops,
    int? rim,
    bool? rmSingle,
    bool? passivate,
    int? passivElem,
    int? core,
    bool? fill,
  }) {
    final data = widget.data;
    if (data == null) return;
    widget.model.setProxyData(
      widget.nodeId,
      APIProxyData(
        focus: focus ?? data.focus,
        hops: hops ?? data.hops,
        rim: rim ?? data.rim,
        rmSingle: rmSingle ?? data.rmSingle,
        passivate: passivate ?? data.passivate,
        passivElem: passivElem ?? data.passivElem,
        core: core ?? data.core,
        fill: fill ?? data.fill,
        availableTags: const [],
      ),
    );
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
            title: 'Simulation Proxy',
            nodeTypeName: 'proxy',
          ),
          const SizedBox(height: 16),

          // ---- focus ------------------------------------------------------
          _PinOverride(
            connected: _isPinConnected(_FOCUS_PIN),
            property: 'Focus tag',
            pinName: 'focus',
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                StringInput(
                  // Key on the stored value so the field rebuilds its
                  // controller when a suggestion chip changes it underneath.
                  key: ValueKey('proxy_focus_${data.focus}'),
                  label: 'Focus tag',
                  value: data.focus,
                  onChanged: (value) => _commit(focus: value.trim()),
                ),
                TagNameSuggestions(
                  available: data.availableTags,
                  currentName: data.focus,
                  onPick: (name) => _commit(focus: name),
                ),
              ],
            ),
          ),
          const SizedBox(height: 16),

          // ---- hops / rim -------------------------------------------------
          _PinOverride(
            connected: _isPinConnected(_HOPS_PIN),
            property: 'Hops',
            pinName: 'hops',
            child: _LabelledSpin(
              label: 'Hops',
              value: data.hops,
              minimumValue: 0,
              onChanged: (value) => _commit(hops: value),
            ),
          ),
          const SizedBox(height: 12),
          _PinOverride(
            connected: _isPinConnected(_RIM_PIN),
            property: 'Rim',
            pinName: 'rim',
            child: _LabelledSpin(
              label: 'Rim (frozen shells)',
              value: data.rim,
              minimumValue: 0,
              onChanged: (value) => _commit(rim: value),
            ),
          ),
          const SizedBox(height: 16),

          // ---- core -------------------------------------------------------
          _PinOverride(
            connected: _isPinConnected(_CORE_PIN),
            property: 'Core',
            pinName: 'core',
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                CheckboxListTile(
                  title: const Text('Tag an ONIOM core (`high`)'),
                  value: data.core >= 0,
                  onChanged: (value) =>
                      _commit(core: (value ?? false) ? _DEFAULT_CORE : -1),
                  controlAffinity: ListTileControlAffinity.leading,
                  contentPadding: EdgeInsets.zero,
                ),
                if (data.core >= 0)
                  Padding(
                    padding: const EdgeInsets.only(left: 16.0, top: 4.0),
                    child: _LabelledSpin(
                      label: 'Core (hops)',
                      value: data.core,
                      minimumValue: 0,
                      onChanged: (value) => _commit(core: value),
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(height: 12),

          // ---- the three flags --------------------------------------------
          _PinOverride(
            connected: _isPinConnected(_FILL_PIN),
            property: 'Fill',
            pinName: 'fill',
            child: CheckboxListTile(
              title: const Text('Fill bridging atoms'),
              value: data.fill,
              onChanged: (value) => _commit(fill: value ?? true),
              controlAffinity: ListTileControlAffinity.leading,
              contentPadding: EdgeInsets.zero,
            ),
          ),
          _PinOverride(
            connected: _isPinConnected(_RM_SINGLE_PIN),
            property: 'Remove single-neighbour atoms',
            pinName: 'rm_single',
            child: CheckboxListTile(
              title: const Text('Remove single-neighbour atoms'),
              value: data.rmSingle,
              onChanged: (value) => _commit(rmSingle: value ?? false),
              controlAffinity: ListTileControlAffinity.leading,
              contentPadding: EdgeInsets.zero,
            ),
          ),
          _PinOverride(
            connected: _isPinConnected(_PASSIVATE_PIN),
            property: 'Passivate',
            pinName: 'passivate',
            child: CheckboxListTile(
              title: const Text('Cap severed bonds'),
              value: data.passivate,
              onChanged: (value) => _commit(passivate: value ?? true),
              controlAffinity: ListTileControlAffinity.leading,
              contentPadding: EdgeInsets.zero,
            ),
          ),
          const SizedBox(height: 12),

          // ---- passivant element ------------------------------------------
          _PinOverride(
            connected: _isPinConnected(_PASSIV_ELEM_PIN),
            property: 'Passivant element',
            pinName: 'passiv_elem',
            child: Opacity(
              opacity: data.passivate ? 1.0 : 0.5,
              child: IgnorePointer(
                ignoring: !data.passivate,
                child: PassivantDropdown(
                  value: data.passivElem,
                  onChanged: (value) => _commit(passivElem: value),
                ),
              ),
            ),
          ),
          const SizedBox(height: 16),

          // ---- the report, last: settings first, results after -------------
          _ProxyReport(stats: _stats),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}

/// A caption over an [IntSpinField] with its `−` / `+` buttons. `IntInput`
/// would do, but it owns its own label styling and this panel stacks many
/// short rows; keeping one shape here keeps them aligned.
class _LabelledSpin extends StatelessWidget {
  final String label;
  final int value;
  final int? minimumValue;
  final ValueChanged<int> onChanged;

  const _LabelledSpin({
    required this.label,
    required this.value,
    required this.onChanged,
    this.minimumValue,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: Theme.of(context).textTheme.bodySmall),
        const SizedBox(height: 4),
        IntSpinField(
          value: value,
          onChanged: onChanged,
          minimumValue: minimumValue,
        ),
      ],
    );
  }
}

/// The "disable on wired input" wrapper (node_data/AGENTS.md): an italic line
/// naming the pin, then the control greyed and inert. The stored value is left
/// alone so it re-activates on disconnect.
class _PinOverride extends StatelessWidget {
  final bool connected;
  final String property;
  final String pinName;
  final Widget child;

  const _PinOverride({
    required this.connected,
    required this.property,
    required this.pinName,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (connected)
          Padding(
            padding: const EdgeInsets.only(bottom: 4.0),
            child: Text(
              '$property supplied by `$pinName` input. Disconnect to edit inline.',
              style: TextStyle(
                fontStyle: FontStyle.italic,
                fontSize: 12,
                color: Theme.of(context).colorScheme.primary,
              ),
            ),
          ),
        Opacity(
          opacity: connected ? 0.5 : 1.0,
          child: IgnorePointer(ignoring: connected, child: child),
        ),
      ],
    );
  }
}

/// The cut's report — `ProxyStats`, read from the selected node's eval cache.
///
/// Null until the node has been evaluated as a root node with this panel open
/// (a node that is not displayed never becomes one), which is also what a
/// broken upstream leaves behind.
class _ProxyReport extends StatelessWidget {
  final APIProxyStats? stats;

  const _ProxyReport({required this.stats});

  /// An absent distance means "no such pair within the module's search
  /// radius" — a fact, not a missing measurement, so it prints as an em dash
  /// rather than as a zero.
  static String _distance(double? value) =>
      value == null ? '—' : '${formatNatural(value, 3)} Å';

  @override
  Widget build(BuildContext context) {
    final stats = this.stats;
    return Card(
      elevation: 1,
      child: Padding(
        padding: const EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Cut report', style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            if (stats == null)
              Text(
                'No cut evaluated yet — display the node to see its report.',
                style: Theme.of(context).textTheme.bodySmall,
              )
            else ...[
              Text(
                stats.formula,
                style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                      fontFamily: 'monospace',
                      fontWeight: FontWeight.bold,
                    ),
              ),
              const SizedBox(height: 8),
              _ReportRow(
                  label: 'Heavy / riders / caps',
                  value: '${stats.heavy} / ${stats.riders} / ${stats.caps}'),
              _ReportRow(
                  label: 'Free / frozen',
                  value: '${stats.free} / ${stats.frozen}'),
              _ReportRow(
                  label: 'Filled',
                  value: '${stats.filled} (${stats.fillRounds} rounds)'),
              _ReportRow(
                  label: 'Farthest hop (size)', value: '${stats.farthestHop}'),
              _ReportRow(
                  label: 'Free depth (hops)', value: '${stats.freeHops}'),
              _ReportRow(
                  label: 'Open valences', value: '${stats.openValences}'),
              _ReportRow(
                  label: 'Closest cap pair',
                  value: _distance(stats.minCapPair)),
              _ReportRow(
                  label: 'Nearest dropped',
                  value: _distance(stats.nearestDropped)),
              if (stats.nearestDropped != null && stats.nearestDropped! < 4.0)
                Padding(
                  padding: const EdgeInsets.only(top: 4.0),
                  child: Text(
                    'An unbonded neighbour this close was cut away — tag one of '
                    'its atoms as focus too.',
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: Theme.of(context).colorScheme.error,
                        ),
                  ),
                ),
              if (stats.minCapPair != null && stats.minCapPair! < 2.0)
                Padding(
                  padding: const EdgeInsets.only(top: 4.0),
                  child: Text(
                    'Caps this close are unphysical — turn `fill` on.',
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: Theme.of(context).colorScheme.error,
                        ),
                  ),
                ),
            ],
          ],
        ),
      ),
    );
  }
}

/// One `label … value` line of the report.
class _ReportRow extends StatelessWidget {
  final String label;
  final String value;

  const _ReportRow({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2.0),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Text(label, style: Theme.of(context).textTheme.bodySmall),
          ),
          const SizedBox(width: 8),
          Text(
            value,
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  fontFamily: 'monospace',
                ),
          ),
        ],
      ),
    );
  }
}
