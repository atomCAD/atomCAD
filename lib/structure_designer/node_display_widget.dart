import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:provider/provider.dart';
import '../common/ui_common.dart';
import 'structure_designer_model.dart';

/// Widget that allows selecting between different node display policies
class NodeDisplayWidget extends StatelessWidget {
  final StructureDesignerModel model;

  const NodeDisplayWidget({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          return Row(
            mainAxisAlignment: MainAxisAlignment.start,
            children: [
              // Manual display policy
              _buildIconButton(
                context,
                Icons.tune, // Using tune icon to represent manual control
                'Node display policy: Manual (User Selection)',
                key: const Key('node_display_manual'),
                isSelected:
                    model.preferences?.nodeDisplayPreferences.displayPolicy ==
                        NodeDisplayPolicy.manual,
                onPressed: () {
                  // Create a copy of the preferences to modify
                  final prefs = model.preferences!;
                  prefs.nodeDisplayPreferences.displayPolicy =
                      NodeDisplayPolicy.manual;
                  model.setPreferences(prefs);
                },
              ),

              // Prefer Selected display policy
              _buildIconButton(
                context,
                Icons.star, // Using star icon to represent selected items
                'Node display policy: Prefer Selected Nodes',
                key: const Key('node_display_prefer_selected'),
                isSelected:
                    model.preferences?.nodeDisplayPreferences.displayPolicy ==
                        NodeDisplayPolicy.preferSelected,
                onPressed: () {
                  // Create a copy of the preferences to modify
                  final prefs = model.preferences!;
                  prefs.nodeDisplayPreferences.displayPolicy =
                      NodeDisplayPolicy.preferSelected;
                  model.setPreferences(prefs);
                },
              ),

              // Prefer Frontier display policy
              _buildIconButton(
                context,
                Icons.explore, // Using explore icon for frontier/boundary
                'Node display policy: Prefer Frontier Nodes',
                key: const Key('node_display_prefer_frontier'),
                isSelected:
                    model.preferences?.nodeDisplayPreferences.displayPolicy ==
                        NodeDisplayPolicy.preferFrontier,
                onPressed: () {
                  // Create a copy of the preferences to modify
                  final prefs = model.preferences!;
                  prefs.nodeDisplayPreferences.displayPolicy =
                      NodeDisplayPolicy.preferFrontier;
                  model.setPreferences(prefs);
                },
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildIconButton(BuildContext context, IconData icon, String tooltip,
      {required bool isSelected, required VoidCallback onPressed, Key? key}) {
    return Tooltip(
      message: tooltip,
      child: Material(
        key: key,
        color: isSelected ? AppColors.primaryAccent : Colors.transparent,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(4.0)),
        child: InkWell(
          borderRadius: BorderRadius.circular(4.0),
          onTap: onPressed,
          child: Padding(
            padding: const EdgeInsets.all(2.0),
            child: Icon(
              icon,
              size: 20,
              color: isSelected ? Colors.white : Colors.grey[700],
            ),
          ),
        ),
      ),
    );
  }
}
