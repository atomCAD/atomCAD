
/// This includes a file as a slice of `u32`s.
/// Useful for including compiled shaders.
macro_rules! include_u32_slice {
    ($path:literal) => {{
        struct AlignedAsU32<Bytes: ?Sized> {
            _align: [u32; 0],
            bytes: Bytes,
        }

        static ALIGNED: &AlignedAsU32<[u8]> = &AlignedAsU32 {
            _align: [],
            bytes: *include_bytes!($path),
        };

        unsafe {
            std::slice::from_raw_parts(ALIGNED.bytes.as_ptr() as *const u32, ALIGNED.bytes.len() / 4)
        }
    }};
}