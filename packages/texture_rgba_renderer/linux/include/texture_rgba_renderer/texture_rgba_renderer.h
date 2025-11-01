#ifndef TEXTURE_RGBA_RENDERER_H
#define TEXTURE_RGBA_RENDERER_H

#include <flutter_linux/flutter_linux.h>
#include <gtk/gtk.h>

#include "texture_rgba_renderer_plugin.h"

G_BEGIN_DECLS

typedef struct {
  FlPixelBufferTextureClass parent_class;
} TextureRgbaClass;

struct _TextureRgba
{
  FlPixelBufferTexture parent_instance;

  uint8_t *buffer = nullptr;
  FlTextureRegistrar* texture_registrar = nullptr;
  int64_t texture_id = 0;
  uint8_t *prior_buffer = nullptr;
  gboolean buffer_ready = FALSE;
  gboolean terminated = FALSE;
  gint32 video_width = 0;
  gint32 video_height = 0;
  GMutex mutex;
};

typedef struct _TextureRgba TextureRgba;

/// Constructor.
// G_DECLARE_DERIVABLE_TYPE(TextureRgba, texture_rgba, TEXTURE_RGBA_RENDERER, TEXTURE_RGBA, FlPixelBufferTextureClass)
/// Add private data.
G_DEFINE_TYPE(TextureRgba, texture_rgba,
                        fl_pixel_buffer_texture_get_type())

#define TEXTURE_RGBA(obj)                                     \
  (G_TYPE_CHECK_INSTANCE_CAST((obj), texture_rgba_get_type(), \
                              TextureRgba))

static inline void switch_rgba(uint8_t* pixels, int len, int height) {
    uint8_t temp;
    int byes_per_row = len / height;
    int width = byes_per_row >> 2;
    for (int i = 0; i < height; ++i) {
      int row_idx_base = i * byes_per_row;
      for (int k = 0; k < width; ++k) {
        int idx = row_idx_base + k * 4;
        temp = pixels[idx];
        pixels[idx] = pixels[idx + 2];
        pixels[idx + 2] = temp;
      }
    }
}


static void texture_rgba_terminate(TextureRgba* self) {
  g_mutex_lock(&self->mutex);
  g_atomic_int_set(&self->terminated, TRUE);
  g_mutex_unlock(&self->mutex);
}

static gboolean texture_rgba_is_terminate(TextureRgba* self) {
  return g_atomic_int_get(&self->terminated);
}

static gboolean texture_rgba_copy_pixels(FlPixelBufferTexture* texture,
                          const uint8_t** out_buffer,
                          uint32_t* width,
                          uint32_t* height,
                          GError** error) {
  TextureRgba *self = TEXTURE_RGBA(texture);
  // This method is called on Render Thread. Be careful with your
  // cross-thread operation.
  g_mutex_lock(&self->mutex);
  // @width and @height are initially stored the canvas size in Flutter.
  // You must prepare your pixel buffer in RGBA format.
  // So you may do some format conversion first if your original pixel
  // buffer is not in RGBA format.
  uint8_t* buffer = (uint8_t*)g_atomic_pointer_get(&self->buffer);
  // manage_your_pixel_buffer_here ();
  if (g_atomic_int_get(&self->buffer_ready)) {
    // Directly return pointer to your pixel buffer here.
    // Flutter takes content of your pixel buffer after this function
    // is finished. So you must make the buffer live long enough until
    // next tick of Render Thread.
    // If it is hard to manage lifetime of your pixel buffer, you should
    // take look into #FlTextureGL.
    if (self->prior_buffer) {
      delete[] self->prior_buffer;
      self->prior_buffer = nullptr;
    }
    *out_buffer = (const uint8_t *)buffer;
    *width = g_atomic_int_get(&self->video_width);
    *height = g_atomic_int_get(&self->video_height);
    self->prior_buffer = (uint8_t *)buffer;
    g_atomic_int_set(&self->buffer_ready, FALSE);
    g_mutex_unlock(&self->mutex);
    return TRUE;
  } else if (g_atomic_int_get(&self->terminated)) {
    g_mutex_unlock(&self->mutex);
    // set @error to report failure.
    *error = g_error_new(g_quark_from_static_string("TextureRgba Renderer"), -1, "the texture is already terminated, ignoring.");
    return FALSE;
  } else {
    g_mutex_unlock(&self->mutex);
    // *error = g_error_new(g_quark_from_static_string("TextureRgba Renderer"), 1, "waiting for new incoming images.");
    return TRUE;
  }
}

// static gboolean texture_rgba_populate(FlTextureGL *texture,
//                                       uint32_t *target,
//                                       uint32_t *name,
//                                       uint32_t *width,
//                                       uint32_t *height,
//                                       GError **error)
// {
//   TextureRgbaPrivate *self = (TextureRgbaPrivate *)
//       texture_rgba_get_instance_private(TEXTURE_RGBA_RENDERER_TEXTURE_RGBA(texture));
  
//   g_mutex_lock(&self->mutex);
//   if (self->texture_id == 0)
//   {
//     glGenTextures(1, &self->texture_id);
//     glBindTexture(GL_TEXTURE_2D, self->texture_id);
//     // further configuration here.
//     g_atomic_pointer_set(&self->buffer, nullptr);
//   }
//   else
//   {
//     glBindTexture(GL_TEXTURE_2D, self->texture_id);
//   }
//   // For example, we render pixel buffer here.
  // // Note that Flutter only accepts textures in GL_RGBA8 format.

//   g_atomic_int_set(&self->buffer_ready, FALSE);
//   *target = GL_TEXTURE_2D;
//   *name = self->texture_id;
//   *width = g_atomic_int_get(&self->video_width);
//   *height = g_atomic_int_get(&self->video_height);
//   g_mutex_unlock(&self->mutex);
//   return TRUE;
// }

static TextureRgba *texture_rgba_new(FlTextureRegistrar* registrar)
{
  auto texture = TEXTURE_RGBA(g_object_new(texture_rgba_get_type(), nullptr));
  texture->texture_registrar = registrar;
  g_mutex_init(&texture->mutex);
  return texture;
}

static void texture_rgba_class_init(TextureRgbaClass *klass)
{
  FL_PIXEL_BUFFER_TEXTURE_CLASS(klass)->copy_pixels = texture_rgba_copy_pixels;
}

static void texture_rgba_init(TextureRgba* self) {
  FL_PIXEL_BUFFER_TEXTURE_GET_CLASS(self)->copy_pixels =
          texture_rgba_copy_pixels;
}

G_END_DECLS
#endif
