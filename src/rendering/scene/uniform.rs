// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use glsl_layout::AsStd140;
use std::{marker::PhantomData, mem};

pub struct Uniform<T> {
    buffer: wgpu::Buffer,
    _phantom: PhantomData<T>,
}

impl<T: AsStd140> Uniform<T>
where
    <T as AsStd140>::Std140: Sized,
{
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: mem::size_of::<T>() as u64,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            _phantom: PhantomData,
        }
    }

    pub fn buffer_view(&self) -> wgpu::BufferSlice {
        self.buffer.slice(..mem::size_of::<T>() as u64)
    }

    /// This will eventually be replaced with either `queue.write_buffer` or a staging buffer belt.
    pub fn set(&mut self, queue: &wgpu::Queue, data: T) {
        queue.write_buffer(&self.buffer, 0, glsl_layout::as_bytes(&data.std140()));
    }
}
