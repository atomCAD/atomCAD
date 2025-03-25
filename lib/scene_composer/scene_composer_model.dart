import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/src/rust/api/simple.dart' as simple;
import 'package:vector_math/vector_math.dart' as vector_math;
import 'package:flutter_cad/common/api_utils.dart';

class SceneComposerModel extends ChangeNotifier {
  SceneComposerView? sceneComposerView;

  SceneComposerModel() {
    refreshFromKernel();
  }

  void importXyz(String filePath) {
    simple.importXyz(filePath: filePath);
    refreshFromKernel();
  }

  void selectClusterById(BigInt clusterId, SelectModifier selectModifier) {
    simple.selectClusterById(
      clusterId: clusterId,
      selectModifier: selectModifier,
    );
    refreshFromKernel();
  }

  BigInt? selectClusterByRay(vector_math.Vector3 rayStart,
      vector_math.Vector3 rayDir, SelectModifier selectModifier) {
    final ret = simple.selectClusterByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
      selectModifier: selectModifier,
    );
    refreshFromKernel();
    return ret;
  }

  void refreshFromKernel() {
    sceneComposerView = simple.getSceneComposerView();
    notifyListeners();
  }
}
