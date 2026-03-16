import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'atomic_structure_visualization_widget.dart';
import 'structure_designer_model.dart';

/// Simplified Display widget for Direct Editing Mode.
/// Shows atomic visualization toggle and mode switch radio buttons.
class DirectModeDisplayWidget extends StatelessWidget {
  final StructureDesignerModel model;

  const DirectModeDisplayWidget({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Atomic visualization toggle (Ball and Stick / Space Filling)
              Row(
                mainAxisAlignment: MainAxisAlignment.start,
                children: [
                  AtomicStructureVisualizationWidget(model: model),
                ],
              ),
              const SizedBox(height: 8),
              // Mode switch radio buttons
              _buildModeRadioButtons(model),
            ],
          );
        },
      ),
    );
  }

  Widget _buildModeRadioButtons(StructureDesignerModel model) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _ModeRadioTile(
          key: const Key('mode_radio_direct_editing'),
          label: 'Direct Editing',
          selected: true,
          onTap: null,
        ),
        _ModeRadioTile(
          key: const Key('mode_radio_node_network'),
          label: 'Node Network',
          selected: false,
          onTap: () => model.switchToNodeNetworkMode(),
        ),
      ],
    );
  }
}

/// Mode radio button used in both Direct and Node Network display sections.
class _ModeRadioTile extends StatelessWidget {
  final String label;
  final bool selected;
  final VoidCallback? onTap;

  const _ModeRadioTile({
    super.key,
    required this.label,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final enabled = onTap != null || selected;
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 1.0),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            SizedBox(
              width: 24,
              height: 24,
              child: Radio<bool>(
                value: true,
                groupValue: selected ? true : false,
                onChanged: enabled ? (_) => onTap?.call() : null,
                materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                visualDensity: VisualDensity.compact,
              ),
            ),
            const SizedBox(width: 4),
            Text(
              label,
              style: TextStyle(
                fontSize: 12,
                color: enabled ? null : Colors.grey,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Mode switch radio buttons for the Node Network Mode Display section.
/// Shows "Direct Editing" (conditionally enabled) and "Node Network" (selected).
class NodeNetworkModeRadioButtons extends StatelessWidget {
  final StructureDesignerModel model;

  const NodeNetworkModeRadioButtons({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          final canSwitch = model.canSwitchToDirectEditingMode;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Tooltip(
                message: canSwitch
                    ? ''
                    : 'Select a displayed atom_edit node to enter Direct Editing Mode',
                child: _ModeRadioTile(
                  key: const Key('mode_radio_direct_editing'),
                  label: 'Direct Editing',
                  selected: false,
                  onTap: canSwitch
                      ? () => model.switchToDirectEditingMode()
                      : null,
                ),
              ),
              _ModeRadioTile(
                key: const Key('mode_radio_node_network'),
                label: 'Node Network',
                selected: true,
                onTap: null,
              ),
            ],
          );
        },
      ),
    );
  }
}
