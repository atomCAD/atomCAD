import 'dart:io';
import 'dart:ffi';
import 'dart:math';
import 'dart:typed_data';
import 'package:ffi/ffi.dart';

import 'package:flutter/material.dart';
import 'dart:async';

import 'package:flutter/services.dart';
import 'package:texture_rgba_renderer/texture_rgba_renderer.dart';

void main() {
  runApp(const MyApp());
}

class MyApp extends StatefulWidget {
  const MyApp({super.key});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  final _textureRgbaRendererPlugin = TextureRgbaRenderer();
  int textureId = -1;
  int height = 768;
  int width = 1377;
  int cnt = 0;
  var key = 0;
  int texturePtr = 0;
  final random = Random();
  Uint8List? data;
  Timer? _timer;
  int time = 0;
  int method = 0;
  final strideAlign = Platform.isMacOS ? 64 : 1;

  @override
  void initState() {
    super.initState();
    _textureRgbaRendererPlugin.createTexture(key).then((textureId) {
      if (textureId != -1) {
        debugPrint("Texture register success, textureId=$textureId");
        _textureRgbaRendererPlugin.getTexturePtr(key).then((value) {
          debugPrint("texture ptr: ${value.toRadixString(16)}");
          setState(() {
            texturePtr = value;
          });
        });
        setState(() {
          this.textureId = textureId;
        });
      } else {
        return;
      }
    });
  }

  void start(int methodId) {
    debugPrint("start mockPic");
    method = methodId;
    final rowBytes = (width * 4 + strideAlign - 1) & (~(strideAlign - 1));
    final picDataLength = rowBytes * height;
    debugPrint('REMOVE ME =============================== rowBytes $rowBytes');
    _timer?.cancel();
    // 60 fps
    _timer =
        Timer.periodic(const Duration(milliseconds: 1000 ~/ 60), (timer) async {
      if (methodId == 0) {
        // Method.1: with MethodChannel
        data = mockPicture(width, height, rowBytes, picDataLength);
        final t1 = DateTime.now().microsecondsSinceEpoch;
        final res = await _textureRgbaRendererPlugin.onRgba(
            key, data!, height, width, strideAlign);
        final t2 = DateTime.now().microsecondsSinceEpoch;
        setState(() {
          time = t2 - t1;
        });
        if (!res) {
          debugPrint("WARN: render failed");
        }
      } else {
        final dataPtr = mockPicturePtr(width, height, rowBytes, picDataLength);
        // Method.2: with native ffi
        final t1 = DateTime.now().microsecondsSinceEpoch;
        Native.instance.onRgba(Pointer.fromAddress(texturePtr).cast<Void>(),
            dataPtr, picDataLength, width, height, strideAlign);
        final t2 = DateTime.now().microsecondsSinceEpoch;
        setState(() {
          time = t2 - t1;
        });
        malloc.free(dataPtr);
      }
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    if (key != -1) {
      _textureRgbaRendererPlugin.closeTexture(key);
    }
    super.dispose();
  }

  Uint8List mockPicture(int width, int height, int rowBytes, int length) {
    final pic = List.generate(length, (index) {
      final r = index / rowBytes;
      final c = (index % rowBytes) / 4;
      final p = index & 0x03;
      if (c > 20 && c < 30) {
        if (r > 20 && r < 25) {
          if (p == 0 || p == 3) {
            return 255;
          } else {
            return 0;
          }
        }
        if (r > 40 && r < 45) {
          if (p == 1 || p == 3) {
            return 255;
          } else {
            return 0;
          }
        }
        if (r > 60 && r < 65) {
          if (p == 2 || p == 3) {
            return 255;
          } else {
            return 0;
          }
        }
      }
      return 255;
    });
    return Uint8List.fromList(pic);
  }

  Pointer<Uint8> mockPicturePtr(
      int width, int height, int rowBytes, int length) {
    final pic = List.generate(length, (index) {
      final r = index / rowBytes;
      final c = (index % rowBytes) / 4;
      final p = index & 0x03;
      final edgeH = (c >= 0 && c < 10) || ((c >= width - 10) && c < width);
      final edgeW = (r >= 0 && r < 10) || ((r >= height - 10) && r < height);
      if (edgeH || edgeW) {
        if (p == 0 || p == 3) {
          return 255;
        } else {
          return 0;
        }
      }
      return 255;
    });
    final picAddr = malloc.allocate(pic.length).cast<Uint8>();
    final list = picAddr.asTypedList(pic.length);
    list.setRange(0, pic.length, pic);
    return picAddr;
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        body: Column(
          crossAxisAlignment: CrossAxisAlignment.center,
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Expanded(
              child: textureId == -1
                  ? const Offstage()
                  : Padding(
                      padding: const EdgeInsets.all(8.0),
                      child: Container(
                          alignment: Alignment.center,
                          decoration: const BoxDecoration(color: Colors.blue),
                          // decoration: const BoxDecoration(color: Colors.black),
                          // width: width.toDouble(),
                          // height: height.toDouble(),
                          child: Texture(textureId: textureId)),
                    ),
            ),
            Text(
                "texture id: $textureId, texture memory address: ${texturePtr.toRadixString(16)}"),
            TextButton.icon(
              label: const Text("play with texture (method channel API)"),
              icon: const Icon(Icons.play_arrow),
              onPressed: () => start(0),
            ),
            TextButton.icon(
              label: const Text("play with texture (native API, faster)"),
              icon: const Icon(Icons.play_arrow),
              onPressed: () => start(1),
            ),
            Text(
                "Current mode: ${method == 0 ? 'Method Channel API' : 'Native API'}"),
            time != 0 ? Text("FPS: ${1000000 ~/ time} fps") : const Offstage()
          ],
        ),
      ),
    );
  }
}
