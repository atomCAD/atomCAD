// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::GlobalRenderResources;
use common::AsBytes;
use std::{cmp, mem};
use ultraviolet::Vec4;

#[derive(Copy, Clone, PartialEq)]
#[repr(C, align(16))]
pub struct BondRepr {
    pub start_pos: Vec4, // In the molecule frame
    pub end_pos: Vec4,
    pub order: u32,
    #[allow(unused)]
    pub pad: u32,
}

static_assertions::const_assert_eq!(mem::size_of::<BondRepr>(), 48);
unsafe impl AsBytes for BondRepr {}

/// Essentially a per-fragment uniform.
#[repr(C, align(16))]
#[derive(Default)]
pub struct BondBufferHeader;

unsafe impl AsBytes for BondBufferHeader {}

pub struct BondBuffer {
    bind_group: wgpu::BindGroup,
    number_of_bonds: usize,
}

impl BondBuffer {
    pub fn new<I>(gpu_resources: &GlobalRenderResources, iter: I) -> Self
    where
        I: IntoIterator<Item = BondRepr>,
        I::IntoIter: ExactSizeIterator,
    {
        let bonds = iter.into_iter();
        let number_of_bonds = bonds.len();
        assert!(number_of_bonds > 0, "must have at least one bond");

        // Serialize iterator into buffers
        let texel_count = if number_of_bonds <= 2048 {
            cmp::max(1, number_of_bonds)
        } else {
            (number_of_bonds + 2047) & !2047
        };
        let mut bond_pos =
            Vec::with_capacity((texel_count * 4 * mem::size_of::<f32>() + 255) & !255);
        let mut bond_kind = Vec::with_capacity((texel_count * mem::size_of::<u8>() + 255) & !255);
        for bond in bonds {
            bond_pos.extend_from_slice(bond.pos.as_bytes());
            bond_pos.extend_from_slice(&[0; 4]); // padding
            bond_kind.extend(&(bond.kind.0 as u8).to_ne_bytes());
        }
        bond_pos.resize(bond_pos.capacity(), 0);
        bond_kind.resize(bond_kind.capacity(), 0);

        assert_eq!(
            bond_pos.len() % 256,
            0,
            "texture row must be a multiple of 256 bytes"
        );
        assert_eq!(
            bond_kind.len() % 256,
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
            &bond_pos,
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
            &bond_kind,
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
                layout: &gpu_resources.bond_bgl,
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
            number_of_bonds,
        }
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn len(&self) -> usize {
        self.number_of_bonds
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// End of File
