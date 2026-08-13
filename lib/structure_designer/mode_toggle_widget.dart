import 'package:flutter/material.dart';
import 'display_button_group.dart';
import 'structure_designer_model.dart';

/// The editing-mode cluster of the DISPLAY panel: a radio group switching
/// between Direct Editing and Node Network mode. Present in both modes'
/// sidebars, since it is the way back out of each.
DisplayGroupCluster modeToggleCluster(StructureDesignerModel model) {
  final isDirectMode = model.directEditingMode;
  final canSwitchToDirect = isDirectMode || model.canSwitchToDirectEditingMode;

  return DisplayGroupCluster([
    DisplayButtonGroup([
      DisplayIconButton(
        key: const Key('mode_direct_editing'),
        icon: Icons.edit,
        tooltip: canSwitchToDirect
            ? 'Direct Editing Mode'
            : 'Select a displayed atom_edit node to enter Direct Editing Mode',
        isSelected: isDirectMode,
        enabled: canSwitchToDirect,
        onPressed:
            isDirectMode ? null : () => model.switchToDirectEditingMode(),
      ),
      DisplayIconButton(
        key: const Key('mode_node_network'),
        icon: Icons.account_tree,
        tooltip: 'Node Network Mode',
        isSelected: !isDirectMode,
        onPressed: isDirectMode ? () => model.switchToNodeNetworkMode() : null,
      ),
    ]),
  ]);
}
