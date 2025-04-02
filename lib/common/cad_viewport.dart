import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:texture_rgba_renderer/texture_rgba_renderer.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter/scheduler.dart';
import 'package:vector_math/vector_math.dart' as vector_math;
import 'dart:math';

enum ViewportDragState {
  noDrag,
  defaultDrag, // drag with primary key (left click on windows). Used for click detection, rectangular select, bond creation, etc...
  move,
  rotate
}

// Flutter-side camera transform
class CameraTransform {
  vector_math.Vector3 eye;
  vector_math.Vector3 target;
  vector_math.Vector3 forward;
  vector_math.Vector3 up;
  vector_math.Vector3 right;

  CameraTransform({
    required this.eye,
    required this.target,
    required this.forward,
    required this.up,
    required this.right,
  });
}

class Ray {
  vector_math.Vector3 start;
  vector_math.Vector3 direction;
  Ray({required this.start, required this.direction});
}

CameraTransform? getCameraTransform(APICamera? camera) {
  if (camera == null) {
    return null;
  }
  final eye = APIVec3ToVector3(camera.eye);
  final target = APIVec3ToVector3(camera.target);
  final forward = (target - eye).normalized();
  final up = APIVec3ToVector3(camera.up);
  final right = forward.cross(up);

  return CameraTransform(
    eye: eye,
    target: target,
    forward: forward,
    up: up,
    right: right,
  );
}

// Axis is a normal vector
vector_math.Vector3 rotatePointAroundAxis(vector_math.Vector3 axisPos,
    vector_math.Vector3 axis, double angle, vector_math.Vector3 point) {
  final rotation = vector_math.Quaternion.axisAngle(axis, angle);
  return rotation.rotated(point - axisPos) + axisPos;
}

abstract class CadViewport extends StatefulWidget {
  const CadViewport({super.key});
}

abstract class CadViewportState<T extends CadViewport> extends State<T> {
  static const double _clickThreshold = 4.0;
  static const double _addAtomPlaneDistance = 20.0;

  // These initial values get overwritten
  double viewportWidth = 1280.0;
  double viewportHeight = 544.0;

  // Rotation per pixel in radian
  static const double ROT_PER_PIXEL = 0.02;

  // amount of relative zoom to the zoom target per reported zoom delta.
  static const double ZOOM_PER_ZOOM_DELTA = 0.0015;

  TextureRgbaRenderer? _textureRenderer;

  int? textureId;
  int? _texturePtr;
  ViewportDragState dragState = ViewportDragState.noDrag;
  Offset _dragStartPointerPos = Offset(0.0, 0.0);
  vector_math.Vector3 _pivotPoint = vector_math.Vector3(0.0, 0.0, 0.0);
  CameraTransform? _dragStartCameraTransform;
  double _cameraMovePerPixel = 0.0;

  bool isGadgetDragging = false;

  int draggedGadgetHandle =
      -1; // Relevant when _dragState == ViewportDragState.gadgetDrag

  void onPointerSignal(PointerSignalEvent pointerSignal) {
    if (pointerSignal is PointerScrollEvent) {
      scroll(pointerSignal.localPosition, pointerSignal.scrollDelta.dy);
      //print('Scrolled: ${scrollEvent.scrollDelta}');
    }
  }

  void onPointerDown(PointerDownEvent event) {
    if (event.kind == PointerDeviceKind.mouse) {
      switch (event.buttons) {
        case kPrimaryMouseButton:
          //print('Left mouse button pressed at ${event.position}');
          startPrimaryDrag(event.localPosition);
          break;
        case kSecondaryMouseButton:
          //print('Right mouse button pressed at ${event.position}');
          startRotateCamera(event.localPosition);
          break;
        case kMiddleMouseButton:
          //print('Middle mouse button pressed at ${event.position}');
          startMoveCamera(event.localPosition);
          break;
      }
    }
  }

  void onPointerMove(PointerMoveEvent event) {
    if (event.kind == PointerDeviceKind.mouse) {
      //print('Mouse moved to ${event.position}');
      switch (dragState) {
        case ViewportDragState.move:
          cameraMove(event.localPosition);
          break;
        case ViewportDragState.rotate:
          rotateCamera(event.localPosition);
          break;
        case ViewportDragState.defaultDrag:
          defaultDrag(event.localPosition);
          break;
        default:
      }
    }
  }

  void onPointerUp(PointerUpEvent event) {
    if (event.kind == PointerDeviceKind.mouse) {
      //print('Mouse button released at ${event.position}');
      endDrag(event.localPosition);
    }
  }

  void renderingNeeded() {
    SchedulerBinding.instance.scheduleFrame();
  }

  void _moveCameraAndRender(
      {required APIVec3 eye, required APIVec3 target, required APIVec3 up}) {
    moveCamera(eye: eye, target: target, up: up);
    refreshFromKernel();
    renderingNeeded();
  }

  Ray getRayFromPointerPos(Offset pointerPos) {
    var camera = getCamera();
    final cameraTransform = getCameraTransform(camera);

    final centeredPointerPos =
        pointerPos - Offset(viewportWidth * 0.5, viewportHeight * 0.5);

    final d = viewportHeight * 0.5 / tan(camera!.fovy * 0.5);

    final rayDir = (cameraTransform!.right * centeredPointerPos.dx -
            cameraTransform.up * centeredPointerPos.dy +
            cameraTransform.forward * d)
        .normalized();

    return Ray(start: cameraTransform.eye, direction: rayDir);
  }

  void initTexture() async {
    _textureRenderer = TextureRgbaRenderer();

    // Initialize the texture
    const int textureKey = 0;
    await _textureRenderer?.closeTexture(textureKey);
    var myTextureId = await _textureRenderer?.createTexture(textureKey);
    var texturePtr = await _textureRenderer?.getTexturePtr(textureKey);

    SchedulerBinding.instance
        .addPersistentFrameCallback(_handlePersistentFrame);

    setState(() {
      textureId = myTextureId;
      _texturePtr = texturePtr;
    });
  }

  void _handlePersistentFrame(Duration timeStamp) {
    if (_texturePtr != null) {
      provideTexture(texturePtr: _texturePtr!);
    }
  }

  void startMoveCamera(Offset pointerPos) {
    dragState = ViewportDragState.move;
    _dragStartPointerPos = pointerPos;
    determinePivotPoint(pointerPos);
    final camera = getCamera();
    _dragStartCameraTransform = getCameraTransform(camera);

    var movePlaneDistance = (_pivotPoint - _dragStartCameraTransform!.eye)
        .dot(_dragStartCameraTransform!.forward);
    _cameraMovePerPixel =
        2.0 * movePlaneDistance * tan(camera!.fovy * 0.5) / viewportHeight;
  }

  void cameraMove(Offset pointerPos) {
    if (_dragStartCameraTransform == null) {
      return;
    }
    var relPointerPos = pointerPos - _dragStartPointerPos;

    var newEye = _dragStartCameraTransform!.eye +
        _dragStartCameraTransform!.right *
            ((-_cameraMovePerPixel) * relPointerPos.dx) +
        _dragStartCameraTransform!.up *
            (_cameraMovePerPixel * relPointerPos.dy);
    var newTarget = newEye + _dragStartCameraTransform!.forward;

    _moveCameraAndRender(
        eye: Vector3ToAPIVec3(newEye),
        target: Vector3ToAPIVec3(newTarget),
        up: Vector3ToAPIVec3(_dragStartCameraTransform!.up));
  }

  void determinePivotPoint(Offset pointerPos) {
    final ray = getRayFromPointerPos(pointerPos);

    _pivotPoint = APIVec3ToVector3(findPivotPoint(
        rayStart: Vector3ToAPIVec3(ray.start),
        rayDir: Vector3ToAPIVec3(ray.direction)));
  }

  void startRotateCamera(Offset pointerPos) {
    dragState = ViewportDragState.rotate;
    _dragStartPointerPos = pointerPos;
    determinePivotPoint(pointerPos);
  }

  void rotateCamera(Offset pointerPos) {
    var relPointerPos = pointerPos - _dragStartPointerPos;

    final cameraTransform = getCameraTransform(getCamera());

    // Horizontal component
    // Rotate around up vector

    final horizAngle = relPointerPos.dx * ROT_PER_PIXEL;
    final vertAxis = cameraTransform!.up;
    var newEye = rotatePointAroundAxis(
        _pivotPoint, vertAxis, horizAngle, cameraTransform.eye);
    var newTarget = rotatePointAroundAxis(
        _pivotPoint, vertAxis, horizAngle, cameraTransform.target);

    final newForward = (newTarget - newEye).normalized();
    final newRight = newForward.cross(cameraTransform.up).normalized();

    // Vertical component
    // Rotate around our right vector
    final vertAngle = relPointerPos.dy * ROT_PER_PIXEL;
    final horizAxis = newRight;

    newEye = rotatePointAroundAxis(_pivotPoint, horizAxis, vertAngle, newEye);
    newTarget =
        rotatePointAroundAxis(_pivotPoint, horizAxis, vertAngle, newTarget);
    final newUp = vector_math.Quaternion.axisAngle(horizAxis, vertAngle)
        .rotated(cameraTransform.up);

    _moveCameraAndRender(
        eye: Vector3ToAPIVec3(newEye),
        target: Vector3ToAPIVec3(newTarget),
        up: Vector3ToAPIVec3(newUp));

    _dragStartPointerPos = pointerPos;
  }

  void defaultDrag(Offset pointerPos) {
    if (isGadgetDragging) {
      dragGadget(pointerPos);
    }
  }

  void dragGadget(Offset pointerPos) {
    final ray = getRayFromPointerPos(pointerPos);
    gadgetDrag(
        nodeNetworkName: "sample", // TODO: this should not be needed
        handleIndex: draggedGadgetHandle,
        rayOrigin: Vector3ToAPIVec3(ray.start),
        rayDirection: Vector3ToAPIVec3(ray.direction));
    syncGadgetData(nodeNetworkName: "sample");
    renderingNeeded();
    refreshFromKernel(); // Refresh other widgets when dragging a gadget
  }

  void refreshFromKernel() {}

  void startPrimaryDrag(Offset pointerPos) {
    dragState = ViewportDragState.defaultDrag;
    _dragStartPointerPos = pointerPos;

    final ray = getRayFromPointerPos(pointerPos);

    final hitResult = gadgetHitTest(
        rayOrigin: Vector3ToAPIVec3(ray.start),
        rayDirection: Vector3ToAPIVec3(ray.direction));

    if (hitResult != null) {
      print("Hit result: $hitResult");
      isGadgetDragging = true;
      draggedGadgetHandle = transformDraggedGadgetHandle(hitResult);
      gadgetStartDrag(
          nodeNetworkName: "sample", // TODO: this should not be needed
          handleIndex: draggedGadgetHandle,
          rayOrigin: Vector3ToAPIVec3(ray.start),
          rayDirection: Vector3ToAPIVec3(ray.direction));
      renderingNeeded();
    }
  }

  int transformDraggedGadgetHandle(int handleIndex) {
    return handleIndex;
  }

  void endDrag(Offset pointerPos) {
    final oldDragState = dragState;
    var wasClick =
        ((pointerPos - _dragStartPointerPos).distance < _clickThreshold);
    if (wasClick) {
      onClick(pointerPos);
    }

    dragState = ViewportDragState.noDrag;

    if (oldDragState == ViewportDragState.defaultDrag && isGadgetDragging) {
      gadgetEndDrag(
          nodeNetworkName: "sample"); // TODO: this should not be needed
      renderingNeeded();
      isGadgetDragging = false;
    }
  }

  void onClick(Offset pointerPos) {
    if (dragState == ViewportDragState.defaultDrag) {
      onDefaultClick(pointerPos);
    }
  }

  void onDefaultClick(Offset pointerPos) {
    //print('primary click at ${pointerPos}');

    //TODO: raycast into the model.
    // if do not hit an atom, do the add atom stuff..
    // for now immediately invoke _addAtom
    //_addAtom(pointerPos);
  }

  // Add an atom at a fix distance from the camera eye
  void _addAtom(Offset pointerPos) {
    // First determine the position of the atom to be placed.
    var camera = getCamera();
    final cameraTransform = getCameraTransform(camera);

    var offsetPerPixel =
        2.0 * _addAtomPlaneDistance * tan(camera!.fovy * 0.5) / viewportHeight;

    var centeredPointerPos =
        pointerPos - Offset(viewportWidth * 0.5, viewportHeight * 0.5);

    var atomPos = cameraTransform!.eye +
        cameraTransform.forward * _addAtomPlaneDistance +
        cameraTransform.right * (offsetPerPixel * centeredPointerPos.dx) +
        cameraTransform.up * (offsetPerPixel * (-centeredPointerPos.dy));

    addAtom(atomicNumber: 6, position: Vector3ToAPIVec3(atomPos));
    renderingNeeded();
  }

  void scroll(Offset pointerPos, double scrollDeltaY) {
    if (dragState != ViewportDragState.noDrag) {
      // Do not interfere with move or rotate
      return;
    }

    determinePivotPoint(pointerPos);
    final camera = getCamera();
    final cameraTransform = getCameraTransform(camera);

    final zoomTargetPlaneDistance =
        (_pivotPoint - cameraTransform!.eye).dot(cameraTransform.forward);

    final moveVec = cameraTransform.forward *
        (ZOOM_PER_ZOOM_DELTA * (-scrollDeltaY) * zoomTargetPlaneDistance);

    final newEye = cameraTransform.eye + moveVec;
    final newTarget = cameraTransform.target + moveVec;

    _moveCameraAndRender(
        eye: Vector3ToAPIVec3(newEye),
        target: Vector3ToAPIVec3(newTarget),
        up: Vector3ToAPIVec3(cameraTransform.up));
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        textureId != null
            ? LayoutBuilder(builder: (context, constraints) {
                viewportWidth = constraints.maxWidth;
                viewportHeight = constraints.maxHeight;
                setViewportSize(
                    width: viewportWidth.toInt(),
                    height: viewportHeight.toInt());

                return Listener(
                  onPointerSignal: (pointerSignal) {
                    onPointerSignal(pointerSignal);
                  },
                  onPointerDown: (PointerDownEvent event) {
                    onPointerDown(event);
                  },
                  onPointerMove: (PointerMoveEvent event) {
                    onPointerMove(event);
                  },
                  onPointerUp: (PointerUpEvent event) {
                    onPointerUp(event);
                  },
                  child: Texture(
                    textureId: textureId!,
                  ),
                );
              })
            : Container(
                color: Colors.grey,
              ),
      ],
    );
  }

  @override
  void initState() {
    super.initState();
    initTexture();
  }

  @override
  void dispose() {
    super.dispose();
  }
}
