import 'package:flutter/material.dart';
import 'package:texture_rgba_renderer/texture_rgba_renderer.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';

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

  void initTexture() async {
    _textureRenderer = TextureRgbaRenderer();

    // Initialize the texture
    const int textureKey = 0;
    await _textureRenderer?.closeTexture(textureKey);
    var textureId = await _textureRenderer?.createTexture(textureKey);
    var texturePtr = await _textureRenderer?.getTexturePtr(textureKey);
    setState(() {
      _textureId = textureId;
      _texturePtr = texturePtr;
    });
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
                  if( _texturePtr != null) {
                    var elapsedInSec = provideTexture(texturePtr: _texturePtr!);
                    setState(() {
                      _elapsedInSec = elapsedInSec;
                    });
                  }
                },
                child: Text("Provide texture!"),
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
