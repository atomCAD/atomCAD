use std::{mem, slice};
use winit::event::{DeviceEvent, WindowEvent};

pub enum InputEvent<'a> {
    Window(WindowEvent<'a>),
    Device(DeviceEvent),
}

pub unsafe trait AsBytes {
    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self as *const _ as *const u8, mem::size_of_val(self)) }
    }
}

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
        unsafe { slice::from_raw_parts(self.as_ptr().cast(), mem::size_of::<T>() * self.len()) }
    }
}
