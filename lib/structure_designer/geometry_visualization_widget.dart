import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../src/rust/api/structure_designer/structure_designer_api_types.dart';
import '../styles/app_styles.dart';
import '../widgets/card_with_title.dart';
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
          return CardWithTitle(
            title: 'Geometry Visualization',
            child: Padding(
              padding: const EdgeInsets.all(8.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  // Surface Splatting Button
                  _buildVisualizationButton(
                    context,
                    'Surface Splatting',
                    isSelected: model.geometryVisualization3D ==
                            APIGeometryVisualization3D.surfaceSplatting,
                    onPressed: () {
                      model.setGeometryVisualization3D(
                          APIGeometryVisualization3D.surfaceSplatting);
                      model.setWireframeGeometry(false);
                    },
                  ),
                  const SizedBox(height: 8),
                  
                  // Dual Contouring with Wireframe Button
                  _buildVisualizationButton(
                    context,
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
                  const SizedBox(height: 8),
                  
                  // Dual Contouring without Wireframe Button
                  _buildVisualizationButton(
                    context,
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
              ),
            ),
          );
        },
      ),
    );
  }

  Widget _buildVisualizationButton(
    BuildContext context,
    String label,
    {required bool isSelected, required VoidCallback onPressed}
  ) {
    return ElevatedButton(
      style: isSelected 
          ? AppButtonStyles.primary 
          : AppButtonStyles.secondary,
      onPressed: onPressed,
      child: Text(label),
    );
  }
}
