#ifndef TEXTURE_RGBA_H_
#define TEXTURE_RGBA_H_

#include <flutter/texture_registrar.h>
#include <mutex>
#include <chrono>

class TextureRgba
{
public:
	TextureRgba(flutter::TextureRegistrar* texture_registrar);
	~TextureRgba();

	void MarkVideoFrameAvailable(std::vector<uint8_t>& buffer, size_t width, size_t height);

	int64_t texture_id() const { return texture_id_; };

	const FlutterDesktopPixelBuffer* buffer();

private:
	void switch_rgba(std::vector<uint8_t>& buffer, size_t height);

private:
	FlutterDesktopPixelBuffer flutter_pixel_buffer_{};
	flutter::TextureRegistrar* texture_registrar_ = nullptr;
	std::unique_ptr<flutter::TextureVariant> texture_ = nullptr;
	int64_t texture_id_;
	std::mutex mutex_;
	int fg_index_ = 0;
	bool buff_ready_ = false;
	size_t width_[2];
	size_t height_[2];
	std::vector<uint8_t> buffer_tmp_[2];
};

#endif  // TEXTURE_RGBA_H_
