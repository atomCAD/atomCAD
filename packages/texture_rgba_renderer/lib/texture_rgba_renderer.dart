import 'dart:typed_data';

export './texture_rgba_ffi.dart' show Native;

import 'texture_rgba_renderer_platform_interface.dart';

class TextureRgbaRenderer {
  /// Create a texture with unique [key].
  ///
  /// @return a texture id which can be used with
  /// ```dart
  /// Texture(textureId: textureId)
  /// ```
  Future<int> createTexture(int key) {
    return TextureRgbaRendererPlatform.instance.createTexture(key);
  }

  /// Close a texture with unique identifier [key].
  ///
  /// @return a boolean to indicate whether the operation is sucessfully executed.
  Future<bool> closeTexture(int key) {
    return TextureRgbaRendererPlatform.instance.closeTexture(key);
  }

  /// Provide the rgba data to the texture.
  Future<bool> onRgba(
      int key, Uint8List data, int height, int width, int strideAlign) {
    return TextureRgbaRendererPlatform.instance
        .onRgba(key, data, height, width, strideAlign);
  }

  Future<int> getTexturePtr(int key) {
    return TextureRgbaRendererPlatform.instance.getTexturePtr(key);
  }
}
