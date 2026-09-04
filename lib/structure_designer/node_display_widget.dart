import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'display_button_group.dart';
import 'structure_designer_model.dart';

/// The node cluster of the DISPLAY panel: two radio groups, one choosing how
/// node output visibility is managed, one choosing what a node's title bar says
/// (`doc/design_node_names_in_ui.md` D6).
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
    // Node titles: type vs name. A radio group rather than a single toggle
    // because both states are equally "on" — the canvas always says *something*
    // in the title bar, and a lit/unlit toggle would imply the type is the
    // absence of a choice. Flipping it repaints; no node moves (D5).
    DisplayButtonGroup([
      DisplayIconButton(
        key: const Key('node_title_type'),
        icon: Icons.widgets_outlined,
        tooltip: 'Node titles: type names',
        isSelected: model.nodeTitleMode == NodeTitleMode.type,
        onPressed: () => model.setNodeTitleMode(NodeTitleMode.type),
      ),
      DisplayIconButton(
        key: const Key('node_title_name'),
        icon: Icons.label_outline,
        tooltip: 'Node titles: node names (Ctrl+Shift+N)',
        isSelected: model.nodeTitleMode == NodeTitleMode.name,
        onPressed: () => model.setNodeTitleMode(NodeTitleMode.name),
      ),
    ]),
  ]);
}
