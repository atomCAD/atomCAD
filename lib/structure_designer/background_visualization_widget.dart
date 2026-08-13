import 'package:flutter/material.dart';
import 'display_button_group.dart';
import 'structure_designer_model.dart';

/// The background cluster of the DISPLAY panel: the axes and grid toggles.
///
/// These mirror the *Show axes* / *Show grid* checkboxes in
/// **Edit → Preferences → Background**. They live here because they are flipped
/// constantly while composing a clean screenshot (issue #402), which a
/// scroll-and-hunt trip through the preferences dialog does not support.
///
/// Note that *Show lattice axes* deliberately stays in the dialog only: it is
/// subordinate to *Show axes* and is not part of the same quick rhythm.
DisplayGroupCluster backgroundVisualizationCluster(
    StructureDesignerModel model) {
  final prefs = model.preferences?.backgroundPreferences;

  return DisplayGroupCluster([
    // Two independent toggles, so one group with no separator between them.
    DisplayButtonGroup([
      DisplayIconButton(
        key: const Key('background_vis_show_axes'),
        icon: Icons.line_axis,
        tooltip: 'Show axes',
        isSelected: prefs?.showAxes ?? false,
        onPressed: () {
          if (prefs == null) return;
          prefs.showAxes = !prefs.showAxes;
          model.setPreferences(model.preferences!);
        },
      ),
      DisplayIconButton(
        key: const Key('background_vis_show_grid'),
        icon: Icons.grid_on,
        tooltip: 'Show grid',
        isSelected: prefs?.showGrid ?? false,
        onPressed: () {
          if (prefs == null) return;
          prefs.showGrid = !prefs.showGrid;
          model.setPreferences(model.preferences!);
        },
      ),
    ]),
  ]);
}
