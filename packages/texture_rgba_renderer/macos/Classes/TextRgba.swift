//
//  TextRgba.swift
//  texture_rgba_renderer
//
//  Created by kingtous on 2023/2/17.
//

import Foundation
import FlutterMacOS
import CoreVideo

@objc public class TextRgba: NSObject, FlutterTexture {
    public var textureId: Int64 = -1
    private var registry: FlutterTextureRegistry?
    private var data: CVPixelBuffer?
    private var width: Int = 0
    private var height: Int = 0
    private let queue = DispatchQueue(label: "text_rgba_sync_queue")
    // macOS only support 32BGRA currently.
    private let dict: [String: Any] = [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferMetalCompatibilityKey as String: true,
            kCVPixelBufferOpenGLCompatibilityKey as String: true,
            // https://developer.apple.com/forums/thread/712709
            kCVPixelBufferBytesPerRowAlignmentKey as String: 64
        ]

    public static func new(registry: FlutterTextureRegistry?) -> TextRgba {
        let textRgba = TextRgba()
        textRgba.registry = registry
        textRgba.textureId = registry?.register(textRgba) ?? -1
        return textRgba
    }

    public func copyPixelBuffer() -> Unmanaged<CVPixelBuffer>? {
        queue.sync {
            if (data == nil) {
                return nil
            }
            return Unmanaged.passRetained(data!)
        }
    }

    private func _markFrameAvaliable(buffer: UnsafePointer<UInt8>, len: Int, width: Int, height: Int, stride_align: Int) -> Bool {
        // Calculate bytes per row: if stride_align is 0, assume tightly packed RGBA data
        let bytesPerRow = stride_align > 0 ? stride_align : width * 4
        
        // Create a mutable copy of the buffer data for CVPixelBuffer to manage
        let bufferSize = len
        let mutableBuffer = UnsafeMutableRawPointer.allocate(byteCount: bufferSize, alignment: MemoryLayout<UInt8>.alignment)
        mutableBuffer.copyMemory(from: buffer, byteCount: bufferSize)
        
        // Release callback to properly deallocate the buffer when CVPixelBuffer is done with it
        let releaseCallback: CVPixelBufferReleaseBytesCallback = { _, baseAddress in
            baseAddress?.deallocate()
        }
        
        var pixelBufferCopy: CVPixelBuffer?
        let result = CVPixelBufferCreateWithBytes(
            kCFAllocatorDefault,
            width,
            height,
            kCVPixelFormatType_32BGRA,
            mutableBuffer,
            bytesPerRow,
            releaseCallback,
            nil, // releaseRefCon
            dict as CFDictionary,
            &pixelBufferCopy
        )
        
        guard result == kCVReturnSuccess else {
            // Clean up on failure
            mutableBuffer.deallocate()
            return false
        }
        
        self.data = pixelBufferCopy
        self.width = width
        self.height = height
        
        if textureId != -1 && self.data != nil {
            registry?.textureFrameAvailable(textureId)
            return true
        } else {
            return false
        }
    }

    @objc public func markFrameAvaliableRaw(buffer: UnsafePointer<UInt8>, len: Int, width: Int, height: Int, stride_align: Int) -> Bool {
        queue.sync {
            _markFrameAvaliable(buffer: buffer, len: len, width: width, height: height, stride_align: stride_align)
        }
    }


    @objc public func markFrameAvaliable(data: Data, width: Int, height: Int, stride_align: Int) -> Bool {
        data.withUnsafeBytes { buffer in
            markFrameAvaliableRaw(
                buffer: buffer.baseAddress!.assumingMemoryBound(to: UInt8.self),
                len: data.count,
                width: width,
                height: height,
                stride_align: stride_align)
        }
    }
}
