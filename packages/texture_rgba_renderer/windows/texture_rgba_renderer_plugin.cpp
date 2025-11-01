#include "texture_rgba_renderer_plugin.h"

// This must be included before many other Windows headers.
#include <windows.h>

// For getPlatformVersion; remove unless needed for your plugin implementation.
#include <VersionHelpers.h>

#include <flutter/method_channel.h>
#include <flutter/plugin_registrar_windows.h>
#include <flutter/standard_method_codec.h>

#include <memory>
#include <sstream>

namespace texture_rgba_renderer {

// static
void TextureRgbaRendererPlugin::RegisterWithRegistrar(
    flutter::PluginRegistrarWindows *registrar) {
  auto channel =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          registrar->messenger(), "texture_rgba_renderer",
          &flutter::StandardMethodCodec::GetInstance());

  auto plugin = std::make_unique<TextureRgbaRendererPlugin>();
  plugin->texture_registrar = registrar->texture_registrar();

  channel->SetMethodCallHandler(
      [plugin_pointer = plugin.get()](const auto &call, auto result) {
        plugin_pointer->HandleMethodCall(call, std::move(result));
      });

  registrar->AddPlugin(std::move(plugin));
}

TextureRgbaRendererPlugin::TextureRgbaRendererPlugin() {
    this->texture_registrar = nullptr;
}

TextureRgbaRendererPlugin::~TextureRgbaRendererPlugin() {}

void TextureRgbaRendererPlugin::HandleMethodCall(
    const flutter::MethodCall<flutter::EncodableValue> &method_call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (method_call.method_name().compare("createTexture") == 0) {
     auto args = std::get<flutter::EncodableMap>(*method_call.arguments());
     auto key = std::get<int>(args.at(flutter::EncodableValue("key")));
     auto [it, add] = textures_.try_emplace(key, nullptr);
     if (add) {
         // Create a texture.
         it->second = std::make_unique<TextureRgba>(this->texture_registrar);
         result->Success(flutter::EncodableValue(it->second->texture_id()));
     }
  }
  else if (method_call.method_name().compare("closeTexture") == 0) {
      auto args = std::get<flutter::EncodableMap>(*method_call.arguments());
      auto key = std::get<int>(args.at(flutter::EncodableValue("key")));
      if (textures_.find(key) == textures_.end()) {
          result->Success(flutter::EncodableValue(false));
      }
      else {
          this->texture_registrar->UnregisterTexture(textures_[key]->texture_id());
          textures_.erase(key);
          result->Success(flutter::EncodableValue(true));
      }
  }
  else if (method_call.method_name().compare("onRgba") == 0) {
      auto args = std::get<flutter::EncodableMap>(*method_call.arguments());
      auto key = std::get<int>(args.at(flutter::EncodableValue("key")));
      if (textures_.find(key) == textures_.end()) {
          result->Success(flutter::EncodableValue(false));
      }
      else {
          auto& data = std::get<std::vector<uint8_t>>(args.at(flutter::EncodableValue("data")));
          auto height = std::get<int>(args.at(flutter::EncodableValue("height")));
          auto width = std::get<int>(args.at(flutter::EncodableValue("width")));
          textures_[key]->MarkVideoFrameAvailable(data, width, height);
          result->Success(flutter::EncodableValue(true));
      }
  } 
  else if (method_call.method_name().compare("getTexturePtr") == 0) {
      auto args = std::get<flutter::EncodableMap>(*method_call.arguments());
      auto key = std::get<int>(args.at(flutter::EncodableValue("key")));
      if (textures_.find(key) == textures_.end()) {
          result->Success(flutter::EncodableValue(-1));
      }
      else {
          // Return an address.
          size_t rgba = reinterpret_cast<size_t>((void*) textures_[key].get());
          result->Success(flutter::EncodableValue(int64_t(rgba)));
      }
  }
  else {
    result->NotImplemented();
  }
}

}  // namespace texture_rgba_renderer

