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

class CadViewport extends StatefulWidget {
  const CadViewport({super.key});

  @override
  _CadViewportState createState() => _CadViewportState();
}

class _CadViewportState extends State<CadViewport> {


  static const double _clickThreshold = 4.0;
  static const double _addAtomPlaneDistance = 20.0;

  TextureRgbaRenderer? _textureRenderer;

  int? _textureId;
  int? _texturePtr;
  double? _elapsedInSec;
  int _frameId = 0;
  bool _continuousRendering = false;
  ViewportDragState _dragState = ViewportDragState.noDrag;
  Offset _dragStartPointerPos = Offset(0.0, 0.0);
  vector_math.Vector3 _pivotPoint = vector_math.Vector3(0.0, 0.0, 0.0);
  APICamera? _dragStartCamera;
  double _cameraMovePerPixel = 0.0; 

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
    _frameId++;
    SchedulerBinding.instance.addPostFrameCallback(_handlePostFrame);
  }

  void _handlePostFrame(Duration timeStamp) {
    //print("_handlePostFrame $_frameId");

    if(_continuousRendering) {
      SchedulerBinding.instance.scheduleFrame();
      if(_texturePtr != null) {
        var elapsedInSec = provideTexture(texturePtr: _texturePtr!);
        setState(() {
          _elapsedInSec = elapsedInSec;
        });
      }
    }
  }

  void _startMoveCamera(Offset pointerPos) {
    _dragState = ViewportDragState.move;
    _dragStartPointerPos = pointerPos;
    _dragStartCamera = getCamera();

    var movePlaneDistance = (_pivotPoint - APIVec3ToVector3(_dragStartCamera!.eye)).dot(
    (APIVec3ToVector3(_dragStartCamera!.target) - APIVec3ToVector3(_dragStartCamera!.eye)).normalized());
    _cameraMovePerPixel = 2.0 * movePlaneDistance * tan(_dragStartCamera!.fovy * 0.5) / 704.0; // TODO***: 704 should not be hardcoded
  }

  void _moveCamera(Offset pointerPos) {
    if (_dragStartCamera == null) {
      return;
    }
    var relPointerPos = pointerPos - _dragStartPointerPos;
    //_cameraMovePerPixel * relPointerPos

//pub fn move_camera(eye: APIVec3, target: APIVec3, up: APIVec3) {

    var eye = APIVec3ToVector3(_dragStartCamera!.eye);
    var target = APIVec3ToVector3(_dragStartCamera!.target);
    var forward = (target - eye).normalized();
    var up = APIVec3ToVector3(_dragStartCamera!.up);
    var right = forward.cross(up);

    var newEye = eye + right * ((-_cameraMovePerPixel) * relPointerPos.dx) + up * (_cameraMovePerPixel * relPointerPos.dy);
    var newTarget = newEye + forward;

    moveCamera(eye: Vector3ToAPIVec3(newEye), target: Vector3ToAPIVec3(newTarget), up: Vector3ToAPIVec3(up));

    //print('camera up: ${camera?.up.x} ${camera?.up.y} ${camera?.up.z}');

  }

  void _startRotateCamera(Offset pointerPos) {
    _dragState = ViewportDragState.rotate;
    _dragStartPointerPos = pointerPos;
    _dragStartCamera = getCamera();
  }

  void _rotateCamera(Offset pointerPos) {
    var relPointerPos = pointerPos - _dragStartPointerPos;
    //TODO***
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
    print('primary click at ${pointerPos}');

    //TODO: raycast into the model.
    // if not hit an atom, do the add atom stuff.
    // add the atom at a fix distance from the camera eye.
    // for now immediately invoke _addAtom
    _addAtom(pointerPos);
  }

  void _addAtom(Offset pointerPos) {
    //TODO***
    // First determine the position of the atom to be placed.
    var camera = getCamera();
    var eye = APIVec3ToVector3(camera!.eye);
    var target = APIVec3ToVector3(camera.target);
    var forward = (target - eye).normalized();
    var up = APIVec3ToVector3(camera.up);
    var right = forward.cross(up);

    var offsetPerPixel = 2.0 * _addAtomPlaneDistance * tan(camera.fovy * 0.5) / 704.0; // TODO***: 704 should not be hardcoded

    var centeredPointerPos = pointerPos - Offset(1280.0 * 0.5, 704.0 * 0.5); // TODO: should not be hardcoded

    var atomPos = eye + forward * _addAtomPlaneDistance + right * (offsetPerPixel * centeredPointerPos.dx) + up * (offsetPerPixel * (-centeredPointerPos.dy));

    addAtom(atomicNumber: 6, position: Vector3ToAPIVec3(atomPos));
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
            width: 1280,
            height: 704,
            child: Listener(
              onPointerDown: (PointerDownEvent event) {
                if (event.kind == PointerDeviceKind.mouse) {
                  switch (event.buttons) {
                    case kPrimaryMouseButton:
                      print('Left mouse button pressed at ${event.position}');
                      _startPrimaryDrag(event.position);                      
                      break;
                    case kSecondaryMouseButton:
                      print('Right mouse button pressed at ${event.position}');
                      _startRotateCamera(event.position);
                      break;
                    case kMiddleMouseButton:
                      print('Middle mouse button pressed at ${event.position}');
                      _startMoveCamera(event.position);
                      break;
                  }
                }
              },
              onPointerMove: (PointerMoveEvent event) {
                if (event.kind == PointerDeviceKind.mouse) {
                  print('Mouse moved to ${event.position}');
                  switch(_dragState) {
                    case  ViewportDragState.move:
                      _moveCamera(event.position);
                      break;
                    case ViewportDragState.rotate:
                      _rotateCamera(event.position);
                      break;
                    default:
                  }
                }
              },
              onPointerUp: (PointerUpEvent event) {
                if (event.kind == PointerDeviceKind.mouse) {
                  print('Mouse button released at ${event.position}');
                  _endDrag(event.position);
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
        Center(
          child: Row(
            mainAxisSize: MainAxisSize.min, // Ensure the Row's size wraps its children
            children: [
              ElevatedButton(
                onPressed: () {
                  _continuousRendering = true;
                  SchedulerBinding.instance.scheduleFrame();
                },
                child: Text("Start anim"),
              ),
              SizedBox(width: 8),
              ElevatedButton(
                onPressed: () {
                  _continuousRendering = false;
                },
                child: Text("Stop"),
              ),
              SizedBox(width: 8), // Small gap between the button and the label
              Text(_elapsedInSec == null ? 'no texture yet' : 'texture provided in ${(_elapsedInSec! * 1000.0).toStringAsFixed(2)} milliseconds',
                style: TextStyle(color: Colors.white)
              ), // Text label to the right of the button
            ],
          ),
        ),
      ],
    );
  }
}
