// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::GlobalRenderResources;
use common::AsBytes;
use periodic_table::Element;
use std::{cmp, mem};
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

static_assertions::const_assert_eq!(mem::size_of::<AtomKind>(), 4);
unsafe impl AsBytes for AtomKind {}

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
    number_of_atoms: usize,
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

        // Serialize iterator into buffers
        let texel_count = if number_of_atoms <= 2048 {
            cmp::max(1, number_of_atoms)
        } else {
            (number_of_atoms + 2047) & !2047
        };
        let mut atom_pos =
            Vec::with_capacity((texel_count * 4 * mem::size_of::<f32>() + 255) & !255);
        let mut atom_kind = Vec::with_capacity((texel_count * mem::size_of::<u8>() + 255) & !255);
        for atom in atoms {
            atom_pos.extend_from_slice(atom.pos.as_bytes());
            atom_pos.extend_from_slice(&[0; 4]); // padding
            atom_kind.extend(&(atom.kind.0 as u8).to_ne_bytes());
        }
        atom_pos.resize(atom_pos.capacity(), 0);
        atom_kind.resize(atom_kind.capacity(), 0);

        assert_eq!(
            atom_pos.len() % 256,
            0,
            "texture row must be a multiple of 256 bytes"
        );
        assert_eq!(
            atom_kind.len() % 256,
            0,
            "texture row must be a multiple of 256 bytes"
        );

        let size = wgpu::Extent3d {
            width: cmp::min(texel_count, 2048) as u32,
            height: ((texel_count + 2047) / 2048) as u32,
            depth_or_array_layers: 1,
        };

        let pos_texture = gpu_resources
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

        gpu_resources.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &pos_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atom_pos,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(size.width * 4 * mem::size_of::<f32>() as u32),
                rows_per_image: Some(size.height),
            },
            size,
        );

        let kind_texture = gpu_resources
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

        gpu_resources.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &kind_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atom_kind,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(size.width * mem::size_of::<u8>() as u32),
                rows_per_image: Some(size.height),
            },
            size,
        );

        let pos_texture_view = pos_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let kind_texture_view = kind_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = gpu_resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &gpu_resources.atom_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&pos_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&kind_texture_view),
                    },
                ],
            });

        Self {
            bind_group,
            number_of_atoms,
        }
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn len(&self) -> usize {
        self.number_of_atoms
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// End of File
