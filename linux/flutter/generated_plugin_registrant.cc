//
//  Generated file. Do not edit.
//

// clang-format off

#include "generated_plugin_registrant.h"

#include <texture_rgba_renderer/texture_rgba_renderer_plugin.h>

void fl_register_plugins(FlPluginRegistry* registry) {
  g_autoptr(FlPluginRegistrar) texture_rgba_renderer_registrar =
      fl_plugin_registry_get_registrar_for_plugin(registry, "TextureRgbaRendererPlugin");
  texture_rgba_renderer_plugin_register_with_registrar(texture_rgba_renderer_registrar);
}
