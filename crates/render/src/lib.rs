use crate::bind_groups::{AsBindingResource as _, BindGroupLayouts};
pub use crate::{
    camera::{Camera, CameraRepr, RenderCamera},
    parts::Parts,
};
use common::AsBytes as _;
use periodic_table::Element;
use std::{convert::TryInto as _, mem, sync::Arc};
use wgpu::util::DeviceExt as _;
use wgpu_conveyor::{AutomatedBuffer, AutomatedBufferManager, UploadStyle};
use winit::{dpi::PhysicalSize, window::Window};

mod atoms;
mod bind_groups;
mod camera;
mod parts;
mod utils;

macro_rules! include_spirv {
    ($name:literal) => {
        wgpu::include_spirv!(concat!(env!("OUT_DIR"), "/shaders/", $name))
    };
}

const SWAPCHAIN_FORMAT: wgpu::TextureFormat = if cfg!(target_arch = "wasm32") {
    wgpu::TextureFormat::Bgra8Unorm
} else {
    wgpu::TextureFormat::Bgra8UnormSrgb
};

pub struct Renderer {
    swap_chain_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    surface: wgpu::Surface,

    size: PhysicalSize<u32>,

    device: Arc<wgpu::Device>,
    queue: wgpu::Queue,
    buffer_mananger: AutomatedBufferManager,

    atom_pipeline_layout: wgpu::PipelineLayout,
    atom_render_pipeline: wgpu::RenderPipeline,
    atom_transform_buffer: AutomatedBuffer,

    atom_vert_shader: wgpu::ShaderModule,
    atom_frag_shader: wgpu::ShaderModule,

    bgl: BindGroupLayouts,

    depth_texture: wgpu::TextureView,

    shader_runtime_config_buffer: wgpu::Buffer,
    global_bg: wgpu::BindGroup,
    camera: RenderCamera,
}

impl Renderer {
    pub async fn new(window: &Window) -> (Arc<wgpu::Device>, Self) {
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

        let mut buffer_mananger = AutomatedBufferManager::new(UploadStyle::Staging);

        let bgl = BindGroupLayouts::create(&device);

        let camera = RenderCamera::new_empty(&device, 0.7, 0.1);

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
            color_states: &[SWAPCHAIN_FORMAT.into()],
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

        let atom_transform_buffer =
            buffer_mananger.create_new_buffer(&device, 0, wgpu::BufferUsage::VERTEX, None::<&str>);

        let swap_chain_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: SWAPCHAIN_FORMAT,
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

                size,

                device,
                queue,
                buffer_mananger,

                atom_pipeline_layout,
                atom_render_pipeline,
                atom_transform_buffer,

                atom_vert_shader,
                atom_frag_shader,

                bgl,

                depth_texture,

                shader_runtime_config_buffer,
                global_bg,
                camera,
            },
        )
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.swap_chain_desc.width = new_size.width;
        self.swap_chain_desc.height = new_size.height;

        self.swap_chain = self
            .device
            .create_swap_chain(&self.surface, &self.swap_chain_desc);

        self.depth_texture = self
            .device
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

    /// TODO: Upload any new transforms or transforms that changed
    pub fn upload_transforms(&mut self, encoder: &mut wgpu::CommandEncoder, parts: &Parts) {
        // let transform_count: usize = parts
        //     .iter()
        //     .map(|part| part.fragments().len() * mem::size_of::<Mat4>())
        //     .sum();

        // self.atom_transform_buffer.write_to_buffer(
        //     &self.device,
        //     encoder,
        //     transform_count as u64,
        //     |buffer| {
        //         for part in parts.iter() {
        //             let part_transform = todo!();
        //             let fragment_transform = todo!();
        //         }
        //     },
        // );
    }

    pub fn render(&mut self, parts: &Parts) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        if !self.camera.upload(&self.queue) {
            // no camera is set, so no reason to do rendering.
            return;
        }

        self.upload_transforms(&mut encoder, parts);

        let frame = self
            .swap_chain
            .get_current_frame()
            .map(|mut frame| {
                if frame.suboptimal {
                    // try again
                    frame = self
                        .swap_chain
                        .get_current_frame()
                        .expect("could not retrieve swapchain on second try");
                    if frame.suboptimal {
                        log::warn!("suboptimal swapchain frame");
                    }
                }
                frame
            })
            .expect("failed to get next swapchain");

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
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                    attachment: &self.depth_texture,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            rpass.set_pipeline(&self.atom_render_pipeline);
            rpass.set_bind_group(0, &self.global_bg, &[]);

            for part in parts.iter() {
                // TODO: set vertex buffer to the right matrixes.

                for fragment in part.fragments() {
                    rpass.set_bind_group(1, &fragment.atoms().bind_group(), &[]);
                    rpass.draw(0..(fragment.atoms().len() * 3).try_into().unwrap(), 0..1)
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
    }

    /// Immediately calls resize on the supplied camera.
    pub fn set_camera<C: Camera + 'static>(&mut self, camera: C) {
        self.camera.set_camera(camera, self.size);
    }

    pub fn camera(&mut self) -> &mut RenderCamera {
        &mut self.camera
    }

    pub fn bind_group_layouts(&self) -> &BindGroupLayouts {
        &self.bgl
    }
}
