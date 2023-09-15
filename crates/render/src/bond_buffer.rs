// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::GlobalRenderResources;
use common::AsBytes;
use std::{cmp, mem};

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C, align(16))]
pub struct BondRepr {
    pub atom_1: u32,
    pub atom_2: u32,
    pub order: u8,
}

static_assertions::const_assert_eq!(mem::size_of::<BondRepr>(), 16);
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

        let capacity = (texel_count * 4 * mem::size_of::<f32>() + 255) & !255;
        let mut bond_a1 = Vec::with_capacity(capacity);
        let mut bond_a2 = Vec::with_capacity(capacity);
        let mut bond_order = Vec::with_capacity(capacity);

        for bond in bonds {
            bond_a1.extend(&(bond.atom_1).to_ne_bytes());
            bond_a2.extend(&(bond.atom_2).to_ne_bytes());
            bond_order.extend(&(bond.order).to_ne_bytes());
        }

        bond_a1.resize(bond_a1.capacity(), 0);
        bond_a2.resize(bond_a2.capacity(), 0);
        bond_order.resize(bond_order.capacity(), 0);

        assert_eq!(
            bond_a1.len() % 256,
            0,
            "texture row must be a multiple of 256 bytes"
        );
        assert_eq!(
            bond_a2.len() % 256,
            0,
            "texture row must be a multiple of 256 bytes"
        );
        assert_eq!(
            bond_order.len() % 256,
            0,
            "texture row must be a multiple of 256 bytes"
        );

        let size = wgpu::Extent3d {
            width: cmp::min(texel_count, 2048) as u32,
            height: ((texel_count + 2047) / 2048) as u32,
            depth_or_array_layers: 1,
        };

        let index_texture_descriptor = wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let bond_a1_texture = gpu_resources
            .device
            .create_texture(&index_texture_descriptor);
        let bond_a2_texture = gpu_resources
            .device
            .create_texture(&index_texture_descriptor);

        // The bond order is only a u8 so we give it a different descriptor
        let bond_order_texture = gpu_resources
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

        let data_layout = wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(size.width * 4 * mem::size_of::<f32>() as u32),
            rows_per_image: Some(size.height),
        };

        gpu_resources.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &bond_a1_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bond_a1,
            data_layout,
            size,
        );

        gpu_resources.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &bond_a2_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bond_a2,
            data_layout,
            size,
        );

        gpu_resources.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &bond_order_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bond_order,
            data_layout,
            size,
        );

        let bond_a1_texture_view =
            bond_a1_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bond_a2_texture_view =
            bond_a2_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bond_order_texture_view =
            bond_order_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = gpu_resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &gpu_resources.bond_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&bond_a1_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&bond_a2_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&bond_order_texture_view),
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
