import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'display_button_group.dart';
import 'structure_designer_model.dart';

/// The geometry cluster of the DISPLAY panel: the rendering-method radio group,
/// then the "show geometry shell on Crystal and Molecule" toggle.
///
/// Two groups rather than one because the shell toggle is a different axis of
/// choice from the rendering method — see `display_button_group.dart`.
DisplayGroupCluster geometryVisualizationCluster(StructureDesignerModel model) {
  final prefs = model.preferences?.geometryVisualizationPreferences;

  void setMethod(GeometryVisualization visualization, bool wireframe) {
    if (prefs == null) return;
    prefs.geometryVisualization = visualization;
    prefs.wireframeGeometry = wireframe;
    model.setPreferences(model.preferences!);
  }

  return DisplayGroupCluster([
    // Radio group: how geometry node outputs are rendered.
    DisplayButtonGroup([
      DisplayIconButton(
        key: const Key('geometry_vis_surface_splatting'),
        icon: Icons.blur_on, // Using blur_on to represent point cloud
        tooltip: 'Geometry visualization: Surface Splatting',
        isSelected: prefs?.geometryVisualization ==
            GeometryVisualization.surfaceSplatting,
        onPressed: () =>
            setMethod(GeometryVisualization.surfaceSplatting, false),
      ),
      DisplayIconButton(
        key: const Key('geometry_vis_wireframe'),
        icon: Icons.grid_3x3, // Using grid to represent wireframe
        tooltip: 'Geometry visualization: Wireframe',
        isSelected: prefs?.geometryVisualization ==
                GeometryVisualization.explicitMesh &&
            prefs?.wireframeGeometry == true,
        onPressed: () => setMethod(GeometryVisualization.explicitMesh, true),
      ),
      DisplayIconButton(
        key: const Key('geometry_vis_solid'),
        icon: Icons.view_in_ar, // Using 3D object icon for solid
        tooltip: 'Geometry visualization: Solid',
        isSelected: prefs?.geometryVisualization ==
                GeometryVisualization.explicitMesh &&
            prefs?.wireframeGeometry == false,
        onPressed: () => setMethod(GeometryVisualization.explicitMesh, false),
      ),
    ]),
    // Toggle: draw the geometry shell alongside the atoms of Crystal/Molecule.
    DisplayButtonGroup([
      DisplayIconButton(
        key: const Key('geometry_vis_shell_on_atomic'),
        icon: Icons.layers,
        tooltip: 'Show geometry shell on Crystal and Molecule',
        isSelected: prefs?.showGeometryShellForAtomic == true,
        onPressed: () {
          if (prefs == null) return;
          prefs.showGeometryShellForAtomic = !prefs.showGeometryShellForAtomic;
          model.setPreferences(model.preferences!);
        },
      ),
    ]),
  ]);
}
