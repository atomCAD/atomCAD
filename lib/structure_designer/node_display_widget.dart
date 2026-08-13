import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'display_button_group.dart';
import 'structure_designer_model.dart';

/// The node-display-policy cluster of the DISPLAY panel: a single radio group
/// choosing how node output visibility is managed.
DisplayGroupCluster nodeDisplayCluster(StructureDesignerModel model) {
  final prefs = model.preferences?.nodeDisplayPreferences;

  void setPolicy(NodeDisplayPolicy policy) {
    if (prefs == null) return;
    prefs.displayPolicy = policy;
    model.setPreferences(model.preferences!);
  }

  return DisplayGroupCluster([
    DisplayButtonGroup([
      DisplayIconButton(
        key: const Key('node_display_manual'),
        icon: Icons.tune, // Using tune icon to represent manual control
        tooltip: 'Node display policy: Manual (User Selection)',
        isSelected: prefs?.displayPolicy == NodeDisplayPolicy.manual,
        onPressed: () => setPolicy(NodeDisplayPolicy.manual),
      ),
      DisplayIconButton(
        key: const Key('node_display_prefer_selected'),
        icon: Icons.star, // Using star icon to represent selected items
        tooltip: 'Node display policy: Prefer Selected Nodes',
        isSelected: prefs?.displayPolicy == NodeDisplayPolicy.preferSelected,
        onPressed: () => setPolicy(NodeDisplayPolicy.preferSelected),
      ),
      DisplayIconButton(
        key: const Key('node_display_prefer_frontier'),
        icon: Icons.explore, // Using explore icon for frontier/boundary
        tooltip: 'Node display policy: Prefer Frontier Nodes',
        isSelected: prefs?.displayPolicy == NodeDisplayPolicy.preferFrontier,
        onPressed: () => setPolicy(NodeDisplayPolicy.preferFrontier),
      ),
    ]),
  ]);
}
