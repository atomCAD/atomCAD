import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:texture_rgba_renderer/texture_rgba_renderer.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter/scheduler.dart';
import 'package:vector_math/vector_math.dart' as vector_math;
import 'dart:math';

vector_math.Vector3 APIVec3ToVector3(APIVec3 v) {
  return vector_math.Vector3(v.x, v.y, v.z);
}

APIVec3 Vector3ToAPIVec3(vector_math.Vector3 v) {
  return APIVec3(x: v.x, y: v.y, z: v.z);
}

enum ViewportDragState {
  noDrag,
  primaryDrag, // drag with primary key (left click on windows). Used for click detection, rectangular select, bond creation, etc...
  move,
  rotate
}

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
vector_math.Vector3 rotatePointAroundAxis(vector_math.Vector3 axisPos, vector_math.Vector3 axis, double angle, vector_math.Vector3 point) {
    final rotation = vector_math.Quaternion.axisAngle(axis, angle);
    return rotation.rotated(point - axisPos) + axisPos;
} 

class CadViewport extends StatefulWidget {
  const CadViewport({super.key});

  @override
  _CadViewportState createState() => _CadViewportState();
}

class _CadViewportState extends State<CadViewport> {

  static const double _clickThreshold = 4.0;
  static const double _addAtomPlaneDistance = 20.0;

  //TODO: viewport should be resizable.
  static const double VIEWPORT_WIDTH = 1280.0;
  static const double VIEWPORT_HEIGHT = 704.0;

  // Rotation per pixel in radian
  static const double ROT_PER_PIXEL = 0.02; 

  // amount of relative zoom to the zoom target per reported zoom delta.
  static const double ZOOM_PER_ZOOM_DELTA = 0.0015;

  TextureRgbaRenderer? _textureRenderer;

  int? _textureId;
  int? _texturePtr;
  ViewportDragState _dragState = ViewportDragState.noDrag;
  Offset _dragStartPointerPos = Offset(0.0, 0.0);
  vector_math.Vector3 _pivotPoint = vector_math.Vector3(0.0, 0.0, 0.0);
  CameraTransform? _dragStartCameraTransform;
  double _cameraMovePerPixel = 0.0; 

  void _renderingNeeded() {
      SchedulerBinding.instance.scheduleFrame();
  }

  void _moveCameraAndRender({required APIVec3 eye, required APIVec3 target, required APIVec3 up}) {
    moveCamera(eye: eye, target: target, up: up);
    _renderingNeeded();
  }

  void initTexture() async {
    _textureRenderer = TextureRgbaRenderer();

    // Initialize the texture
    const int textureKey = 0;
    await _textureRenderer?.closeTexture(textureKey);
    var textureId = await _textureRenderer?.createTexture(textureKey);
    var texturePtr = await _textureRenderer?.getTexturePtr(textureKey);

    SchedulerBinding.instance.addPersistentFrameCallback(_handlePersistentFrame);

    setState(() {
      _textureId = textureId;
      _texturePtr = texturePtr;
    });
  }

  void _handlePersistentFrame(Duration timeStamp) {
    if(_texturePtr != null) {
      provideTexture(texturePtr: _texturePtr!);
    }
  }

  void _startMoveCamera(Offset pointerPos) {
    _dragState = ViewportDragState.move;
    _dragStartPointerPos = pointerPos;
    determinePivotPoint(pointerPos);
    final camera = getCamera();
    _dragStartCameraTransform = getCameraTransform(camera);

    var movePlaneDistance = (_pivotPoint - _dragStartCameraTransform!.eye).dot(_dragStartCameraTransform!.forward);
    _cameraMovePerPixel = 2.0 * movePlaneDistance * tan(camera!.fovy * 0.5) / VIEWPORT_HEIGHT;
  }

  void _moveCamera(Offset pointerPos) {
    if (_dragStartCameraTransform == null) {
      return;
    }
    var relPointerPos = pointerPos - _dragStartPointerPos;

    var newEye = _dragStartCameraTransform!.eye + _dragStartCameraTransform!.right * ((-_cameraMovePerPixel) * relPointerPos.dx)
      + _dragStartCameraTransform!.up * (_cameraMovePerPixel * relPointerPos.dy);
    var newTarget = newEye + _dragStartCameraTransform!.forward;

    _moveCameraAndRender(eye: Vector3ToAPIVec3(newEye), target: Vector3ToAPIVec3(newTarget), up: Vector3ToAPIVec3(_dragStartCameraTransform!.up));
  }

  void determinePivotPoint(Offset pointerPos) {

    var camera = getCamera();
    final cameraTransform = getCameraTransform(camera);

    final centeredPointerPos = pointerPos - Offset(VIEWPORT_WIDTH * 0.5, VIEWPORT_HEIGHT * 0.5);

    final d = VIEWPORT_HEIGHT * 0.5 / tan(camera!.fovy * 0.5);

    final rayDir = (cameraTransform!.right * centeredPointerPos.dx
      - cameraTransform.up * centeredPointerPos.dy
      + cameraTransform.forward * d).normalized();

    _pivotPoint = APIVec3ToVector3(findPivotPoint(rayStart: Vector3ToAPIVec3(cameraTransform.eye), rayDir: Vector3ToAPIVec3(rayDir)));
  }

  void _startRotateCamera(Offset pointerPos) {
    _dragState = ViewportDragState.rotate;
    _dragStartPointerPos = pointerPos;
    determinePivotPoint(pointerPos);
  }

  void _rotateCamera(Offset pointerPos) {
    var relPointerPos = pointerPos - _dragStartPointerPos;
    
    final cameraTransform = getCameraTransform(getCamera());

    // Horizontal component
    // Rotate around up vector

    final horizAngle = relPointerPos.dx * ROT_PER_PIXEL;
    final vertAxis = cameraTransform!.up;
    var newEye = rotatePointAroundAxis(_pivotPoint, vertAxis, horizAngle, cameraTransform.eye);
    var newTarget = rotatePointAroundAxis(_pivotPoint, vertAxis, horizAngle, cameraTransform.target);

    final newForward = (newTarget - newEye).normalized();
    final newRight = newForward.cross(cameraTransform.up).normalized();

    // Vertical component
    // Rotate around our right vector
    final vertAngle = relPointerPos.dy * ROT_PER_PIXEL;
    final horizAxis = newRight;

    newEye = rotatePointAroundAxis(_pivotPoint, horizAxis, vertAngle, newEye);
    newTarget = rotatePointAroundAxis(_pivotPoint, horizAxis, vertAngle, newTarget);
    final newUp = vector_math.Quaternion.axisAngle(horizAxis, vertAngle).rotated(cameraTransform.up);

    _moveCameraAndRender(eye: Vector3ToAPIVec3(newEye), target: Vector3ToAPIVec3(newTarget), up: Vector3ToAPIVec3(newUp));

    _dragStartPointerPos = pointerPos;
  }

  void _startPrimaryDrag(Offset pointerPos) {
    _dragState = ViewportDragState.primaryDrag;
    _dragStartPointerPos = pointerPos;
  }

  void _endDrag(Offset pointerPos) {
    var wasClick = ((pointerPos - _dragStartPointerPos).distance < _clickThreshold);
    if(wasClick) {
      _onClick(pointerPos);
    }
    _dragState = ViewportDragState.noDrag;    
  }

  void _onClick(Offset pointerPos) {
    if (_dragState == ViewportDragState.primaryDrag) {
      _onPrimaryClick(pointerPos);
    }
  }

  void _onPrimaryClick(Offset pointerPos) {
    //print('primary click at ${pointerPos}');

    //TODO: raycast into the model.
    // if do not hit an atom, do the add atom stuff..
    // for now immediately invoke _addAtom
    _addAtom(pointerPos);
  }

  // Add an atom at a fix distance from the camera eye
  void _addAtom(Offset pointerPos) {
    // First determine the position of the atom to be placed.
    var camera = getCamera();
    final cameraTransform = getCameraTransform(camera);

    var offsetPerPixel = 2.0 * _addAtomPlaneDistance * tan(camera!.fovy * 0.5) / VIEWPORT_HEIGHT;

    var centeredPointerPos = pointerPos - Offset(VIEWPORT_WIDTH * 0.5, VIEWPORT_HEIGHT * 0.5);

    var atomPos = cameraTransform!.eye + cameraTransform.forward * _addAtomPlaneDistance + cameraTransform.right * (offsetPerPixel * centeredPointerPos.dx) + cameraTransform.up * (offsetPerPixel * (-centeredPointerPos.dy));

    addAtom(atomicNumber: 6, position: Vector3ToAPIVec3(atomPos));
    _renderingNeeded();
  }

  void _scroll(Offset pointerPos, double scrollDeltaY) {
    if (_dragState != ViewportDragState.noDrag) { // Do not interfere with move or rotate
      return;
    }

    determinePivotPoint(pointerPos);
    final camera = getCamera();
    final cameraTransform = getCameraTransform(camera);

    final zoomTargetPlaneDistance = (_pivotPoint - cameraTransform!.eye).dot(cameraTransform.forward);

    final moveVec = cameraTransform.forward * (ZOOM_PER_ZOOM_DELTA * ( - scrollDeltaY) * zoomTargetPlaneDistance);

    final newEye = cameraTransform.eye + moveVec;
    final newTarget = cameraTransform.target + moveVec;

    _moveCameraAndRender(eye: Vector3ToAPIVec3(newEye), target: Vector3ToAPIVec3(newTarget), up: Vector3ToAPIVec3(cameraTransform.up));
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

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        _textureId != null
      ? Center(
          child: SizedBox(
            width: VIEWPORT_WIDTH,
            height: VIEWPORT_HEIGHT,
            child: Listener(
              onPointerSignal: (pointerSignal){
                if (pointerSignal is PointerScrollEvent) {
                  _scroll(pointerSignal.localPosition, pointerSignal.scrollDelta.dy);
                  //print('Scrolled: ${scrollEvent.scrollDelta}');
                }
              },
              onPointerDown: (PointerDownEvent event) {
                if (event.kind == PointerDeviceKind.mouse) {
                  switch (event.buttons) {
                    case kPrimaryMouseButton:
                      //print('Left mouse button pressed at ${event.position}');
                      _startPrimaryDrag(event.localPosition);                      
                      break;
                    case kSecondaryMouseButton:
                      //print('Right mouse button pressed at ${event.position}');
                      _startRotateCamera(event.localPosition);
                      break;
                    case kMiddleMouseButton:
                      //print('Middle mouse button pressed at ${event.position}');
                      _startMoveCamera(event.localPosition);
                      break;
                  }
                }
              },
              onPointerMove: (PointerMoveEvent event) {
                if (event.kind == PointerDeviceKind.mouse) {
                  //print('Mouse moved to ${event.position}');
                  switch(_dragState) {
                    case  ViewportDragState.move:
                      _moveCamera(event.localPosition);
                      break;
                    case ViewportDragState.rotate:
                      _rotateCamera(event.localPosition);
                      break;
                    default:
                  }
                }
              },
              onPointerUp: (PointerUpEvent event) {
                if (event.kind == PointerDeviceKind.mouse) {
                  //print('Mouse button released at ${event.position}');
                  _endDrag(event.localPosition);
                }
              },
              child: Texture(
                textureId: _textureId!,
              ),
            ),
          )
        )
      : Container(
          color: Colors.grey,
        ),
      ],
    );
  }
}
