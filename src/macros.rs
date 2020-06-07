// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/// This includes a file as a slice of `u32`s.
/// Useful for including compiled shaders.
macro_rules! include_shader_binary {
    ($path:literal) => {{
        struct AlignedAsU32<Bytes: ?Sized> {
            _align: [u32; 0],
            bytes: Bytes,
        }

        static ALIGNED: &AlignedAsU32<[u8]> = &AlignedAsU32 {
            _align: [],
            bytes: *include_bytes!(concat!(env!("OUT_DIR"), "/shaders/", $path)),
        };

        unsafe {
            std::slice::from_raw_parts(
                ALIGNED.bytes.as_ptr() as *const u32,
                ALIGNED.bytes.len() / 4,
            )
        }
    }};
}
