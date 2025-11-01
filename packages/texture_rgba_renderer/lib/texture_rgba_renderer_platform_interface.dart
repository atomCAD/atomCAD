import 'dart:typed_data';

import 'package:flutter/foundation.dart';
import 'package:plugin_platform_interface/plugin_platform_interface.dart';

import 'texture_rgba_renderer_method_channel.dart';

abstract class TextureRgbaRendererPlatform extends PlatformInterface {
  /// Constructs a TextureRgbaRendererPlatform.
  TextureRgbaRendererPlatform() : super(token: _token);

  static final Object _token = Object();

  static TextureRgbaRendererPlatform _instance =
      MethodChannelTextureRgbaRenderer();

  /// The default instance of [TextureRgbaRendererPlatform] to use.
  ///
  /// Defaults to [MethodChannelTextureRgbaRenderer].
  static TextureRgbaRendererPlatform get instance => _instance;

  /// Platform-specific implementations should set this with their own
  /// platform-specific class that extends [TextureRgbaRendererPlatform] when
  /// they register themselves.
  static set instance(TextureRgbaRendererPlatform instance) {
    PlatformInterface.verifyToken(instance, _token);
    _instance = instance;
  }

  Future<String?> getPlatformVersion() {
    throw UnimplementedError('getPlatformVersion() has not been implemented.');
  }

  Future<int> createTexture(int key) {
    throw UnimplementedError('createTexture() has not been implemented.');
  }

  Future<bool> closeTexture(int key) {
    throw UnimplementedError('closeTexture() has not been implemented.');
  }

  Future<bool> onRgba(
      int key, Uint8List data, int height, int width, int strideAlign) {
    throw UnimplementedError('onRgba() has not been implemented.');
  }

  Future<int> getTexturePtr(int key) {
    throw UnimplementedError('onRgba() has not been implemented.');
  }
}
