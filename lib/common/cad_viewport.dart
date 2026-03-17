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
  final eye = apiVec3ToVector3(camera.eye);
  final target = apiVec3ToVector3(camera.target);
  final forward = (target - eye).normalized();
  final up = apiVec3ToVector3(camera.up);
  final right = forward.cross(up);

  return CameraTransform(
    eye: eye,
    target: target,
    forward: forward,
    up: up,
    right: right,
    pivotPoint: apiVec3ToVector3(camera.pivotPoint),
  );
}

// Axis is a normal vector
vector_math.Vector3 rotatePointAroundAxis(vector_math.Vector3 axisPos,
    vector_math.Vector3 axis, double angle, vector_math.Vector3 point) {
  final rotation = vector_math.Quaternion.axisAngle(axis, angle);
  return rotation.rotated(point - axisPos) + axisPos;
}

/// Interface for tools that need to take over primary mouse button interactions.
/// When the delegate returns true, the base class skips its own click-threshold
/// / gadget logic. When it returns false, the base class runs normally.
abstract class PrimaryPointerDelegate {
  /// Called on primary button down. Return true to consume (base won't do
  /// click-threshold / gadget hit test). Return false to let base handle it.
  bool onPrimaryDown(Offset pos);

  /// Called on primary button move while consumed. Return true to consume.
  /// Return false to let base handle it (e.g., for gadget dragging).
  bool onPrimaryMove(Offset pos);

  /// Called on primary button up while consumed. Return true to consume.
  /// Return false to let base handle it.
  bool onPrimaryUp(Offset pos);

  /// Called when the pointer interaction is cancelled (e.g., system steal,
  /// window focus loss). Resets any in-progress interaction.
  void onPrimaryCancel();
}

abstract class CadViewport extends StatefulWidget {
  const CadViewport({super.key});
}

abstract class CadViewportState<T extends CadViewport> extends State<T> {
  static const double _clickThreshold = 4.0;

  // These initial values get overwritten
  double viewportWidth = 1280.0;
  double viewportHeight = 544.0;

  // Rotation per pixel in radian
  static const double _rotPerPixel = 0.02;

  // amount of relative zoom to the zoom target per reported zoom delta.
  static const double _zoomPerZoomDelta = 0.002;

  TextureRgbaRenderer? _textureRenderer;

  int? textureId;
  int? _texturePtr;
  ViewportDragState dragState = ViewportDragState.noDrag;
  Offset _dragStartPointerPos = Offset(0.0, 0.0);
  CameraTransform? _dragStartCameraTransform;
  double _cameraMovePerPixel = 0.0;

  bool isGadgetDragging = false;
  bool _delegateConsumedDown = false;

  // Coalesced camera drag position — multiple mouse events between frames
  // are merged so only the last position is processed before each render.
  Offset? _pendingCameraDragPos;

  int draggedGadgetHandle =
      -1; // Relevant when _dragState == ViewportDragState.gadgetDrag

  /// Override in subclass to provide a delegate for the active tool.
  @protected
  PrimaryPointerDelegate? get primaryPointerDelegate => null;

  /// Start a gadget drag from a known handle index (no Flutter-side hit test).
  /// Used when Rust already determined the gadget was hit.
  @protected
  void startGadgetDragFromHandle(int handleIndex, Offset pointerPos) {
    dragState = ViewportDragState.defaultDrag;
    _dragStartPointerPos = pointerPos;
    final ray = getRayFromPointerPos(pointerPos);
    isGadgetDragging = true;
    draggedGadgetHandle = transformDraggedGadgetHandle(handleIndex);
    gadgetStartDrag(
      handleIndex: draggedGadgetHandle,
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    renderingNeeded();
  }

  void onPointerSignal(PointerSignalEvent pointerSignal) {
    if (pointerSignal is PointerScrollEvent) {
      scroll(pointerSignal.localPosition, pointerSignal.scrollDelta.dy);
      //print('Scrolled: ${scrollEvent.scrollDelta}');
    }
  }

  // Handle trackpad/Magic Mouse pan-zoom start
  void onPointerPanZoomStart(PointerPanZoomStartEvent event) {
    // Initialize pan-zoom gesture if needed
  }

  // Handle trackpad/Magic Mouse pan-zoom updates (includes zoom)
  void onPointerPanZoomUpdate(PointerPanZoomUpdateEvent event) {
    // Check if Shift is pressed for panning mode
    bool isShiftPressed = HardwareKeyboard.instance.isShiftPressed;

    // Handle zoom from multiple sources
    bool zoomHandled = false;

    // 1. Trackpad pinch-to-zoom (scale changes) - only when Shift is NOT pressed
    if (!isShiftPressed && event.scale != 1.0) {
      final scaleDelta = event.scale - 1.0;
      final scrollDelta =
          -scaleDelta * 100.0; // Further reduced sensitivity (was 250.0)
      scroll(event.localPosition, scrollDelta);
      zoomHandled = true;
    }

    // 2. Shift + PanZoom for camera panning
    if (isShiftPressed &&
        (event.panDelta.dx.abs() > 0.1 || event.panDelta.dy.abs() > 0.1)) {
      // Use pan delta for camera panning when Shift is pressed
      // Convert pan delta to camera movement
      panCameraWithDelta(event.localPosition, event.panDelta);
      zoomHandled = true; // Prevent zoom when panning
    }

    // 3. Magic Mouse / Trackpad vertical pan for zoom (when no scale change and Shift not pressed)
    if (!zoomHandled && !isShiftPressed && event.panDelta.dy.abs() > 0.1) {
      // Use vertical pan delta for zooming - increased sensitivity
      final scrollDelta =
          event.panDelta.dy * 4.0; // Increased sensitivity (was 2.0)
      scroll(event.localPosition, scrollDelta);
    }
  }

  // Handle trackpad/Magic Mouse pan-zoom end
  void onPointerPanZoomEnd(PointerPanZoomEndEvent event) {
    // Clean up pan-zoom gesture if needed
  }

  // Convert pan delta to camera movement for Shift + PanZoom gestures
  void panCameraWithDelta(Offset pointerPos, Offset panDelta) {
    final camera = getCamera();
    final cameraTransform = getCameraTransform(camera);

    if (cameraTransform == null) return;

    // Calculate movement scale similar to existing camera move logic
    double movePerPixel;
    if (camera!.orthographic) {
      movePerPixel = 2.0 * camera.orthoHalfHeight / viewportHeight;
    } else {
      var movePlaneDistance = (cameraTransform.pivotPoint - cameraTransform.eye)
          .dot(cameraTransform.forward);
      movePerPixel =
          2.0 * movePlaneDistance * tan(camera.fovy * 0.5) / viewportHeight;
    }

    // Convert pan delta to camera movement (invert Y for natural feel)
    var moveDelta = cameraTransform.right * ((-movePerPixel) * panDelta.dx) +
        cameraTransform.up * (movePerPixel * panDelta.dy);

    var newEye = cameraTransform.eye + moveDelta;
    var newTarget = cameraTransform.target + moveDelta;

    _moveCameraAndRender(
        eye: vector3ToApiVec3(newEye),
        target: vector3ToApiVec3(newTarget),
        up: vector3ToApiVec3(cameraTransform.up));
  }

  void onPointerDown(PointerDownEvent event) {
    if (event.kind == PointerDeviceKind.mouse) {
      switch (event.buttons) {
        case kPrimaryMouseButton:
          final delegate = primaryPointerDelegate;
          if (delegate != null && delegate.onPrimaryDown(event.localPosition)) {
            _delegateConsumedDown = true;
            return;
          }
          _delegateConsumedDown = false;
          startPrimaryDrag(event.localPosition);
          break;
        case kSecondaryMouseButton:
          //print('Right mouse button pressed at ${event.position}');
          // Check for Shift + Right mouse for panning
          if (HardwareKeyboard.instance.isShiftPressed) {
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
      if (_delegateConsumedDown) {
        final delegate = primaryPointerDelegate;
        if (delegate != null && delegate.onPrimaryMove(event.localPosition)) {
          return;
        }
        // Delegate declined move (e.g., gadget dragging) — fall through to base
      }
      switch (dragState) {
        case ViewportDragState.move:
        case ViewportDragState.rotate:
          // Coalesce camera drag events — just store latest position,
          // actual update happens once in _handlePersistentFrame before render.
          _pendingCameraDragPos = event.localPosition;
          renderingNeeded();
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
      if (_delegateConsumedDown) {
        final delegate = primaryPointerDelegate;
        if (delegate != null && delegate.onPrimaryUp(event.localPosition)) {
          _delegateConsumedDown = false;
          return;
        }
        // Delegate declined up (e.g., gadget dragging) — fall through to base
        _delegateConsumedDown = false;
      }
      endDrag(event.localPosition);
    }
  }

  void onPointerCancel(PointerCancelEvent event) {
    if (_delegateConsumedDown) {
      final delegate = primaryPointerDelegate;
      delegate?.onPrimaryCancel();
      _delegateConsumedDown = false;
    }
    // Reset any base-class drag state
    if (dragState != ViewportDragState.noDrag) {
      dragState = ViewportDragState.noDrag;
      if (isGadgetDragging) {
        gadgetEndDrag();
        isGadgetDragging = false;
      }
    }
  }

  void renderingNeeded() {
    SchedulerBinding.instance.scheduleFrame();
  }

  void _moveCameraAndRender(
      {required APIVec3 eye, required APIVec3 target, required APIVec3 up}) {
    moveCamera(eye: eye, target: target, up: up);
    // Skip refreshFromKernel() during camera drags — only camera data changed,
    // node networks/tools/preferences are untouched. Refresh on drag end instead.
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
      // Process coalesced camera drag before rendering — many mouse events
      // may have queued up while the previous render was blocking; only the
      // last position matters.
      _flushPendingCameraDrag();
      provideTexture(texturePtr: _texturePtr!);
    }
  }

  void _flushPendingCameraDrag([ViewportDragState? overrideState]) {
    final pos = _pendingCameraDragPos;
    if (pos == null) return;
    _pendingCameraDragPos = null;

    switch (overrideState ?? dragState) {
      case ViewportDragState.move:
        cameraMove(pos);
        break;
      case ViewportDragState.rotate:
        rotateCamera(pos);
        break;
      default:
        break;
    }
  }

  void startMoveCamera(Offset pointerPos) {
    // Adjust camera target based on what's under the cursor
    final ray = getRayFromPointerPos(pointerPos);
    adjustCameraTarget(
        rayOrigin: vector3ToApiVec3(ray.start),
        rayDirection: vector3ToApiVec3(ray.direction));

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
        eye: vector3ToApiVec3(newEye),
        target: vector3ToApiVec3(newTarget),
        up: vector3ToApiVec3(_dragStartCameraTransform!.up));
  }

  void startRotateCamera(Offset pointerPos) {
    // Adjust camera target based on what's under the cursor
    final ray = getRayFromPointerPos(pointerPos);
    adjustCameraTarget(
        rayOrigin: vector3ToApiVec3(ray.start),
        rayDirection: vector3ToApiVec3(ray.direction));

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
    final horizAngle = relPointerPos.dx * _rotPerPixel;
    final vertAxis = vector_math.Vector3(0.0, 0.0, 1.0);
    var newEye = rotatePointAroundAxis(
        cameraTransform!.pivotPoint, vertAxis, horizAngle, cameraTransform.eye);

    final newForward = rotatePointAroundAxis(vector_math.Vector3.zero(),
        vertAxis, horizAngle, cameraTransform.forward);
    final newRight = newForward.cross(cameraTransform.up).normalized();

    // Vertical component - rotate around right vector
    final vertAngle = relPointerPos.dy * _rotPerPixel;
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
        eye: vector3ToApiVec3(newEye),
        target: vector3ToApiVec3(newEye + newForward2),
        up: vector3ToApiVec3(newUp));

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
        rayOrigin: vector3ToApiVec3(ray.start),
        rayDirection: vector3ToApiVec3(ray.direction));
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
        rayOrigin: vector3ToApiVec3(ray.start),
        rayDirection: vector3ToApiVec3(ray.direction));

    if (hitResult != null) {
      isGadgetDragging = true;
      draggedGadgetHandle = transformDraggedGadgetHandle(hitResult);
      gadgetStartDrag(
          handleIndex: draggedGadgetHandle,
          rayOrigin: vector3ToApiVec3(ray.start),
          rayDirection: vector3ToApiVec3(ray.direction));
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

    // Flush any coalesced camera move so the final position is applied,
    // then refresh the full UI state (was skipped during drag for performance).
    if (oldDragState == ViewportDragState.move ||
        oldDragState == ViewportDragState.rotate) {
      _flushPendingCameraDrag(oldDragState);
      refreshFromKernel();
    }

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

    addAtom(atomicNumber: 6, position: vector3ToApiVec3(atomPos));
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
        rayOrigin: vector3ToApiVec3(ray.start),
        rayDirection: vector3ToApiVec3(ray.direction));

    final camera = getCamera();

    if (camera!.orthographic) {
      // For orthographic projection, we adjust orthoHalfHeight instead of moving the camera
      // Positive scrollDeltaY = zoom out = increase orthoHalfHeight
      // Negative scrollDeltaY = zoom in = decrease orthoHalfHeight

      // Get the current value
      final currentHalfHeight = camera.orthoHalfHeight;

      // Calculate the zoom factor (scrolling down increases size, up decreases)
      final zoomFactor = 1.0 + _zoomPerZoomDelta * scrollDeltaY;

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
          (_zoomPerZoomDelta * (-scrollDeltaY));

      _moveCameraAndRender(
          eye: vector3ToApiVec3(cameraTransform.eye + moveVec),
          target: vector3ToApiVec3(cameraTransform.target + moveVec),
          up: vector3ToApiVec3(cameraTransform.up));
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
                  onPointerCancel: (PointerCancelEvent event) {
                    onPointerCancel(event);
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
