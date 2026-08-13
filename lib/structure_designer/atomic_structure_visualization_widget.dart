import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'display_button_group.dart';
import 'structure_designer_model.dart';

/// The atomic-structure cluster of the DISPLAY panel: the visualization-method
/// radio group, then the scene transparency toggle.
DisplayGroupCluster atomicStructureVisualizationCluster(
    StructureDesignerModel model) {
  final prefs = model.preferences?.atomicStructureVisualizationPreferences;

  void setMethod(AtomicStructureVisualization visualization) {
    if (prefs == null) return;
    prefs.visualization = visualization;
    model.setPreferences(model.preferences!);
  }

  return DisplayGroupCluster([
    // Radio group: how atoms and bonds are drawn.
    DisplayButtonGroup([
      DisplayIconButton(
        key: const Key('atomic_vis_ball_and_stick'),
        // Using hub icon to represent atoms (circles) connected by bonds (lines)
        icon: Icons.hub,
        tooltip: 'Atomic visualization: Ball and Stick',
        isSelected:
            prefs?.visualization == AtomicStructureVisualization.ballAndStick,
        onPressed: () => setMethod(AtomicStructureVisualization.ballAndStick),
      ),
      DisplayIconButton(
        key: const Key('atomic_vis_space_filling'),
        icon: Icons.circle, // Using circle to represent space filling spheres
        tooltip: 'Atomic visualization: Space Filling',
        isSelected:
            prefs?.visualization == AtomicStructureVisualization.spaceFilling,
        onPressed: () => setMethod(AtomicStructureVisualization.spaceFilling),
      ),
    ]),
    // Toggle: ghosts the whole scene (impostor mode only). The alpha is set in
    // Preferences; this button just flips it on/off for a quick "see through
    // everything" look.
    DisplayButtonGroup([
      DisplayIconButton(
        key: const Key('atomic_vis_scene_transparency'),
        icon: Icons.opacity,
        tooltip: 'Make whole scene transparent (set alpha in Preferences)',
        isSelected: prefs?.sceneTransparencyEnabled ?? false,
        onPressed: () {
          if (prefs == null) return;
          prefs.sceneTransparencyEnabled = !prefs.sceneTransparencyEnabled;
          model.setPreferences(model.preferences!);
        },
      ),
    ]),
  ]);
}
