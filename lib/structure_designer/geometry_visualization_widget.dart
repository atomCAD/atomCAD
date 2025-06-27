import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:provider/provider.dart';
import '../common/ui_common.dart';
import 'structure_designer_model.dart';

/// Widget that allows selecting between different geometry visualization modes
class GeometryVisualizationWidget extends StatelessWidget {
  final StructureDesignerModel model;

  const GeometryVisualizationWidget({
    Key? key,
    required this.model,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          return Row(
            mainAxisAlignment: MainAxisAlignment.start,
            children: [
              // Surface Splatting Button (point cloud visualization)
              _buildIconButton(
                context,
                Icons.blur_on, // Using blur_on to represent point cloud
                'Geometry visualization: Surface Splatting',
                isSelected: model.preferences?.geometryVisualizationPreferences
                        .geometryVisualization ==
                    GeometryVisualization.surfaceSplatting,
                onPressed: () {
                  model.preferences?.geometryVisualizationPreferences
                          .geometryVisualization =
                      GeometryVisualization.surfaceSplatting;
                  model.preferences?.geometryVisualizationPreferences
                      .wireframeGeometry = false;
                  model.setPreferences(model.preferences!);
                },
              ),

              // Dual Contouring with Wireframe Button
              _buildIconButton(
                context,
                Icons.grid_3x3, // Using grid to represent wireframe
                'Geometry visualization: Wireframe (explicit mesh)',
                isSelected: model.preferences?.geometryVisualizationPreferences
                            .geometryVisualization ==
                        GeometryVisualization.explicitMesh &&
                    model.preferences?.geometryVisualizationPreferences
                            .wireframeGeometry ==
                        true,
                onPressed: () {
                  model.preferences?.geometryVisualizationPreferences
                          .geometryVisualization =
                      GeometryVisualization.explicitMesh;
                  model.preferences?.geometryVisualizationPreferences
                      .wireframeGeometry = true;
                  model.setPreferences(model.preferences!);
                },
              ),

              // Dual Contouring without Wireframe Button (solid)
              _buildIconButton(
                context,
                Icons.view_in_ar, // Using 3D object icon for solid
                'Geometry visualization: Solid (explicit mesh)',
                isSelected: model.preferences?.geometryVisualizationPreferences
                            .geometryVisualization ==
                        GeometryVisualization.explicitMesh &&
                    model.preferences?.geometryVisualizationPreferences
                            .wireframeGeometry ==
                        false,
                onPressed: () {
                  model.preferences?.geometryVisualizationPreferences
                          .geometryVisualization =
                      GeometryVisualization.explicitMesh;
                  model.preferences?.geometryVisualizationPreferences
                      .wireframeGeometry = false;
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
      {required bool isSelected, required VoidCallback onPressed}) {
    return Tooltip(
      message: tooltip,
      child: Material(
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
