import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../src/rust/api/structure_designer/structure_designer_api_types.dart';
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
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              // Surface Splatting Button (point cloud visualization)
              _buildIconButton(
                context,
                Icons.blur_on,  // Using blur_on to represent point cloud
                'Surface Splatting',
                isSelected: model.geometryVisualization3D == 
                    APIGeometryVisualization3D.surfaceSplatting,
                onPressed: () {
                  model.setGeometryVisualization3D(
                      APIGeometryVisualization3D.surfaceSplatting);
                  model.setWireframeGeometry(false);
                },
              ),
              
              const SizedBox(width: 8),
              
              // Dual Contouring with Wireframe Button
              _buildIconButton(
                context,
                Icons.grid_3x3,  // Using grid to represent wireframe
                'Dual Contouring Wireframe',
                isSelected: model.geometryVisualization3D ==
                        APIGeometryVisualization3D.dualContouring &&
                    model.wireframeGeometry == true,
                onPressed: () {
                  model.setGeometryVisualization3D(
                      APIGeometryVisualization3D.dualContouring);
                  model.setWireframeGeometry(true);
                },
              ),
              
              const SizedBox(width: 8),
              
              // Dual Contouring without Wireframe Button (solid)
              _buildIconButton(
                context,
                Icons.view_in_ar,  // Using 3D object icon for solid
                'Dual Contouring Solid',
                isSelected: model.geometryVisualization3D ==
                        APIGeometryVisualization3D.dualContouring &&
                    model.wireframeGeometry == false,
                onPressed: () {
                  model.setGeometryVisualization3D(
                      APIGeometryVisualization3D.dualContouring);
                  model.setWireframeGeometry(false);
                },
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildIconButton(
      BuildContext context,
      IconData icon,
      String tooltip,
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
            padding: const EdgeInsets.all(8.0),
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
