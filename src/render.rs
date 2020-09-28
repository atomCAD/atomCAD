use crate::{
    bind_groups::{BindGroupLayouts, AsBindingResource as _},
    elements::Element,
    parts::Parts,
    utils::AsBytes as _,
    camera::Camera,
};
use std::{
    mem,
    sync::Arc,
    convert::TryInto as _,
};
use wgpu::util::DeviceExt as _;
use winit::{
    window::Window,
    dpi::PhysicalSize,
};

macro_rules! include_spirv {
    ($name:literal) => {
        wgpu::include_spirv!(concat!(env!("OUT_DIR"), "/shaders/", $name))
    };
}

pub struct Renderer {
    swap_chain_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    surface: wgpu::Surface,
    device: Arc<wgpu::Device>,
    queue: wgpu::Queue,

    atom_pipeline_layout: wgpu::PipelineLayout,
    atom_render_pipeline: wgpu::RenderPipeline,

    atom_vert_shader: wgpu::ShaderModule,
    atom_frag_shader: wgpu::ShaderModule,

    bgl: BindGroupLayouts,
    
    depth_texture: wgpu::TextureView,

    shader_runtime_config_buffer: wgpu::Buffer,
    global_bg: wgpu::BindGroup,
    camera: Camera,
}

impl Renderer {
    pub async fn new(window: &Window, swapchain_format: wgpu::TextureFormat) -> (Arc<wgpu::Device>, Self) {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let surface = unsafe { instance.create_surface(window) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("failed to find an appropriate adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    shader_validation: true,
                },
                None,
            )
            .await
            .expect("failed to create device");
        let device = Arc::new(device);
            
        let bgl = BindGroupLayouts::create(&device);

        let camera = Camera::new(&device, size, 0.7, 0.1);

        let shader_runtime_config_buffer =
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: Element::RENDERING_CONFIG.as_ref().as_bytes(),
            usage: wgpu::BufferUsage::STORAGE,
        });

        let global_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shader_runtime_config_bg"),
            layout: &bgl.global,
            entries: &[
                // camera
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_binding_resource(),
                },
                // shader runtime config (element attributes, e.g.)
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &shader_runtime_config_buffer,
                        offset: 0,
                        size: None,
                    },
                },
            ],
        });

        let atom_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl.global, &bgl.atoms],
            push_constant_ranges: &[],
        });

        let atom_vert_shader = device.create_shader_module(include_spirv!("billboard.vert"));
        let atom_frag_shader = device.create_shader_module(include_spirv!("billboard.frag"));

        let atom_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&atom_pipeline_layout),
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &atom_vert_shader,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &atom_frag_shader,
                entry_point: "main",
            }),
            rasterization_state: None, // this might not be right
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &[swapchain_format.into()],
            depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Greater,
                stencil: wgpu::StencilStateDescriptor::default(),
            }),
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[wgpu::VertexBufferDescriptor {
                    stride: mem::size_of::<ultraviolet::Mat4>() as _,
                    step_mode: wgpu::InputStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        // part and fragment transform matrix
                        0 => Float4,
                        1 => Float4,
                        2 => Float4,
                        3 => Float4,
                    ],
                }],
            },
            sample_count: 1, // TODO: Try multisampling?
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        let swap_chain_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let swap_chain = device.create_swap_chain(&surface, &swap_chain_desc);

        let depth_texture = device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            })
            .create_view(&wgpu::TextureViewDescriptor::default());

        (
            Arc::clone(&device),
            Self {
                swap_chain_desc,
                swap_chain,
                surface,
                device,
                queue,

                atom_pipeline_layout,
                atom_render_pipeline,

                atom_vert_shader,
                atom_frag_shader,

                bgl,

                depth_texture,

                shader_runtime_config_buffer,
                global_bg,
                camera,
            }
        )
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.swap_chain_desc.width = new_size.width;
        self.swap_chain_desc.height = new_size.height;

        self.swap_chain = self.device.create_swap_chain(&self.surface, &self.swap_chain_desc);

        self.depth_texture = self.device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: new_size.width,
                height: new_size.height,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        })
        .create_view(&wgpu::TextureViewDescriptor::default());

        self.camera.resize(new_size);
    }

    pub fn prepare_for_frame(&mut self) {
        self.camera.upload(&self.queue);

        // TODO: Upload all transformation matricies (maybe quats + offset in the future)?
        // Should I instead only send up a patch and have a compute shader rewrite the
        // correct matrices?
    }

    pub fn render(&self, parts: &Parts) {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: None,
        });

        let frame = self.swap_chain.get_current_frame().map(|mut frame| {
            if frame.suboptimal {
                // try again
                frame = self.swap_chain.get_current_frame().expect("could not retrieve swapchain on second try");
                if frame.suboptimal {
                    log::warn!("suboptimal swapchain frame");
                }
            }
            frame
        }).expect("failed to get next swapchain");

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &frame.output.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.8,
                            g: 0.8,
                            b: 0.8,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(
                    wgpu::RenderPassDepthStencilAttachmentDescriptor {
                        attachment: &self.depth_texture,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.0),
                            store: true,
                        }),
                        stencil_ops: None,
                    },
                ),
            });

            rpass.set_pipeline(&self.atom_render_pipeline);
            rpass.set_bind_group(0, &self.global_bg, &[]);

            for part in parts.parts() {
                // TODO: set vertex buffer to the right matrixes.

                for fragment in part.fragments() {
                    rpass.set_bind_group(1, &fragment.atoms().bind_group(), &[]);
                    rpass.draw(0..(fragment.atoms().len() * 3).try_into().unwrap(), 0..1)
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));

    }

    pub fn camera(&mut self) -> &mut Camera {
        &mut self.camera
    }

    pub fn bind_group_layouts(&self) -> &BindGroupLayouts {
        &self.bgl
    }
}
