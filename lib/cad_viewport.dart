import 'package:flutter/material.dart';
import 'package:texture_rgba_renderer/texture_rgba_renderer.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter/scheduler.dart';

class CadViewport extends StatefulWidget {
  const CadViewport({super.key});

  @override
  _CadViewportState createState() => _CadViewportState();
}

class _CadViewportState extends State<CadViewport> {
  TextureRgbaRenderer? _textureRenderer;

  int? _textureId;
  int? _texturePtr;
  double? _elapsedInSec;
  int _frameId = 0;
  bool _continuousRendering = false;

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
            child: Texture(
              textureId: _textureId!,
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
