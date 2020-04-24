use std::{
    slice,
    mem,
};
use ultraviolet::{Mat4, Vec3, projection::perspective_gl};

mod isosphere;
use isosphere::IsoSphere;

const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const SOBEL_FILTER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

pub unsafe trait Pod: Sized {
    fn bytes(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(
                self as *const Self as *const u8,
                mem::size_of::<Self>(),
            )
        }
    }
}
unsafe impl<T: Pod> Pod for &[T] {
    fn bytes(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(
                self.as_ptr() as *const u8,
                mem::size_of::<T>() * self.len(),
            )
        }
    }
}
unsafe impl Pod for u16 {}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}
unsafe impl Pod for Vertex {}

pub struct Entity {
    vertex_buffer: wgpu::Buffer,
    index_buffer: Option<wgpu::Buffer>,

    bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,

    vertex_num: usize,
}

pub struct Scene {
    global_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    normals_fbo: wgpu::Texture,
    // sobel_bind_group: wgpu::BindGroup,

    icosphere: Entity,
}

fn generate_matrix(aspect_ratio: f32) -> Mat4 {
    let opengl_to_wgpu_matrix: Mat4 = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.5, 0.0,
        0.0, 0.0, 0.5, 1.0,
    ].into();

    let mx_projection = perspective_gl(45_f32.to_radians(), aspect_ratio, 1.0, 10.0);
    let mx_view = Mat4::look_at(
        Vec3::new(1.5, -5.0, 3.0),
        Vec3::zero(),
        Vec3::unit_z(),
    );
    let mx_correction = opengl_to_wgpu_matrix;
    mx_correction * mx_projection * mx_view
}

impl Scene {
    pub fn new(device: &wgpu::Device, sc_desc: &wgpu::SwapChainDescriptor) -> Self {
        let mx_total = generate_matrix(sc_desc.width as f32 / sc_desc.height as f32);

        let uniform_buffer = device.create_buffer_with_data(
            mx_total.as_byte_slice(),
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        );

        let global_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::VERTEX,
                        ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                    }
                ],
                label: None,
            });

        let global_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &global_bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &uniform_buffer,
                        range: 0 .. mem::size_of::<Mat4>() as u64,
                    },
                }
            ],
            label: None,
        });

        let icosphere = create_unit_icosphere_entity(device, &global_bind_group_layout);

        let normals_fbo = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: sc_desc.width,
                height: sc_desc.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SOBEL_FILTER_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            label: None,
        });

        // let sobel_bind_group = {
        //     let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        //         bindings: &[
        //             wgpu::BindGroupLayoutEntry {
        //                 binding: 0,
        //                 visibility: wgpu::ShaderStage::COMPUTE,
        //                 ty: wgpu::BindingType::StorageTexture {
        //                     dimension: wgpu::TextureViewDimension::D2,
        //                     format: SOBEL_FILTER_FORMAT,
        //                     readonly: true,
        //                 },
        //             },
        //             wgpu::BindGroupLayoutEntry {
        //                 binding: 1,
        //                 visibility: wgpu::ShaderStage::COMPUTE,
        //                 ty: wgpu::BindingType::StorageTexture {
        //                     dimension: wgpu::TextureViewDimension::D2,
        //                     format: DEFAULT_FORMAT,
        //                     readonly: false,
        //                 },
        //             },
        //         ]
        //     });

        //     device.create_bind_group(&wgpu::BindGroupDescriptor {
        //         layout: &bind_group_layout,
        //         bindings: &[
        //             wgpu::Binding {
        //                 binding: 0,
        //                 resource: wgpu::BindingResource::TextureView(&normals_fbo.create_default_view()),
        //             },
        //             wgpu::Binding {
                        
        //             },
        //         ],
        //     })
        // };

        Self {
            global_bind_group,
            uniform_buffer,
            normals_fbo,

            icosphere,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, sc_desc: &wgpu::SwapChainDescriptor, encoder: &mut wgpu::CommandEncoder) {
        let mx_total = generate_matrix(sc_desc.width as f32 / sc_desc.height as f32);

        let matrix_src = device.create_buffer_with_data(
            mx_total.as_byte_slice(),
            wgpu::BufferUsage::COPY_SRC,
        );

        encoder.copy_buffer_to_buffer(
            &matrix_src,
            0,
            &self.uniform_buffer,
            0,
            mem::size_of_val(&mx_total) as u64,
        );

        self.normals_fbo = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: sc_desc.width,
                height: sc_desc.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SOBEL_FILTER_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            label: None,
        });
    }

    pub fn draw(&mut self, encoder: &mut wgpu::CommandEncoder, attachment: &wgpu::TextureView) {
        let normals_view = self.normals_fbo.create_default_view();

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[
                    wgpu::RenderPassColorAttachmentDescriptor {
                        attachment,
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::WHITE,
                    },
                    wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &normals_view,
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::TRANSPARENT,
                    },
                ],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.icosphere.render_pipeline);
            render_pass.set_bind_group(0, &self.global_bind_group, &[]);
            // render_pass.set_bind_group(1, &self.icosphere.bind_group, &[]);
            render_pass.set_vertex_buffer(0, &self.icosphere.vertex_buffer, 0, 0);
            // render_pass.set_bind_group(index, bind_group, offsets)
            render_pass.draw(0 .. self.icosphere.vertex_num as u32, 0..1);
        }

        {
            let mut compute_pass = encoder.begin_compute_pass();

        }
    }
}

fn create_unit_icosphere_entity(device: &wgpu::Device, global_bind_group_layout: &wgpu::BindGroupLayout) -> Entity {
    let vert_shader = include_shader_binary!("icosphere.vert");
    let frag_shader = include_shader_binary!("icosphere.frag");

    let vert_module = device.create_shader_module(vert_shader);
    let frag_module = device.create_shader_module(frag_shader);

    let icosphere = IsoSphere::new();

    let vertex_buffer = device
        .create_buffer_with_data(icosphere.vertices().bytes(), wgpu::BufferUsage::VERTEX);
    
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        bindings: &[],
        label: None,
    });
    
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        bindings: &[],
        label: None,
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[global_bind_group_layout],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        layout: &pipeline_layout,
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &vert_module,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &frag_module,
            entry_point: "main",
        }),
        rasterization_state: Some(wgpu::RasterizationStateDescriptor {
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: wgpu::CullMode::None,
            depth_bias: 0,
            depth_bias_slope_scale: 0.0,
            depth_bias_clamp: 0.0,
        }),
        primitive_topology: wgpu::PrimitiveTopology::TriangleList,
        color_states: &[
            wgpu::ColorStateDescriptor {
                format: DEFAULT_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            },
            wgpu::ColorStateDescriptor {
                format: SOBEL_FILTER_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            },
        ],
        depth_stencil_state: None,
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float3,
                        offset: 4 * 3,
                        shader_location: 1,
                    },
                ],
            }],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    });

    Entity {
        vertex_buffer,
        index_buffer: None,

        bind_group,
        render_pipeline,

        vertex_num: icosphere.vertices().len(),
    }
}

// fn create_cube_entity(device: &wgpu::Device) -> Entity {
    

//     let vertex_buffer = device
//         .create_buffer_with_data(vertex_data.as_slice().bytes(), wgpu::BufferUsage::VERTEX);
//     let index_buffer = device
//         .create_buffer_with_data(index_data.as_slice().bytes(), wgpu::BufferUsage::INDEX);
    
//     let bind_group_layout =
//         device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
//             bindings: &[wgpu::BindGroupLayoutEntry {
//                 binding: 0,
//                 visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
//                 ty: wgpu::BindingType::UniformBuffer { dynamic: false },
//             }],
//         });
    

//     Entity {
//         vertex_buffer,
//         index_buffer,


//     }
// }
