use crate::SharedRenderState;
use common::AsBytes;
use std::{
    sync::Arc,
    marker::PhantomData,
};
use wgpu::util::StagingBelt;
use parking_lot::Mutex;

/// Does not contain a bind group.
pub struct GpuVec<T> {
    buffer: Option<wgpu::Buffer>,
    len: u64,
    capacity: u64,
    _marker: PhantomData<T>,
}

impl<T: AsBytes> GpuVec<T> {
    pub fn new(shared: &SharedRenderState) -> Self {
        Self {
            buffer: None,
            len: 0,
            capacity: 0,
            _marker: PhantomData,
        }
    }

    pub fn 
}
