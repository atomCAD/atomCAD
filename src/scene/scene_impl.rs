// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::mem;
use ultraviolet::{Mat4, Vec3};
use winit::{dpi::PhysicalSize, event::ElementState};

use super::{Mouse, Scene, State, DEFAULT_FORMAT, NORMAL_FORMAT};

impl State {
    pub fn new() -> Self {
        State {
            mouse: Mouse {
                old_cursor: None,
                cursor: None,
                left_button: ElementState::Released,
            },
        }
    }
}

impl Scene {
    pub fn new(device: &wgpu::Device, size: PhysicalSize<u32>) -> Scene {
        let camera = Vec3::new(1.5, -5.0, 3.0);

        let mx_total = super::generate_matrix(camera, size.width as f32 / size.height as f32);

        let uniform_buffer = device.create_buffer_with_data(
            mx_total.as_byte_slice(),
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        );

        let global_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                }],
                label: if cfg!(build = "debug") {
                    Some("scene global bind group layout")
                } else {
                    None
                },
            });

        let global_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &global_bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &uniform_buffer,
                    range: 0..mem::size_of::<Mat4>() as u64,
                },
            }],
            label: if cfg!(build = "debug") {
                Some("scene global bind group")
            } else {
                None
            },
        });

        let icosphere = super::create_unit_icosphere_entity(&device, &global_bind_group_layout);

        // Create the texture that normals are stored in.
        // This is used for filters.
        let normals_fbo = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: NORMAL_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            label: if cfg!(build = "debug") {
                Some("scene normal texture")
            } else {
                None
            },
        });

        // The scene renders to this texture.
        // The main (UI) thread has a view of this texture and copies
        // from it at 60fps.
        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEFAULT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: if cfg!(build = "debug") {
                Some("scene render texture")
            } else {
                None
            },
        });

        Self {
            global_bind_group,
            uniform_buffer,
            normals_fbo,
            render_texture,
            size,
            world_mx: mx_total,
            camera,

            icosphere,

            state: State::new(),
        }
    }
}
