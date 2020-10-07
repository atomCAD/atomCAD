use crate::GlobalRenderResources;
use common::AsBytes;
use std::{
    any::type_name,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    slice,
};
use wgpu::util::{BufferInitDescriptor, DeviceExt as _};

pub enum PushStategy {
    InPlace,
    Realloc,
}

/// Does not contain a bind group.
pub struct BufferVec<Header, T> {
    buffer: wgpu::Buffer,
    len: u64,
    capacity: u64,
    usage: wgpu::BufferUsage,
    _marker: PhantomData<(Header, T)>,
}

impl<Header, T> BufferVec<Header, T>
where
    Header: AsBytes,
    T: AsBytes,
{
    pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsage, header: Header) -> Self {
        assert!(
            mem::align_of::<Header>() <= 1
                || mem::align_of::<T>() <= 1
                || mem::align_of::<Header>() % mem::align_of::<T>() == 0,
            "align of `{}` must be a multiple of the align of `{}`",
            type_name::<Header>(),
            type_name::<T>(),
        );
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents: header.as_bytes(),
            usage: usage | wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::COPY_SRC,
        });

        Self {
            buffer,
            len: 0,
            capacity: 0,
            usage: usage | wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::COPY_SRC,
            _marker: PhantomData,
        }
    }

    pub fn new_with_data<F>(
        device: &wgpu::Device,
        usage: wgpu::BufferUsage,
        len: u64,
        fill: F,
    ) -> Self
    where
        F: FnOnce(&mut MaybeUninit<Header>, &mut [MaybeUninit<T>]),
    {
        assert!(
            mem::align_of::<Header>() <= 1
                || mem::align_of::<T>() <= 1
                || mem::align_of::<Header>() % mem::align_of::<T>() == 0,
            "align of `{}` must be a multiple of the align of `{}`",
            type_name::<Header>(),
            type_name::<T>(),
        );
        #[cfg(not(target_arch = "wasm32"))]
        let buffer = {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: mem::size_of::<Header>() as u64 + mem::size_of::<T>() as u64 * len,
                usage: usage | wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::COPY_SRC,
                mapped_at_creation: true,
            });

            {
                let mut buffer_view = buffer.slice(..).get_mapped_range_mut();

                let (header, rest) = buffer_view.split_at_mut(mem::size_of::<Header>());
                assert_eq!(header.len(), mem::size_of::<Header>());

                unsafe {
                    fill(
                        &mut *(header.as_mut_ptr() as *mut MaybeUninit<Header>),
                        slice::from_raw_parts_mut(
                            rest.as_mut_ptr() as *mut MaybeUninit<T>,
                            rest.len() / mem::size_of::<T>(),
                        ),
                    );
                }
            }
            buffer.unmap();

            // println!("buffer address: {:#x?}", unsafe {
            //     buffer.get_device_address()
            // });

            buffer
        };
        #[cfg(target_arch = "wasm32")]
        let buffer = {
            use wgpu::util::{BufferInitDescriptor, DeviceExt as _};
            let mut vec = vec![0; size as usize];

            let (header, rest) = vec.split_at_mut(mem::size_of::<Header>());
            assert_eq!(header.len(), mem::size_of::<Header>());

            unsafe {
                fill(
                    header.as_mut_ptr() as *mut MaybeUninit<Header>,
                    slice::from_raw_parts_mut(
                        rest.as_mut_ptr() as *mut MaybeUninit<T>,
                        rest.len() / mem::size_of::<T>(),
                    ),
                );
            }

            device.create_buffer_init(&BufferInitDescriptor {
                label: None,
                contents: &vec[..],
                usage: usage | wgpu::BufferUsage::COPY_DST,
            })
        };

        Self {
            buffer,
            len,
            capacity: len,
            usage: usage | wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::COPY_SRC,
            _marker: PhantomData,
        }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn inner_buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    #[must_use = "user must be aware if the buffer re-allocated or not"]
    pub fn push_small(
        &mut self,
        gpu_resources: &GlobalRenderResources,
        encoder: &mut wgpu::CommandEncoder,
        data: &[T],
    ) -> PushStategy {
        let offset = (mem::size_of::<Header>() as u64) + (mem::size_of::<T>() as u64) * self.len;

        if (data.len() as u64) <= self.capacity - self.len {
            // There's enough space to push this data immediately.
            gpu_resources
                .queue
                .write_buffer(&self.buffer, offset, data.as_bytes());
            self.len += data.len() as u64;

            PushStategy::InPlace
        } else {
            // we need to reallocate
            let new_capacity = (self.capacity * 2)
                .max((data.len() * 2) as u64)
                .next_power_of_two();

            log::info!(
                "allocating new buffer (`{}` + `{}`) with capacity of {} to fit {} ({} bytes -> {} bytes)",
                type_name::<Header>(),
                type_name::<T>(),
                new_capacity,
                data.len(),
                mem::size_of::<Header>() as u64 + new_capacity * mem::size_of::<T>() as u64,
                mem::size_of::<Header>() as u64 + (data.len() * mem::size_of::<T>()) as u64,
            );

            #[cfg(not(target_arch = "wasm32"))]
            let new_buffer = {
                let new_buffer = gpu_resources.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: mem::size_of::<Header>() as u64
                        + new_capacity * mem::size_of::<T>() as u64,
                    usage: self.usage,
                    mapped_at_creation: true,
                });

                {
                    let mut buffer_view = new_buffer
                        .slice(offset..offset + (data.len() * mem::size_of::<T>()) as u64)
                        .get_mapped_range_mut();
                    buffer_view.copy_from_slice(data.as_bytes());
                }

                new_buffer.unmap();
                new_buffer
            };
            #[cfg(target_arch = "wasm32")]
            let new_buffer = {
                let new_buffer = gpu_resources.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: mem::size_of::<Header>() as u64
                        + new_capacity * mem::size_of::<T>() as u64,
                    usage: self.usage,
                    mapped_at_creation: false,
                });
                gpu_resources
                    .queue
                    .write_buffer(&new_buffer, offset, data.as_bytes());
                new_buffer
            };

            encoder.copy_buffer_to_buffer(&self.buffer, 0, &new_buffer, 0, offset);

            self.buffer = new_buffer;
            self.capacity = new_capacity;
            self.len += data.len() as u64;

            PushStategy::Realloc
        }
        // if let Some(inner) = self.inner.as_mut() {

        // } else {
        //     let capacity = (data.len() * 2).next_power_of_two() as u64;
        //     log::info!(
        //         "allocating buffer with capacity of {} to fit {}",
        //         capacity,
        //         data.len()
        //     );
        //     // there's no buffer yet, let's allocate it + some extra space and fill it
        //     #[cfg(not(target_arch = "wasm32"))]
        //     let buffer = {
        //         let buffer = gpu_resources.device.create_buffer(&wgpu::BufferDescriptor {
        //             label: None,
        //             size: mem::size_of::<Header>() as u64 + capacity * mem::size_of::<T>() as u64,
        //             usage: self.usage,
        //             mapped_at_creation: true,
        //         });

        //         {
        //             let offset = mem
        //             let mut buffer_view = buffer.slice(..data.len() as u64).get_mapped_range_mut();
        //             buffer_view.copy_from_slice(data);
        //         }
        //         buffer.unmap();
        //         buffer
        //     };
        //     #[cfg(target_arch = "wasm32")]
        //     let buffer = {
        //         let buffer = gpu_resources.device.create_buffer(&wgpu::BufferDescriptor {
        //             label: None,
        //             size: capacity,
        //             usage: self.usage,
        //             mapped_at_creation: false,
        //         });
        //         gpu_resources.queue.write_buffer(&buffer, 0, data);
        //         buffer
        //     };

        //     self.inner = Some(BufferVecInner {
        //         buffer,
        //         len: data.len() as u64,
        //         capacity,
        //     });

        //     BufferVecOp::Realloc
        // }
    }

    // #[must_use = "user must be aware if the vector re-allocated or not"]
    // pub fn push_large -> BufferVecOp

    pub fn write_partial_small(
        &mut self,
        gpu_resources: &GlobalRenderResources,
        starting_index: u64,
        data: &[T],
    ) {
        if starting_index + (data.len() as u64) <= self.len {
            let offset =
                (mem::size_of::<Header>() as u64) + (data.len() * mem::size_of::<T>()) as u64;
            gpu_resources
                .queue
                .write_buffer(&self.buffer, offset, data.as_bytes());
        } else {
            panic!("attempting to partially write beyond buffer bounds")
        }
    }

    pub fn copy_new<F>(&self, gpu_resources: &GlobalRenderResources, f: F) -> Self
    where
        F: FnOnce(u64, &wgpu::Buffer /* from */, &wgpu::Buffer /* to */) -> u64,
    {
        unimplemented!()
    }
}
