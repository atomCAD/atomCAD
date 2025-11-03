import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:texture_rgba_renderer/texture_rgba_renderer.dart';
import 'package:flutter_cad/src/rust/api/common_api.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
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
  vector_math.Vector3 pivotPoint;

  CameraTransform({
    required this.eye,
    required this.target,
    required this.forward,
    required this.up,
    required this.right,
    required this.pivotPoint,
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
    pivotPoint: APIVec3ToVector3(camera.pivotPoint),
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

  // Handle trackpad/Magic Mouse pan-zoom start
  void onPointerPanZoomStart(PointerPanZoomStartEvent event) {
    print('PanZoomStart - localPosition: ${event.localPosition}, pointer: ${event.pointer}');
    // Initialize pan-zoom gesture if needed
  }

  // Handle trackpad/Magic Mouse pan-zoom updates (includes zoom)
  void onPointerPanZoomUpdate(PointerPanZoomUpdateEvent event) {
    print('PanZoomUpdate - localPosition: ${event.localPosition}, scale: ${event.scale}, pan: ${event.pan}, panDelta: ${event.panDelta}, rotation: ${event.rotation}');
    
    // Handle zoom from multiple sources
    bool zoomHandled = false;
    
    // 1. Trackpad pinch-to-zoom (scale changes)
    if (event.scale != 1.0) {
      final scaleDelta = event.scale - 1.0;
      final scrollDelta = -scaleDelta * 250.0; // Reduced sensitivity (was 1000.0)
      print('  -> Trackpad pinch: scaleDelta: $scaleDelta to scrollDelta: $scrollDelta');
      scroll(event.localPosition, scrollDelta);
      zoomHandled = true;
    }
    
    // 2. Magic Mouse / Trackpad vertical pan for zoom (when no scale change)
    if (!zoomHandled && event.panDelta.dy.abs() > 0.1) {
      // Use vertical pan delta for zooming
      final scrollDelta = event.panDelta.dy * 2.0; // Adjust sensitivity as needed
      print('  -> Vertical pan zoom: panDelta.dy: ${event.panDelta.dy} to scrollDelta: $scrollDelta');
      scroll(event.localPosition, scrollDelta);
    }
    
    // Note: We don't use panDelta.dx for panning to avoid confusion
    // All users (including trackpad) should use Shift + Right mouse drag for panning
  }

  // Handle trackpad/Magic Mouse pan-zoom end
  void onPointerPanZoomEnd(PointerPanZoomEndEvent event) {
    print('PanZoomEnd - localPosition: ${event.localPosition}, pointer: ${event.pointer}');
    // Clean up pan-zoom gesture if needed
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
          // Check for Shift + Right mouse for panning
          if (HardwareKeyboard.instance.isShiftPressed) {
            print('Shift + Right mouse: starting camera move');
            startMoveCamera(event.localPosition);
          } else {
            startRotateCamera(event.localPosition);
          }
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

    if (camera!.orthographic) {
      // Orthographic mode: rays are parallel and start from the near plane
      // Calculate width from height and aspect ratio
      final orthoHalfWidth = camera.orthoHalfHeight * camera.aspect;

      // Calculate position on near plane (scaled by orthoHalfWidth/orthoHalfHeight)
      final xOffset =
          (centeredPointerPos.dx / (viewportWidth * 0.5)) * orthoHalfWidth;
      final yOffset = (centeredPointerPos.dy / (viewportHeight * 0.5)) *
          camera.orthoHalfHeight;

      // Ray starts from a point on the near plane
      final rayStart = cameraTransform!.eye +
          cameraTransform.right * xOffset -
          cameraTransform.up * yOffset;

      // Ray direction is always the forward vector in orthographic mode
      return Ray(start: rayStart, direction: cameraTransform.forward);
    } else {
      // Perspective mode (original implementation)
      final d = viewportHeight * 0.5 / tan(camera.fovy * 0.5);

      final rayDir = (cameraTransform!.right * centeredPointerPos.dx -
              cameraTransform.up * centeredPointerPos.dy +
              cameraTransform.forward * d)
          .normalized();

      return Ray(start: cameraTransform.eye, direction: rayDir);
    }
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

    // Check if widget is still mounted before calling setState
    if (mounted) {
      setState(() {
        textureId = myTextureId;
        _texturePtr = texturePtr;
      });
    }
  }

  void _handlePersistentFrame(Duration timeStamp) {
    // Check if widget is still mounted to prevent errors
    if (mounted && _texturePtr != null) {
      provideTexture(texturePtr: _texturePtr!);
    }
  }

  void startMoveCamera(Offset pointerPos) {
    // Adjust camera target based on what's under the cursor
    final ray = getRayFromPointerPos(pointerPos);
    adjustCameraTarget(
        rayOrigin: Vector3ToAPIVec3(ray.start),
        rayDirection: Vector3ToAPIVec3(ray.direction));

    dragState = ViewportDragState.move;
    _dragStartPointerPos = pointerPos;
    final camera = getCamera();
    _dragStartCameraTransform = getCameraTransform(camera);

    if (camera!.orthographic) {
      // In orthographic mode, movement scale is based directly on orthoHalfHeight
      // and viewport dimensions
      _cameraMovePerPixel = 2.0 * camera.orthoHalfHeight / viewportHeight;
    } else {
      // Original perspective mode calculation
      var movePlaneDistance = (_dragStartCameraTransform!.pivotPoint -
              _dragStartCameraTransform!.eye)
          .dot(_dragStartCameraTransform!.forward);
      _cameraMovePerPixel =
          2.0 * movePlaneDistance * tan(camera.fovy * 0.5) / viewportHeight;
    }
  }

  void cameraMove(Offset pointerPos) {
    if (_dragStartCameraTransform == null) {
      return;
    }
    var relPointerPos = pointerPos - _dragStartPointerPos;

    var moveDelta = _dragStartCameraTransform!.right *
            ((-_cameraMovePerPixel) * relPointerPos.dx) +
        _dragStartCameraTransform!.up *
            (_cameraMovePerPixel * relPointerPos.dy);

    var newEye = _dragStartCameraTransform!.eye + moveDelta;
    var newTarget = _dragStartCameraTransform!.target + moveDelta;

    _moveCameraAndRender(
        eye: Vector3ToAPIVec3(newEye),
        target: Vector3ToAPIVec3(newTarget),
        up: Vector3ToAPIVec3(_dragStartCameraTransform!.up));
  }

  void startRotateCamera(Offset pointerPos) {
    // Adjust camera target based on what's under the cursor
    final ray = getRayFromPointerPos(pointerPos);
    adjustCameraTarget(
        rayOrigin: Vector3ToAPIVec3(ray.start),
        rayDirection: Vector3ToAPIVec3(ray.direction));

    dragState = ViewportDragState.rotate;
    _dragStartPointerPos = pointerPos;
  }

  /// Calculates the rotation angle between two vectors around a specified axis.
  /// Projects both vectors onto the plane perpendicular to the axis and calculates
  /// the signed angle between them.
  ///
  /// [fromVector] - The starting vector
  /// [toVector] - The ending vector
  /// [axisVector] - The axis around which to measure rotation
  ///
  /// Returns the rotation angle in radians
  double calculateRotationAngleAroundAxis(vector_math.Vector3 fromVector,
      vector_math.Vector3 toVector, vector_math.Vector3 axisVector) {
    // Create two orthogonal vectors perpendicular to the axis
    vector_math.Vector3 perpendicular1;
    vector_math.Vector3 perpendicular2;

    // Find a vector that's not parallel to the axis
    if (axisVector.x.abs() < 0.9) {
      perpendicular1 =
          vector_math.Vector3(1.0, 0.0, 0.0).cross(axisVector).normalized();
    } else {
      perpendicular1 =
          vector_math.Vector3(0.0, 0.0, 1.0).cross(axisVector).normalized();
    }
    perpendicular2 = axisVector.cross(perpendicular1).normalized();

    // Project both vectors onto the plane perpendicular to the axis
    final fromProj = vector_math.Vector2(
            fromVector.dot(perpendicular1), fromVector.dot(perpendicular2))
        .normalized();

    final toProj = vector_math.Vector2(
            toVector.dot(perpendicular1), toVector.dot(perpendicular2))
        .normalized();

    // Calculate the rotation angle using atan2
    final fromAngle = atan2(fromProj.y, fromProj.x);
    final toAngle = atan2(toProj.y, toProj.x);
    return toAngle - fromAngle;
  }

  void rotateCamera(Offset pointerPos) {
    var relPointerPos = pointerPos - _dragStartPointerPos;

    final camera = getCamera();
    final cameraTransform = getCameraTransform(camera);

    // Horizontal component - rotate around global up vector (Z-up)
    final horizAngle = relPointerPos.dx * ROT_PER_PIXEL;
    final vertAxis = vector_math.Vector3(0.0, 0.0, 1.0);
    var newEye = rotatePointAroundAxis(
        cameraTransform!.pivotPoint, vertAxis, horizAngle, cameraTransform.eye);

    final newForward = rotatePointAroundAxis(vector_math.Vector3.zero(),
        vertAxis, horizAngle, cameraTransform.forward);
    final newRight = newForward.cross(cameraTransform.up).normalized();

    // Vertical component - rotate around right vector
    final vertAngle = relPointerPos.dy * ROT_PER_PIXEL;
    newEye = rotatePointAroundAxis(
        cameraTransform.pivotPoint, newRight, vertAngle, newEye);

    final newForward2 = rotatePointAroundAxis(
        vector_math.Vector3.zero(), newRight, vertAngle, newForward);

    // Global up vector (Z-up)
    final globalUp = vector_math.Vector3(0.0, 0.0, 1.0);

    // Calculate right vector as cross product of forward and global up
    final newRight2 = newForward2.cross(globalUp).normalized();

    // Calculate correct up vector to avoid roll
    vector_math.Vector3 newUp;
    if (newRight2.length < 0.001) {
      // When looking directly up/down, use previous right vector
      final prevRight = cameraTransform.up.cross(newForward2).normalized();
      newUp = newForward2.cross(prevRight).normalized();
    } else {
      // Calculate up vector as cross product of right and forward
      newUp = newRight2.cross(newForward2).normalized();
    }

    _moveCameraAndRender(
        eye: Vector3ToAPIVec3(newEye),
        target: Vector3ToAPIVec3(newEye + newForward2),
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
        handleIndex: draggedGadgetHandle,
        rayOrigin: Vector3ToAPIVec3(ray.start),
        rayDirection: Vector3ToAPIVec3(ray.direction));
    syncGadgetData();
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
      isGadgetDragging = true;
      draggedGadgetHandle = transformDraggedGadgetHandle(hitResult);
      gadgetStartDrag(
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
    if (wasClick && (!isGadgetDragging)) {
      onClick(pointerPos);
    }

    dragState = ViewportDragState.noDrag;

    if (oldDragState == ViewportDragState.defaultDrag && isGadgetDragging) {
      gadgetEndDrag();
      renderingNeeded();
      refreshFromKernel();
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

/*
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
*/
  void scroll(Offset pointerPos, double scrollDeltaY) {
    if (dragState != ViewportDragState.noDrag) {
      // Do not interfere with move or rotate
      return;
    }

    // Adjust camera target based on what's under the cursor
    final ray = getRayFromPointerPos(pointerPos);
    adjustCameraTarget(
        rayOrigin: Vector3ToAPIVec3(ray.start),
        rayDirection: Vector3ToAPIVec3(ray.direction));

    final camera = getCamera();

    if (camera!.orthographic) {
      // For orthographic projection, we adjust orthoHalfHeight instead of moving the camera
      // Positive scrollDeltaY = zoom out = increase orthoHalfHeight
      // Negative scrollDeltaY = zoom in = decrease orthoHalfHeight

      // Get the current value
      final currentHalfHeight = camera.orthoHalfHeight;

      // Calculate the zoom factor (scrolling down increases size, up decreases)
      final zoomFactor = 1.0 + ZOOM_PER_ZOOM_DELTA * scrollDeltaY;

      // Apply the zoom factor with limits to prevent extreme zoom in/out
      final newHalfHeight = currentHalfHeight * zoomFactor;
      final limitedHalfHeight = newHalfHeight.clamp(0.1, 1000.0);

      // Update the orthographic height
      setOrthoHalfHeight(halfHeight: limitedHalfHeight);
      refreshFromKernel();
      renderingNeeded();
    } else {
      // Original perspective zooming code
      final cameraTransform = getCameraTransform(camera);

      final moveVec = (cameraTransform!.pivotPoint - cameraTransform.eye) *
          (ZOOM_PER_ZOOM_DELTA * (-scrollDeltaY));

      _moveCameraAndRender(
          eye: Vector3ToAPIVec3(cameraTransform.eye + moveVec),
          target: Vector3ToAPIVec3(cameraTransform.target + moveVec),
          up: Vector3ToAPIVec3(cameraTransform.up));
    }
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
                  onPointerPanZoomStart: (PointerPanZoomStartEvent event) {
                    onPointerPanZoomStart(event);
                  },
                  onPointerPanZoomUpdate: (PointerPanZoomUpdateEvent event) {
                    onPointerPanZoomUpdate(event);
                  },
                  onPointerPanZoomEnd: (PointerPanZoomEndEvent event) {
                    onPointerPanZoomEnd(event);
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
    // We can't remove persistent frame callbacks, so clean up resources
    // and release texture resources to prevent memory leaks
    const int textureKey = 0;
    _textureRenderer?.closeTexture(textureKey);
    _textureRenderer = null;
    _texturePtr = null;

    super.dispose();
  }
}
