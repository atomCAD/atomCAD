import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'atomic_structure_visualization_widget.dart';
import 'background_visualization_widget.dart';
import 'display_button_group.dart';
import 'geometry_visualization_widget.dart';
import 'mode_toggle_widget.dart';
import 'node_display_widget.dart';
import 'structure_designer_model.dart';

/// The contents of the sidebar's DISPLAY section: a bar of icon buttons
/// grouped by subject.
///
/// Assembly only — each cluster is built by the file that owns its subject, and
/// the grouping/separator/wrapping rules live in `display_button_group.dart`.
/// Adding a control means adding it to the cluster whose subject it belongs to;
/// only a genuinely new subject warrants a new cluster here.
class DisplayPanel extends StatelessWidget {
  final StructureDesignerModel model;

  /// Direct Editing Mode has no node network, so it drops the geometry and
  /// node-display-policy clusters.
  final bool directEditingMode;

  const DisplayPanel({
    super.key,
    required this.model,
    this.directEditingMode = false,
  });

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          return DisplayGroupBar(
            clusters: directEditingMode
                ? [
                    atomicStructureVisualizationCluster(model),
                    backgroundVisualizationCluster(model),
                    modeToggleCluster(model),
                  ]
                : [
                    geometryVisualizationCluster(model),
                    atomicStructureVisualizationCluster(model),
                    backgroundVisualizationCluster(model),
                    nodeDisplayCluster(model),
                    modeToggleCluster(model),
                  ],
          );
        },
      ),
    );
  }
}
