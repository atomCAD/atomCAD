use std::{
    slice,
    mem,
};
use winit::event::{WindowEvent, DeviceEvent};


pub enum InputEvent<'a> {
    Window(WindowEvent<'a>),
    Device(DeviceEvent),
}

pub unsafe trait AsBytes {
    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self as *const _ as *const u8, mem::size_of_val(self)) }
    }
}

unsafe impl<T> AsBytes for [T]
where
    T: AsBytes + Sized,
{
    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.as_ptr().cast(), mem::size_of::<T>() * self.len()) }
    }
}
