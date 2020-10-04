use crate::GlobalGpuResources;

struct BufferVecInner {
    buffer: wgpu::Buffer,
    len: u64,
    capacity: u64,
}

pub enum BufferVecOp {
    InPlace,
    Realloc,
}

/// Does not contain a bind group.
pub struct BufferVec {
    inner: Option<BufferVecInner>,
    usage: wgpu::BufferUsage,
}

impl BufferVec {
    pub fn new(usage: wgpu::BufferUsage) -> Self {
        Self {
            inner: None,
            usage: usage | wgpu::BufferUsage::COPY_DST,
        }
    }

    pub fn new_with_data<F>(
        device: &wgpu::Device,
        usage: wgpu::BufferUsage,
        size: u64,
        fill: F,
    ) -> Self
    where
        F: FnOnce(&mut [u8]),
    {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: usage | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: true,
        });

        {
            let mut buffer_view = buffer.slice(..).get_mapped_range_mut();
            fill(&mut buffer_view);
        }
        buffer.unmap();

        Self {
            inner: Some(BufferVecInner {
                buffer,
                len: size,
                capacity: size,
            }),
            usage: usage | wgpu::BufferUsage::COPY_DST,
        }
    }

    pub fn len(&self) -> u64 {
        self.inner.as_ref().map(|inner| inner.len).unwrap_or(0)
    }

    pub fn inner_buffer(&self) -> &wgpu::Buffer {
        &self.inner.as_ref().unwrap().buffer
    }

    #[must_use = "user must be aware if the vector re-allocated or not"]
    pub fn push_small(
        &mut self,
        gpu_resources: &GlobalGpuResources,
        encoder: &mut wgpu::CommandEncoder,
        data: &[u8],
    ) -> BufferVecOp {
        if let Some(inner) = self.inner.as_mut() {
            if inner.capacity - inner.len >= data.len() as u64 {
                // There's enough space to push this data immediately.
                gpu_resources
                    .queue
                    .write_buffer(&inner.buffer, inner.len, data);
                inner.len += data.len() as u64;

                BufferVecOp::InPlace
            } else {
                // we need to reallocate
                let new_capacity = (inner.capacity * 2)
                    .max((data.len() * 2) as u64)
                    .next_power_of_two();

                log::info!(
                    "allocating new buffer with capacity of {} to fit {}",
                    new_capacity,
                    data.len()
                );

                let new_buffer = gpu_resources.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: new_capacity,
                    usage: self.usage,
                    mapped_at_creation: true,
                });

                {
                    let mut buffer_view = new_buffer
                        .slice(inner.len..(inner.len + data.len() as u64))
                        .get_mapped_range_mut();
                    buffer_view.copy_from_slice(data);
                }

                new_buffer.unmap();

                encoder.copy_buffer_to_buffer(&inner.buffer, 0, &new_buffer, 0, inner.len);

                inner.buffer = new_buffer;
                inner.capacity = new_capacity;
                inner.len += data.len() as u64;

                BufferVecOp::Realloc
            }
        } else {
            let capacity = (data.len() * 2).next_power_of_two() as u64;
            log::info!(
                "allocating buffer with capacity of {} to fit {}",
                capacity,
                data.len()
            );
            // there's no buffer yet, let's allocate it + some extra space and fill it
            let buffer = gpu_resources.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: capacity,
                usage: self.usage,
                mapped_at_creation: true,
            });

            {
                let mut buffer_view = buffer.slice(..data.len() as u64).get_mapped_range_mut();
                buffer_view.copy_from_slice(data);
            }
            buffer.unmap();

            self.inner = Some(BufferVecInner {
                buffer,
                len: data.len() as u64,
                capacity,
            });

            BufferVecOp::Realloc
        }
    }

    // #[must_use = "user must be aware if the vector re-allocated or not"]
    // pub fn push_large -> BufferVecOp

    pub fn write_partial_small(
        &mut self,
        gpu_resources: &GlobalGpuResources,
        offset: u64,
        data: &[u8],
    ) {
        let inner = self.inner.as_ref().expect(
            "you must have already instantiated this buffer vec to call `write_partial_small`",
        );
        if offset + (data.len() as u64) <= inner.len {
            gpu_resources
                .queue
                .write_buffer(&inner.buffer, offset, data);
        } else {
            panic!("attempting to partially write beyond buffer bounds")
        }
    }
}
