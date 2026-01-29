import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:provider/provider.dart';
import '../common/ui_common.dart';
import 'structure_designer_model.dart';

/// Widget that allows selecting between different atomic structure visualization modes
class AtomicStructureVisualizationWidget extends StatelessWidget {
  final StructureDesignerModel model;

  const AtomicStructureVisualizationWidget({
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
              // Ball and Stick Button
              _buildIconButton(
                context,
                Icons.hub, // Using hub icon to represent atoms (circles) connected by bonds (lines)
                'Atomic visualization: Ball and Stick',
                key: const Key('atomic_vis_ball_and_stick'),
                isSelected: model.preferences?.atomicStructureVisualizationPreferences
                        .visualization ==
                    AtomicStructureVisualization.ballAndStick,
                onPressed: () {
                  model.preferences?.atomicStructureVisualizationPreferences
                          .visualization =
                      AtomicStructureVisualization.ballAndStick;
                  model.setPreferences(model.preferences!);
                },
              ),

              // Space Filling Button
              _buildIconButton(
                context,
                Icons.circle, // Using circle to represent space filling spheres
                'Atomic visualization: Space Filling',
                key: const Key('atomic_vis_space_filling'),
                isSelected: model.preferences?.atomicStructureVisualizationPreferences
                        .visualization ==
                    AtomicStructureVisualization.spaceFilling,
                onPressed: () {
                  model.preferences?.atomicStructureVisualizationPreferences
                          .visualization =
                      AtomicStructureVisualization.spaceFilling;
                  model.setPreferences(model.preferences!);
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
