import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';

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
              final entry = clusters.entries.elementAt(index);
              final cluster = entry.value;
              
              return ListTile(
                title: Text(cluster.name),
                selected: cluster.selected,
                selectedTileColor: Colors.blue.withOpacity(0.1),
              );
            },
          );
        },
      ),
    );
  }
}
