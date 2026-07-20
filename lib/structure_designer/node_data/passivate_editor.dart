import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/common/passivant_dropdown.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Index of the `element` input pin on `passivate`.
/// 0 = molecule, 1 = region, 2 = element.
const int _ELEMENT_PIN_INDEX = 2;

/// Editor widget for the `passivate` node (né `add_hydrogen`, issue #405).
/// Exposes the terminator element as a restricted dropdown (H/F/Cl/Br/I). When
/// the optional `element` input pin is wired, the wired value wins at eval, so
/// the dropdown renders disabled (but keeps its stored value for re-activation
/// on disconnect) — the standard "disable on wired input" pattern.
class PassivateEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIPassivateData? data;
  final StructureDesignerModel model;

  const PassivateEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  /// True when the optional `element` input pin is wired. Detected by walking
  /// the current network view's wires (see node_data/AGENTS.md "Disable on
  /// wired input" pattern).
  bool _isElementPinConnected() {
    final view = model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == nodeId &&
          wire.destParamIndex == BigInt.from(_ELEMENT_PIN_INDEX)) {
        return true;
      }
    }
    return false;
  }

  @override
  Widget build(BuildContext context) {
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final connected = _isElementPinConnected();

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Passivate',
            nodeTypeName: 'passivate',
          ),
          const SizedBox(height: 16),
          if (connected)
            Padding(
              padding: const EdgeInsets.only(bottom: 8.0),
              child: Text(
                'Element supplied by `element` input. Disconnect to edit inline.',
                style: TextStyle(
                  fontStyle: FontStyle.italic,
                  fontSize: 12,
                  color: Theme.of(context).colorScheme.primary,
                ),
              ),
            ),
          Opacity(
            opacity: connected ? 0.5 : 1.0,
            child: IgnorePointer(
              ignoring: connected,
              child: PassivantDropdown(
                value: data!.element,
                onChanged: (newValue) {
                  model.setPassivateData(
                    nodeId,
                    APIPassivateData(element: newValue),
                  );
                },
              ),
            ),
          ),
          const SizedBox(height: 8),
          Text(
            'Caps undersaturated atoms with this terminator at the correct '
            'host–terminator bond length. Halogens (F/Cl/Br/I) are placed at '
            'their equilibrium bond length, so no relax pass is needed.',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}
