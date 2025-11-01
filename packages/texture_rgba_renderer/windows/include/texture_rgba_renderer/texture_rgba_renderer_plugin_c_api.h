#ifndef FLUTTER_PLUGIN_TEXTURE_RGBA_RENDERER_PLUGIN_C_API_H_
#define FLUTTER_PLUGIN_TEXTURE_RGBA_RENDERER_PLUGIN_C_API_H_

#include <flutter_plugin_registrar.h>

#ifdef FLUTTER_PLUGIN_IMPL
#define FLUTTER_PLUGIN_EXPORT __declspec(dllexport)
#else
#define FLUTTER_PLUGIN_EXPORT __declspec(dllimport)
#endif

#if defined(__cplusplus)
extern "C" {
#endif

FLUTTER_PLUGIN_EXPORT void TextureRgbaRendererPluginCApiRegisterWithRegistrar(
    FlutterDesktopPluginRegistrarRef registrar);

FLUTTER_PLUGIN_EXPORT void FlutterRgbaRendererPluginOnRgba(void* texture_rgba, const uint8_t* buffer, int len, int width, int height, int stride_align);

#if defined(__cplusplus)
}  // extern "C"
#endif

#endif  // FLUTTER_PLUGIN_TEXTURE_RGBA_RENDERER_PLUGIN_C_API_H_
