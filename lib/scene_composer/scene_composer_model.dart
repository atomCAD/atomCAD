import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/src/rust/api/simple.dart' as simple;
import 'package:vector_math/vector_math.dart' as vector_math;
import 'package:flutter_cad/common/api_utils.dart';

class SceneComposerModel extends ChangeNotifier {
  SceneComposerView? sceneComposerView;
  String alignToolStateText = '';

  SceneComposerModel() {
    refreshFromKernel();
  }

  void importXyz(String filePath) {
    simple.importXyz(filePath: filePath);
    refreshFromKernel();
  }

  void exportXyz(String filePath) {
    simple.exportXyz(filePath: filePath);
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

  void setActiveTool(APISceneComposerTool tool) {
    simple.setActiveSceneComposerTool(tool: tool);
    refreshFromKernel();
  }

  BigInt? selectAlignAtomByRay(
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    final ret = simple.selectAlignAtomByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
    refreshFromKernel();
    return ret;
  }

  APITransform? getSelectedFrameTransform() {
    return simple.getSelectedFrameTransform();
  }

  bool isFrameLockedToAtoms() {
    return simple.isFrameLockedToAtoms();
  }

  void setFrameLockedToAtoms(bool locked) {
    simple.setFrameLockedToAtoms(locked: locked);
    refreshFromKernel();
  }

  void setSelectedFrameTransform(APITransform transform) {
    print(
        "setSelectedFrameTransform ${transform.translation.x} ${transform.translation.y} ${transform.translation.z}");
    simple.setSelectedFrameTransform(transform: transform);
    refreshFromKernel();
  }

  APITransform getCameraTransform() {
    return simple.getCameraTransform();
  }

  void setCameraTransform(APITransform transform) {
    simple.setCameraTransform(transform: transform);
    refreshFromKernel();
  }

  void translateAlongLocalAxis(int axisIndex, double translation) {
    simple.translateAlongLocalAxis(
        axisIndex: axisIndex, translation: translation);
    refreshFromKernel();
  }

  void rotateAroundLocalAxis(int axisIndex, double angleDegrees) {
    simple.rotateAroundLocalAxis(
        axisIndex: axisIndex, angleDegrees: angleDegrees);
    refreshFromKernel();
  }

  void refreshFromKernel() {
    sceneComposerView = simple.getSceneComposerView();
    alignToolStateText = simple.getAlignToolStateText();
    notifyListeners();
  }
}
