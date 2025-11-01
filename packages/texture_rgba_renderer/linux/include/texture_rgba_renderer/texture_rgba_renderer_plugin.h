#ifndef FLUTTER_PLUGIN_TEXTURE_RGBA_RENDERER_PLUGIN_H_
#define FLUTTER_PLUGIN_TEXTURE_RGBA_RENDERER_PLUGIN_H_

#include <flutter_linux/flutter_linux.h>

G_BEGIN_DECLS

#ifdef FLUTTER_PLUGIN_IMPL
#define FLUTTER_PLUGIN_EXPORT __attribute__((visibility("default")))
#else
#define FLUTTER_PLUGIN_EXPORT
#endif

typedef struct _TextureRgbaRendererPlugin TextureRgbaRendererPlugin;
typedef struct {
  GObjectClass parent_class;
} TextureRgbaRendererPluginClass;

FLUTTER_PLUGIN_EXPORT GType texture_rgba_renderer_plugin_get_type();

FLUTTER_PLUGIN_EXPORT void texture_rgba_renderer_plugin_register_with_registrar(
    FlPluginRegistrar* registrar);

// Keep the same symbol name with Windows.
FLUTTER_PLUGIN_EXPORT void FlutterRgbaRendererPluginOnRgba(void *texture_rgba, const uint8_t *buffer, int len, int width, int height, int stride_align);

G_END_DECLS

#endif  // FLUTTER_PLUGIN_TEXTURE_RGBA_RENDERER_PLUGIN_H_
