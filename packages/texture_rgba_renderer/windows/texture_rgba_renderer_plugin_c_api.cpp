#include <assert.h>
#include "include/texture_rgba_renderer/texture_rgba_renderer_plugin_c_api.h"

#include <flutter/plugin_registrar_windows.h>

#include "texture_rgba_renderer_plugin.h"
#include "texture_rgba.h"

void TextureRgbaRendererPluginCApiRegisterWithRegistrar(
    FlutterDesktopPluginRegistrarRef registrar)
{
    texture_rgba_renderer::TextureRgbaRendererPlugin::RegisterWithRegistrar(
        flutter::PluginRegistrarManager::GetInstance()
            ->GetRegistrar<flutter::PluginRegistrarWindows>(registrar));
}

void FlutterRgbaRendererPluginOnRgba(void *texture_rgba, const uint8_t *buffer, int len, int width, int height, int stride_align)
{
    assert(texture_rgba != nullptr);
    assert(buffer != nullptr);
    assert(width > 0 && height > 0);
    if (texture_rgba == nullptr || buffer == nullptr || width <= 0 || height <= 0)
    {
        return;
    }
    
    TextureRgba *rgba = static_cast<TextureRgba *>(texture_rgba);
    std::vector<uint8_t> data(buffer, buffer + len);
    rgba->MarkVideoFrameAvailable(data, width, height);
}
