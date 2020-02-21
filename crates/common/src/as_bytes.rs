// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

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

impl_as_bytes!(
    (),
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

// End of File
