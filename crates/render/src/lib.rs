pub use crate::{
    atoms::{AtomKind, AtomRepr},
    camera::{Camera, CameraRepr, RenderCamera},
    world::{Fragment, FragmentId, Part, PartId, World},
};
use crate::{
    bind_groups::{AsBindingResource as _, BindGroupLayouts},
    buffer_vec::BufferVec,
};
use common::AsBytes as _;
use periodic_table::{Element, PeriodicTable};
use std::{
    collections::{HashMap, HashSet},
    convert::TryInto as _,
    mem,
    sync::Arc,
};
use wgpu::util::DeviceExt as _;
use winit::{dpi::PhysicalSize, window::Window};

mod atoms;
mod bind_groups;
mod buffer_vec;
mod camera;
mod utils;
mod world;

macro_rules! include_spirv {
    ($name:literal) => {
        wgpu::include_spirv!(concat!(env!("OUT_DIR"), "/shaders/", $name))
    };
}

const SWAPCHAIN_FORMAT: wgpu::TextureFormat = if cfg!(target_arch = "wasm32") {
    // srgb doesn't work correctly in firefox rn, so we're manually converting to it in the shader
    wgpu::TextureFormat::Bgra8Unorm
} else {
    wgpu::TextureFormat::Bgra8UnormSrgb
};

#[derive(Default)]
pub struct Interactions {
    pub selected_fragments: HashSet<FragmentId>,
}

pub struct GlobalGpuResources {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) bgl: BindGroupLayouts,
    // pub(crate) staging_belt: Arc<Mutex<wgpu::util::StagingBelt>>,
}

pub struct Renderer {
    swap_chain_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    surface: wgpu::Surface,
    gpu_resources: Arc<GlobalGpuResources>,
    size: PhysicalSize<u32>,

    periodic_table: PeriodicTable,

    atom_pipeline_layout: wgpu::PipelineLayout,
    atom_render_pipeline: wgpu::RenderPipeline,
    atom_vert_shader: wgpu::ShaderModule,
    atom_frag_shader: wgpu::ShaderModule,

    depth_texture: wgpu::TextureView,
    stencil_texture: wgpu::TextureView,
    // for deferred rendering/ambient occlusion approximation
    normals_texture: wgpu::TextureView,

    shader_runtime_config_buffer: wgpu::Buffer,
    global_bg: wgpu::BindGroup,
    camera: RenderCamera,

    fragment_transforms: BufferVec,
    per_fragment: HashMap<FragmentId, (PartId, u64 /* transform offset */)>,
}

impl Renderer {
    pub async fn new(window: &Window) -> (Self, Arc<GlobalGpuResources>) {
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

        let bgl = BindGroupLayouts::create(&device);

        let camera = RenderCamera::new_empty(&device, 0.7, 0.1);

        let periodic_table = PeriodicTable::new();

        let shader_runtime_config_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: periodic_table.element_reprs.as_bytes(),
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
            color_states: &[
                SWAPCHAIN_FORMAT.into(),
                wgpu::TextureFormat::Rgba16Float.into(),
            ],
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
            sample_count: 1, // multisampling doesn't work for shader effects (like spherical imposters/billboards)
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        let swap_chain_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: SWAPCHAIN_FORMAT,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let swap_chain = device.create_swap_chain(&surface, &swap_chain_desc);

        let depth_texture = Self::create_depth_texture(&device, size);
        let stencil_texture = Self::create_stencil_buffer(&device, size);
        let normals_texture = Self::create_normals_texture(&device, size);

        let gpu_resources = Arc::new(GlobalGpuResources { device, queue, bgl });

        let fragment_transforms = BufferVec::new(wgpu::BufferUsage::VERTEX);

        (
            Self {
                swap_chain_desc,
                swap_chain,
                surface,
                gpu_resources: Arc::clone(&gpu_resources),
                size,

                periodic_table,

                atom_pipeline_layout,
                atom_render_pipeline,
                // atom_transform_buffer,
                atom_vert_shader,
                atom_frag_shader,

                depth_texture,
                stencil_texture,
                normals_texture,

                shader_runtime_config_buffer,
                global_bg,
                camera,

                fragment_transforms,
                per_fragment: HashMap::new(),
            },
            gpu_resources,
        )
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.swap_chain_desc.width = new_size.width;
        self.swap_chain_desc.height = new_size.height;

        self.swap_chain = self
            .gpu_resources
            .device
            .create_swap_chain(&self.surface, &self.swap_chain_desc);

        self.depth_texture = Self::create_depth_texture(&self.gpu_resources.device, new_size);
        self.stencil_texture = Self::create_stencil_buffer(&self.gpu_resources.device, new_size);
        self.normals_texture = Self::create_normals_texture(&self.gpu_resources.device, new_size);

        self.camera.resize(new_size);
    }

    pub fn render(&mut self, world: &mut World, interactions: &Interactions) {
        let mut encoder = self
            .gpu_resources
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        if !self.camera.upload(&self.gpu_resources.queue) {
            log::warn!("no camera is set");
            // no camera is set, so no reason to do rendering.
            return;
        }

        self.upload_new_transforms(&mut encoder, world);
        self.update_transforms(&mut encoder, world);

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

        self.render_all_fragments(world, &frame.output.view, &mut encoder);

        if interactions.selected_fragments.len() != 0 {
            log::warn!("trying to render to stencil");
            // currently broken
            self.render_fragments_to_stencil(
                world,
                &frame.output.view,
                &mut encoder,
                interactions.selected_fragments.iter().copied(),
            );
        }

        self.gpu_resources.queue.submit(Some(encoder.finish()));
    }

    /// Immediately calls resize on the supplied camera.
    pub fn set_camera<C: Camera + 'static>(&mut self, camera: C) {
        self.camera.set_camera(camera, self.size);
    }

    pub fn camera(&mut self) -> &mut RenderCamera {
        &mut self.camera
    }
}

impl Renderer {
    fn upload_new_transforms(&mut self, encoder: &mut wgpu::CommandEncoder, world: &mut World) {
        if world.added_parts.len() + world.added_fragments.len() == 0 {
            return;
        }

        let (parts, fragments) = (&world.parts, &world.fragments);

        let added_fragments = world.added_fragments.drain(..).chain(
            world
                .added_parts
                .drain(..)
                .map(|part_id| {
                    parts[&part_id]
                        .fragments()
                        .iter()
                        .copied()
                        .map(move |id| (part_id, id))
                })
                .flatten(),
        );

        let mut buffer_offset = self.fragment_transforms.len();

        let transforms: Vec<_> = added_fragments
            .map(|(part_id, fragment_id)| {
                self.per_fragment
                    .insert(fragment_id, (part_id, buffer_offset));
                buffer_offset += mem::size_of::<ultraviolet::Mat4>() as u64;

                let part = &parts[&part_id];
                let fragment = &fragments[&fragment_id];

                let offset = part.offset() + fragment.offset();
                let rotation = part.rotation() * fragment.rotation();

                rotation
                    .into_matrix()
                    .into_homogeneous()
                    .translated(&offset)
            })
            .collect();

        // This doesn't use a bind group.
        // Eventually switch this to `push_large`, once it's written.
        let _ = self.fragment_transforms.push_small(
            &self.gpu_resources,
            encoder,
            transforms[..].as_bytes(),
        );
    }

    fn update_transforms(&mut self, _encoder: &mut wgpu::CommandEncoder, world: &mut World) {
        if world.modified_parts.len() + world.modified_fragments.len() == 0 {
            return;
        }

        let (parts, fragments) = (&world.parts, &world.fragments);

        let modified_fragments = world.modified_fragments.drain(..).chain(
            world
                .modified_parts
                .drain(..)
                .map(|part_id| parts[&part_id].fragments().iter().copied())
                .flatten(),
        );

        for fragment_id in modified_fragments {
            let (part_id, buffer_offset) = self.per_fragment[&fragment_id];

            let part = &parts[&part_id];
            let fragment = &fragments[&fragment_id];

            let offset = part.offset() + fragment.offset();
            let rotation = part.rotation() * fragment.rotation();

            let transform = rotation
                .into_matrix()
                .into_homogeneous()
                .translated(&offset);

            self.fragment_transforms.write_partial_small(
                &self.gpu_resources,
                buffer_offset,
                transform.as_bytes(),
            );
        }
    }

    fn render_all_fragments(
        &self,
        world: &World,
        attachment: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment,
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
                },
                // multiple render targets
                // render to normals texture
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &self.normals_texture,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                },
            ],
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

        let transform_buffer = self.fragment_transforms.inner_buffer();

        // TODO: This should probably be multithreaded.
        for fragment in world.fragments() {
            // TODO: set vertex buffer to the right matrices.
            let transform_offset = self.per_fragment[&fragment.id()].1;
            rpass.set_vertex_buffer(
                0,
                transform_buffer.slice(
                    transform_offset..transform_offset + mem::size_of::<ultraviolet::Mat4>() as u64,
                ),
            );

            rpass.set_bind_group(1, &fragment.atoms().bind_group(), &[]);
            rpass.draw(0..(fragment.atoms().len() * 3).try_into().unwrap(), 0..1)
        }
    }

    /// Render selected objects to the stencil buffer so they can be outlined post-process.
    fn render_fragments_to_stencil(
        &self,
        world: &World,
        attachment: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        fragments: impl Iterator<Item = FragmentId>,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: false,
                },
            }],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: &self.stencil_texture,
                depth_ops: None,
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: true,
                }),
            }),
        });

        rpass.set_pipeline(&self.atom_render_pipeline);
        rpass.set_bind_group(0, &self.global_bg, &[]);

        let transform_buffer = self.fragment_transforms.inner_buffer();

        // TODO: This should probably be multithreaded.
        for fragment in fragments.map(|id| &world.fragments[&id]) {
            // TODO: set vertex buffer to the right matrices.
            let transform_offset = self.per_fragment[&fragment.id()].1;
            rpass.set_vertex_buffer(
                0,
                transform_buffer.slice(
                    transform_offset..transform_offset + mem::size_of::<ultraviolet::Mat4>() as u64,
                ),
            );

            rpass.set_bind_group(1, &fragment.atoms().bind_group(), &[]);
            rpass.draw(0..(fragment.atoms().len() * 3).try_into().unwrap(), 0..1)
        }
    }

    fn create_depth_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::TextureView {
        device
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
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn create_stencil_buffer(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: size.width / 8,
                    height: size.height / 8,
                    depth: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Uint, // This isn't the correct format, should be `Stencil8`
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn create_normals_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::TextureView {
        device
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
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
}
