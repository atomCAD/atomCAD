// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::{mem, slice};

/// # Safety
///
/// This is safe because it merely exposes the backing memory use natively to store the instance of
/// the type as a byte array.  It is tagged unsafe only because of the pointer and slice operations
/// involved.
///
/// Still, even though this won't result in memory leaks or dereferencing NULL, it is still
/// moderately unsafe as the direct memory storage layout may change across architectures.  Be very
/// careful with what you store or you will get inconsistent results across platforms.
pub unsafe trait AsBytes {
    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self as *const _ as *const u8, mem::size_of_val(self)) }
    }
}

#[macro_export]
macro_rules! impl_as_bytes {
    ($ty:ty) => {
        unsafe impl AsBytes for $ty {}
    };
    ($($ty:ty),*) => {
        $(
            impl_as_bytes!($ty);
        )*
    };
}

// Implement AsBytes for built-in types
impl_as_bytes!((), u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

// Implement AsBytes for ultraviolet types
impl_as_bytes!(
    ultraviolet::Vec2,
    ultraviolet::Vec3,
    ultraviolet::Mat2,
    ultraviolet::Mat3,
    ultraviolet::Mat4,
    ultraviolet::Rotor2,
    ultraviolet::Rotor3
);

unsafe impl<T> AsBytes for [T]
where
    T: AsBytes + Sized,
{
    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.as_ptr().cast(), mem::size_of_val(self)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Basic usage with primitive types.
    ///
    /// This test shows how to convert simple primitive values
    /// to byte slices and verify the correct size and content.
    #[test]
    fn test_primitives_as_bytes() {
        // Simple u32 conversion
        let value: u32 = 0x12345678;
        let bytes = value.as_bytes();

        // Verify size is correct
        assert_eq!(bytes.len(), 4); // u32 is 4 bytes

        // Check the actual bytes (this will vary by endianness)
        if cfg!(target_endian = "little") {
            assert_eq!(bytes, &[0x78, 0x56, 0x34, 0x12]);
        } else {
            assert_eq!(bytes, &[0x12, 0x34, 0x56, 0x78]);
        }

        // Float conversion
        let float_val: f32 = 1.0;
        let float_bytes = float_val.as_bytes();
        assert_eq!(float_bytes.len(), 4); // f32 is 4 bytes

        // 1.0f32 has a known bit pattern (0x3F800000), but byte order depends on endianness
        if cfg!(target_endian = "little") {
            assert_eq!(float_bytes, &[0x00, 0x00, 0x80, 0x3F]);
        } else {
            assert_eq!(float_bytes, &[0x3F, 0x80, 0x00, 0x00]);
        }
    }

    /// Working with arrays of primitives.
    ///
    /// This test shows how AsBytes works with arrays, demonstrating
    /// that the entire array is converted to a continuous byte sequence.
    #[test]
    fn test_arrays_as_bytes() {
        // Create an array of u16 values
        let array: [u16; 3] = [0x1122, 0x3344, 0x5566];
        let bytes = array.as_bytes();

        // Verify the size: 3 elements × 2 bytes each = 6 bytes
        assert_eq!(bytes.len(), 6);

        // Check byte values based on platform endianness
        if cfg!(target_endian = "little") {
            assert_eq!(bytes, &[0x22, 0x11, 0x44, 0x33, 0x66, 0x55]);
        } else {
            assert_eq!(bytes, &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        }
    }

    /// Custom struct conversion.
    ///
    /// This test introduces a custom struct with field alignment
    /// to demonstrate how memory layout affects byte representation.
    #[test]
    fn test_custom_struct_as_bytes() {
        // Define a simple struct with explicit repr(C) to ensure consistent layout
        #[repr(C)]
        struct Point {
            x: f32,
            y: f32,
        }

        // Implement AsBytes for our struct
        unsafe impl AsBytes for Point {}

        // Create an instance
        let point = Point { x: 1.0, y: 2.0 };
        let bytes = point.as_bytes();

        // Verify size is correct (8 bytes: 4 for each f32)
        assert_eq!(bytes.len(), 8);

        // Pattern for 1.0f32 and 2.0f32
        // 1.0f32 = 0x3F800000, 2.0f32 = 0x40000000
        if cfg!(target_endian = "little") {
            assert_eq!(bytes[0..4], [0x00, 0x00, 0x80, 0x3F]); // 1.0 in little endian
            assert_eq!(bytes[4..8], [0x00, 0x00, 0x00, 0x40]); // 2.0 in little endian
        } else {
            assert_eq!(bytes[0..4], [0x3F, 0x80, 0x00, 0x00]); // 1.0 in big endian
            assert_eq!(bytes[4..8], [0x40, 0x00, 0x00, 0x00]); // 2.0 in big endian
        }
    }

    /// Practical use case - GPU buffer data.
    ///
    /// This test shows how AsBytes is useful for preparing data to upload
    /// to GPU buffers, a common use case in graphics programming.
    #[test]
    fn test_gpu_buffer_preparation() {
        // Imagine these are vertex data for a 2D triangle
        #[repr(C)]
        struct Vertex {
            position: [f32; 2],
            color: [f32; 4], // RGBA
        }

        impl_as_bytes!(Vertex);

        // Create vertices for a triangle
        let vertices = [
            Vertex {
                position: [0.0, 0.5],
                color: [1.0, 0.0, 0.0, 1.0], // Red
            },
            Vertex {
                position: [-0.5, -0.5],
                color: [0.0, 1.0, 0.0, 1.0], // Green
            },
            Vertex {
                position: [0.5, -0.5],
                color: [0.0, 0.0, 1.0, 1.0], // Blue
            },
        ];

        // Get the byte representation
        let buffer_bytes = vertices.as_bytes();

        // Calculate expected size: 3 vertices, each with 2 position floats and 4 color floats
        // Total: 3 * (2 + 4) * 4 bytes = 72 bytes
        assert_eq!(buffer_bytes.len(), 72);

        // Validate the byte values for each vertex
        let zero: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
        let one: [u8; 4] = if cfg!(target_endian = "little") {
            [0x00, 0x00, 0x80, 0x3F]
        } else {
            [0x3F, 0x80, 0x00, 0x00]
        };
        let half: [u8; 4] = if cfg!(target_endian = "little") {
            [0x00, 0x00, 0x00, 0x3F]
        } else {
            [0x3F, 0x00, 0x00, 0x00]
        };
        let minus_half: [u8; 4] = if cfg!(target_endian = "little") {
            [0x00, 0x00, 0x00, 0xBF]
        } else {
            [0xBF, 0x00, 0x00, 0x00]
        };

        assert_eq!(buffer_bytes[0..4], zero);
        assert_eq!(buffer_bytes[4..8], half);
        assert_eq!(buffer_bytes[8..12], one);
        assert_eq!(buffer_bytes[12..16], zero);
        assert_eq!(buffer_bytes[16..20], zero);
        assert_eq!(buffer_bytes[20..24], one);

        assert_eq!(buffer_bytes[24..28], minus_half);
        assert_eq!(buffer_bytes[28..32], minus_half);
        assert_eq!(buffer_bytes[32..36], zero);
        assert_eq!(buffer_bytes[36..40], one);
        assert_eq!(buffer_bytes[40..44], zero);
        assert_eq!(buffer_bytes[44..48], one);

        assert_eq!(buffer_bytes[48..52], half);
        assert_eq!(buffer_bytes[52..56], minus_half);
        assert_eq!(buffer_bytes[56..60], zero);
        assert_eq!(buffer_bytes[60..64], zero);
        assert_eq!(buffer_bytes[64..68], one);
        assert_eq!(buffer_bytes[68..72], one);
    }
}

// End of File
