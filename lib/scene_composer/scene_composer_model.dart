import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';

class SceneComposerModel extends ChangeNotifier {
  SceneComposerView? sceneComposerView;

  SceneComposerModel() {}

  void selectCluster(BigInt clusterId, SelectModifier selectModifier) {
    selectClusterById(
      clusterId: clusterId,
      selectModifier: selectModifier,
    );
    refreshFromKernel();
  }

  void refreshFromKernel() {
    sceneComposerView = getSceneComposerView();
    notifyListeners();
  }
}
