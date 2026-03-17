import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../common/ui_common.dart';
import 'atomic_structure_visualization_widget.dart';
import 'structure_designer_model.dart';

/// Icon toggle button for mode switching (Direct Editing / Node Network).
class ModeToggleButton extends StatelessWidget {
  final IconData icon;
  final String tooltip;
  final bool isSelected;
  final bool enabled;
  final VoidCallback? onPressed;

  const ModeToggleButton({
    super.key,
    required this.icon,
    required this.tooltip,
    required this.isSelected,
    this.enabled = true,
    required this.onPressed,
  });

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: tooltip,
      child: Material(
        color: isSelected ? AppColors.primaryAccent : Colors.transparent,
        shape:
            RoundedRectangleBorder(borderRadius: BorderRadius.circular(4.0)),
        child: InkWell(
          borderRadius: BorderRadius.circular(4.0),
          onTap: enabled ? onPressed : null,
          child: Padding(
            padding: const EdgeInsets.all(2.0),
            child: Icon(
              icon,
              size: 20,
              color: isSelected
                  ? Colors.white
                  : (enabled ? Colors.grey[700] : Colors.grey[400]),
            ),
          ),
        ),
      ),
    );
  }
}

/// Mode switch buttons showing Direct Editing and Node Network toggle icons.
/// Used in both Direct Editing and Node Network display sections.
class ModeToggleButtons extends StatelessWidget {
  final StructureDesignerModel model;

  const ModeToggleButtons({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          final isDirectMode = model.directEditingMode;
          final canSwitchToDirect =
              isDirectMode || model.canSwitchToDirectEditingMode;
          return Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              ModeToggleButton(
                icon: Icons.edit,
                tooltip: canSwitchToDirect
                    ? 'Direct Editing Mode'
                    : 'Select a displayed atom_edit node to enter Direct Editing Mode',
                isSelected: isDirectMode,
                enabled: canSwitchToDirect,
                onPressed: isDirectMode
                    ? null
                    : () => model.switchToDirectEditingMode(),
              ),
              ModeToggleButton(
                icon: Icons.account_tree,
                tooltip: 'Node Network Mode',
                isSelected: !isDirectMode,
                onPressed: isDirectMode
                    ? () => model.switchToNodeNetworkMode()
                    : null,
              ),
            ],
          );
        },
      ),
    );
  }
}

/// Simplified Display widget for Direct Editing Mode.
/// Shows atomic visualization toggle and mode switch in a single row.
class DirectModeDisplayWidget extends StatelessWidget {
  final StructureDesignerModel model;

  const DirectModeDisplayWidget({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        AtomicStructureVisualizationWidget(model: model),
        const SizedBox(width: 8),
        Container(width: 1, height: 20, color: Colors.grey.shade400),
        const SizedBox(width: 8),
        ModeToggleButtons(model: model),
      ],
    );
  }
}
