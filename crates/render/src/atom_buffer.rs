// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{buffer_vec::BufferVec, GlobalRenderResources};
use common::AsBytes;
use periodic_table::Element;
use std::mem::{self, MaybeUninit};
use ultraviolet::Vec3;

/// Packed bit field
/// | 0 .. 6 | ----------- | 7 .. 31 |
///   ^ atomic number - 1    ^ unspecified
///
/// TODO: Try using a buffer as an atom radius lookup table.
#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct AtomKind(u32);
impl AtomKind {
    pub fn new(element: Element) -> Self {
        Self(((element as u8 - 1) & 0b111_1111) as u32)
    }

    pub fn element(&self) -> Element {
        let n = (self.0 & 0b111_1111) as u8 + 1;
        Element::from_atomic_number(n)
            .unwrap_or_else(|| unreachable!("invalid atomic number in atom kind"))
    }
}

#[derive(Copy, Clone, PartialEq)]
#[repr(C)]
pub struct AtomRepr {
    pub pos: Vec3, // with respect to fragment center
    pub kind: AtomKind,
}

static_assertions::const_assert_eq!(mem::size_of::<AtomRepr>(), 16);
unsafe impl AsBytes for AtomRepr {}

/// Essentially a per-fragment uniform.
#[repr(C, align(16))]
#[derive(Default)]
pub struct AtomBufferHeader;

unsafe impl AsBytes for AtomBufferHeader {}

pub struct AtomBuffer {
    bind_group: wgpu::BindGroup,
    buffer: BufferVec<AtomBufferHeader, AtomRepr>,
    // number_of_atoms: usize,
}

impl AtomBuffer {
    pub fn new<I>(gpu_resources: &GlobalRenderResources, iter: I) -> Self
    where
        I: IntoIterator<Item = AtomRepr>,
        I::IntoIter: ExactSizeIterator,
    {
        let atoms = iter.into_iter();
        let number_of_atoms = atoms.len();

        assert!(number_of_atoms > 0, "must have at least one atom");

        let buffer = BufferVec::new_with_data(
            &gpu_resources.device,
            wgpu::BufferUsages::STORAGE,
            number_of_atoms as u64,
            |header, array| {
                unsafe {
                    std::ptr::write_unaligned(
                        header.as_mut_ptr() as *mut MaybeUninit<AtomBufferHeader>,
                        MaybeUninit::new(AtomBufferHeader),
                    );
                }

                for (block, atom) in array.iter_mut().zip(atoms) {
                    // block.write(atom);
                    unsafe {
                        std::ptr::write_unaligned(block, MaybeUninit::new(atom));
                    }
                }
            },
        );

        assert!(std::mem::size_of::<AtomBufferHeader>() % gpu_resources.device.limits().min_storage_buffer_offset_alignment as usize == 0, "AtomBufferHeader's size needs to be an integer multiple of the min storage buffer offset alignment of the gpu. See https://github.com/shinzlet/atomCAD/issues/1");
        let bind_group = gpu_resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &gpu_resources.atom_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: buffer.inner_buffer(),
                        offset: std::mem::size_of::<AtomBufferHeader>() as u64,
                        size: None,
                    }),
                }],
            });

        Self {
            bind_group,
            buffer,
            // number_of_atoms,
        }
    }

    pub fn copy_new(&self, render_resources: &GlobalRenderResources) -> Self {
        let buffer = self.buffer.copy_new(render_resources, false);

        render_resources
            .queue
            .write_buffer(buffer.inner_buffer(), 0, AtomBufferHeader.as_bytes());

        let bind_group = render_resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &render_resources.atom_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: buffer.inner_buffer(),
                        offset: std::mem::size_of::<AtomBufferHeader>() as u64,
                        size: None,
                    }),
                }],
            });

        Self {
            bind_group,
            buffer,
            // number_of_atoms: self.number_of_atoms,
        }
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn len(&self) -> usize {
        self.buffer.len() as usize
        // self.number_of_atoms
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn with_buffer(&mut self, f: impl Fn(&mut BufferVec<AtomBufferHeader, AtomRepr>)) {
        f(&mut self.buffer);
    }

    pub fn reupload_atoms(
        &mut self,
        atoms: &[AtomRepr],
        gpu_resources: &GlobalRenderResources,
    ) -> crate::buffer_vec::PushStategy {
        let mut encoder = gpu_resources
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.buffer.clear();
        println!("buffer size after clear: {}", self.buffer.len());

        let ret = self.buffer.push_small(gpu_resources, &mut encoder, atoms);
        println!("buffer size after push_small: {}", self.buffer.len());
        ret
    }
}

// End of File
