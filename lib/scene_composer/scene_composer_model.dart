import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/scene_composer_api_types.dart';
import 'package:flutter_cad/src/rust/api/scene_composer_api.dart'
    as scene_composer_api;
import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;
import 'package:vector_math/vector_math.dart' as vector_math;
import 'package:flutter_cad/common/api_utils.dart';

class SceneComposerModel extends ChangeNotifier {
  SceneComposerView? sceneComposerView;
  String alignToolStateText = '';
  String distanceToolStateText = '';
  AtomView? atomInfoView;

  SceneComposerModel() {
    refreshFromKernel();
  }

  void importXyz(String filePath) {
    scene_composer_api.importXyz(filePath: filePath);
    refreshFromKernel();
  }

  void newModel() {
    scene_composer_api.sceneComposerNewModel();
    refreshFromKernel();
  }

  void exportXyz(String filePath) {
    scene_composer_api.exportXyz(filePath: filePath);
    refreshFromKernel();
  }

  void selectClusterById(BigInt clusterId, SelectModifier selectModifier) {
    scene_composer_api.selectClusterById(
      clusterId: clusterId,
      selectModifier: selectModifier,
    );
    refreshFromKernel();
  }

  BigInt? selectClusterByRay(vector_math.Vector3 rayStart,
      vector_math.Vector3 rayDir, SelectModifier selectModifier) {
    final ret = scene_composer_api.selectClusterByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
      selectModifier: selectModifier,
    );
    refreshFromKernel();
    return ret;
  }

  void renameCluster(BigInt clusterId, String newName) {
    scene_composer_api.sceneComposerRenameCluster(
      clusterId: clusterId,
      newName: newName,
    );
    refreshFromKernel();
  }

  void setActiveTool(APISceneComposerTool tool) {
    scene_composer_api.setActiveSceneComposerTool(tool: tool);
    refreshFromKernel();
  }

  BigInt? selectAlignAtomByRay(
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    final ret = scene_composer_api.selectAlignAtomByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
    refreshFromKernel();
    return ret;
  }

  BigInt? selectDistanceAtomByRay(
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    final ret = scene_composer_api.selectDistanceAtomByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
    refreshFromKernel();
    return ret;
  }

  BigInt? selectAtomInfoAtomByRay(
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    final ret = scene_composer_api.selectAtomInfoAtomByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
    refreshFromKernel();
    return ret;
  }

  APITransform? getSelectedFrameTransform() {
    return scene_composer_api.getSelectedFrameTransform();
  }

  bool isFrameLockedToAtoms() {
    return scene_composer_api.isFrameLockedToAtoms();
  }

  void setFrameLockedToAtoms(bool locked) {
    scene_composer_api.setFrameLockedToAtoms(locked: locked);
    refreshFromKernel();
  }

  void setSelectedFrameTransform(APITransform transform) {
    print(
        "setSelectedFrameTransform ${transform.translation.x} ${transform.translation.y} ${transform.translation.z}");
    scene_composer_api.setSelectedFrameTransform(transform: transform);
    refreshFromKernel();
  }

  APITransform getCameraTransform() {
    return common_api.getCameraTransform();
  }

  void setCameraTransform(APITransform transform) {
    common_api.setCameraTransform(transform: transform);
    refreshFromKernel();
  }

  void translateAlongLocalAxis(int axisIndex, double translation) {
    scene_composer_api.translateAlongLocalAxis(
        axisIndex: axisIndex, translation: translation);
    refreshFromKernel();
  }

  void rotateAroundLocalAxis(int axisIndex, double angleDegrees) {
    scene_composer_api.rotateAroundLocalAxis(
        axisIndex: axisIndex, angleDegrees: angleDegrees);
    refreshFromKernel();
  }

  void undo() {
    scene_composer_api.sceneComposerUndo();
    refreshFromKernel();
  }

  void redo() {
    scene_composer_api.sceneComposerRedo();
    refreshFromKernel();
  }

  void refreshFromKernel() {
    sceneComposerView = scene_composer_api.getSceneComposerView();
    alignToolStateText = scene_composer_api.getAlignToolStateText();
    distanceToolStateText = scene_composer_api.getDistanceToolStateText();
    atomInfoView = scene_composer_api.getSceneComposerAtomInfo();
    notifyListeners();
  }
}
