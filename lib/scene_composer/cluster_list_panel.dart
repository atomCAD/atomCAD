import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A widget that displays a list of clusters from the SceneComposerModel.
class ClusterListPanel extends StatelessWidget {
  final SceneComposerModel model;

  const ClusterListPanel({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<SceneComposerModel>(
        builder: (context, model, child) {
          final clusters = model.sceneComposerView?.clusters;

          if (clusters == null || clusters.isEmpty) {
            return const Center(
              child: Text('No clusters available'),
            );
          }

          return ListView.builder(
            itemCount: clusters.length,
            itemBuilder: (context, index) {
              final cluster = clusters[index];

              return ListTile(
                dense: true,
                visualDensity: AppSpacing.compactVerticalDensity,
                contentPadding:
                    const EdgeInsets.symmetric(horizontal: 12, vertical: 0),
                title: Text(
                  cluster.name,
                  style: AppTextStyles.regular,
                ),
                selected: cluster.selected,
                selectedTileColor: AppColors.selectionBackground,
                selectedColor: AppColors.selectionForeground,
                onTap: () {
                  // Determine the selection modifier based on pressed keys
                  final selectModifier =
                      HardwareKeyboard.instance.isControlPressed
                          ? SelectModifier.toggle
                          : HardwareKeyboard.instance.isShiftPressed
                              ? SelectModifier.expand
                              : SelectModifier.replace;

                  // Call the model's selectCluster method
                  model.selectClusterById(cluster.id, selectModifier);
                },
              );
            },
          );
        },
      ),
    );
  }
}
