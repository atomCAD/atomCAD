#include "include/texture_rgba_renderer/texture_rgba_renderer_plugin.h"
#include "include/texture_rgba_renderer/texture_rgba_renderer.h"

#include <flutter_linux/flutter_linux.h>
#include <gtk/gtk.h>
#include <sys/utsname.h>

#define TEXTURE_RGBA_RENDERER_PLUGIN(obj)                                     \
  (G_TYPE_CHECK_INSTANCE_CAST((obj), texture_rgba_renderer_plugin_get_type(), \
                              TextureRgbaRendererPlugin))

#include <cstring>
#include <unordered_map>

std::unordered_map<int64_t, TextureRgba *> g_renderer_map;

void FlutterRgbaRendererPluginOnRgba(void *texture_rgba_ptr, const uint8_t *buffer, int len, int width, int height);
struct _TextureRgbaRendererPlugin
{
  GObject parent_instance;
  FlTextureRegistrar *texture_registrar;
};

G_DEFINE_TYPE(TextureRgbaRendererPlugin, texture_rgba_renderer_plugin, g_object_get_type())

// Called when a method call is received from Flutter.
static void texture_rgba_renderer_plugin_handle_method_call(
    TextureRgbaRendererPlugin *self,
    FlMethodCall *method_call)
{
  g_autoptr(FlMethodResponse) response = nullptr;

  const gchar *method = fl_method_call_get_name(method_call);

  if (strcmp(method, "createTexture") == 0)
  {
    auto args = fl_method_call_get_args(method_call);
    int64_t key = fl_value_get_int(fl_value_lookup_string(args, "key"));
    if (g_renderer_map.find(key) != g_renderer_map.end())
    {
      response = FL_METHOD_RESPONSE(fl_method_success_response_new(fl_value_new_int(-1)));
    }
    else
    {
      TextureRgba* texture_rgba = texture_rgba_new(self->texture_registrar);
      auto texture_id = reinterpret_cast<int64_t>(FL_TEXTURE(texture_rgba));
      texture_rgba->texture_id = texture_id;
      g_renderer_map.insert(std::make_pair(key, texture_rgba));
      // Register to the registrar.
      fl_texture_registrar_register_texture(self->texture_registrar, FL_TEXTURE(texture_rgba));
      response = FL_METHOD_RESPONSE(fl_method_success_response_new(fl_value_new_int(texture_id)));
    }
  }
  else if (strcmp(method, "closeTexture") == 0)
  {
    auto args = fl_method_call_get_args(method_call);
    int64_t key = fl_value_get_int(fl_value_lookup_string(args, "key"));
    if (g_renderer_map.find(key) != g_renderer_map.end())
    {
      fl_texture_registrar_unregister_texture(self->texture_registrar, FL_TEXTURE(g_renderer_map[key]));
      texture_rgba_terminate(g_renderer_map[key]);
      g_renderer_map.erase(key);
    }
    response = FL_METHOD_RESPONSE(fl_method_success_response_new(fl_value_new_bool(true)));
  }
  else if (strcmp(method, "onRgba") == 0)
  {
    auto args = fl_method_call_get_args(method_call);
    auto key = fl_value_get_int(fl_value_lookup_string(args, "key"));
    auto data = fl_value_get_uint8_list(fl_value_lookup_string(args, "data"));
    auto data_length = fl_value_get_length(fl_value_lookup_string(args, "data"));
    auto width = fl_value_get_int(fl_value_lookup_string(args, "width"));
    auto height = fl_value_get_int(fl_value_lookup_string(args, "height"));
    auto stride_align = fl_value_get_int(fl_value_lookup_string(args, "stride_align"));
    auto texture_rgba = g_renderer_map[key];
    FlutterRgbaRendererPluginOnRgba((void*)texture_rgba, data, data_length, width, height, stride_align);
    response = FL_METHOD_RESPONSE(fl_method_success_response_new(
        fl_value_new_bool(fl_texture_registrar_mark_texture_frame_available(self->texture_registrar, FL_TEXTURE(texture_rgba)))));
  }
  else if (strcmp(method, "getTexturePtr") == 0)
  {
    auto args = fl_method_call_get_args(method_call);
    auto key = fl_value_get_int(fl_value_lookup_string(args, "key"));
    if (g_renderer_map.find(key) == g_renderer_map.end())
    {
      response = FL_METHOD_RESPONSE(fl_method_success_response_new(
          fl_value_new_int(0)));
    }
    else
    {
      // Return an address.
      size_t rgba = reinterpret_cast<size_t>((void *)g_renderer_map[key]);
      response = FL_METHOD_RESPONSE(fl_method_success_response_new(
          fl_value_new_int(rgba)));
    }
  }
  else
  {
    response = FL_METHOD_RESPONSE(fl_method_not_implemented_response_new());
  }

  fl_method_call_respond(method_call, response, nullptr);
}

static void texture_rgba_renderer_plugin_dispose(GObject *object)
{
  G_OBJECT_CLASS(texture_rgba_renderer_plugin_parent_class)->dispose(object);
}

static void texture_rgba_renderer_plugin_class_init(TextureRgbaRendererPluginClass *klass)
{
  G_OBJECT_CLASS(klass)->dispose = texture_rgba_renderer_plugin_dispose;
}

static void texture_rgba_renderer_plugin_init(TextureRgbaRendererPlugin *self) {}

static void method_call_cb(FlMethodChannel *channel, FlMethodCall *method_call,
                           gpointer user_data)
{
  TextureRgbaRendererPlugin *plugin = TEXTURE_RGBA_RENDERER_PLUGIN(user_data);
  texture_rgba_renderer_plugin_handle_method_call(plugin, method_call);
}

void texture_rgba_renderer_plugin_register_with_registrar(FlPluginRegistrar *registrar)
{
  TextureRgbaRendererPlugin *plugin = TEXTURE_RGBA_RENDERER_PLUGIN(
      g_object_new(texture_rgba_renderer_plugin_get_type(), nullptr));

  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  g_autoptr(FlMethodChannel) channel =
      fl_method_channel_new(fl_plugin_registrar_get_messenger(registrar),
                            "texture_rgba_renderer",
                            FL_METHOD_CODEC(codec));
  plugin->texture_registrar = fl_plugin_registrar_get_texture_registrar(registrar);
  fl_method_channel_set_method_call_handler(channel, method_call_cb,
                                            g_object_ref(plugin),
                                            g_object_unref);

  g_object_unref(plugin);
}


extern "C" {
   void FlutterRgbaRendererPluginOnRgba(void *texture_rgba_ptr, const uint8_t *buffer, int len, int width, int height, int stride_align) {
      // Check whether the parameter is valid.
      if (width == 0 || height == 0) {
        g_warning("The width, height is not valid, ignoring this request.");
        return;
      }
      TextureRgba* self = TEXTURE_RGBA(texture_rgba_ptr);
      g_mutex_lock(&self->mutex);
      // selfate has registered a texture_id,
      if (self->texture_id != 0 && !g_atomic_int_get(&self->buffer_ready) && !texture_rgba_is_terminate(self)) {
        // copy data to the texture.
        auto copied_data = new uint8_t[len];
        memcpy(copied_data, buffer, len); 
        switch_rgba(copied_data, len, height);
        // It's safe to working on a non reading index
        g_atomic_pointer_set(&self->buffer, copied_data);
        g_atomic_int_set(&self->video_height, height);
        g_atomic_int_set(&self->video_width, width);
        g_atomic_int_set(&self->buffer_ready, TRUE);
        fl_texture_registrar_mark_texture_frame_available(
          self->texture_registrar,
          FL_TEXTURE(self)
        );
      }
      g_mutex_unlock(&self->mutex);
   }
};
