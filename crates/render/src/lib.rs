pub use crate::{
    atoms::{AtomKind, AtomRepr},
    camera::{Camera, CameraRepr, RenderCamera},
    world::{Fragment, FragmentId, Part, PartId, World},
};
use crate::{bind_groups::AsBindingResource as _, buffer_vec::BufferVec};
use common::AsBytes as _;
use periodic_table::PeriodicTable;
use std::{
    collections::{HashMap, HashSet},
    mem,
    sync::Arc,
};
use wgpu::util::DeviceExt as _;
use winit::{dpi::PhysicalSize, window::Window};

mod atoms;
mod bind_groups;
mod buffer_vec;
mod camera;
mod passes;
mod utils;
mod world;

#[macro_export]
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

const STORAGE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Default)]
pub struct Interactions {
    pub selected_fragments: HashSet<FragmentId>,
}

pub struct GlobalRenderResources {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) atom_bgl: wgpu::BindGroupLayout,
    pub(crate) linear_sampler: wgpu::Sampler,
    // pub(crate) staging_belt: Arc<Mutex<wgpu::util::StagingBelt>>,
}

pub struct RenderOptions {
    pub fxaa: Option<()>, // to be filled out with fxaa configuration options
}

pub struct Renderer {
    swap_chain_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    surface: wgpu::Surface,
    render_resources: Arc<GlobalRenderResources>,
    size: PhysicalSize<u32>,

    periodic_table: PeriodicTable,
    periodic_table_buffer: wgpu::Buffer,
    camera: RenderCamera,

    molecular_pass: passes::MolecularPass,
    fxaa_pass: passes::FxaaPass,
    blit_pass: passes::BlitPass,

    fragment_transforms: BufferVec,
    per_fragment: HashMap<FragmentId, (PartId, u64 /* transform offset */)>,

    options: RenderOptions,
}

impl Renderer {
    pub async fn new(
        window: &Window,
        options: RenderOptions,
    ) -> (Self, Arc<GlobalRenderResources>) {
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
                    features: wgpu::Features::DEVICE_BUFFER_ADDRESS,
                    limits: wgpu::Limits::default(),
                    shader_validation: true,
                },
                None,
            )
            .await
            .expect("failed to create device");

        let camera = RenderCamera::new_empty(&device, 0.7, 0.1);

        let periodic_table = PeriodicTable::new();

        let periodic_table_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: periodic_table.element_reprs.as_bytes(),
            usage: wgpu::BufferUsage::STORAGE,
        });

        let swap_chain_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: SWAPCHAIN_FORMAT,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let swap_chain = device.create_swap_chain(&surface, &swap_chain_desc);

        let atom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::StorageBuffer {
                    dynamic: false,
                    min_binding_size: None,
                    readonly: false,
                },
                count: None,
            }],
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        let render_resources = Arc::new(GlobalRenderResources {
            device,
            queue,
            atom_bgl,
            linear_sampler,
        });

        let (molecular_pass, color_texture) = passes::MolecularPass::new(
            &render_resources,
            camera.as_binding_resource(),
            &periodic_table_buffer,
            size,
        );
        let (fxaa_pass, fxaa_texture) =
            passes::FxaaPass::new(&render_resources, size, &color_texture);
        let blit_pass = passes::BlitPass::new(&render_resources, &fxaa_texture);

        let fragment_transforms = BufferVec::new(wgpu::BufferUsage::VERTEX);

        (
            Self {
                swap_chain_desc,
                swap_chain,
                surface,
                render_resources: Arc::clone(&render_resources),
                size,

                periodic_table,
                periodic_table_buffer,
                camera,

                molecular_pass,
                fxaa_pass,
                blit_pass,

                fragment_transforms,
                per_fragment: HashMap::new(),

                options,
            },
            render_resources,
        )
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.swap_chain_desc.width = new_size.width;
        self.swap_chain_desc.height = new_size.height;

        self.swap_chain = self
            .render_resources
            .device
            .create_swap_chain(&self.surface, &self.swap_chain_desc);

        let (color_texture, _normals_texture) =
            self.molecular_pass.update(&self.render_resources, new_size);
        let fxaa_texture = self
            .fxaa_pass
            .update(&self.render_resources, color_texture, new_size);
        self.blit_pass.update(&self.render_resources, fxaa_texture);

        self.camera.resize(new_size);
    }

    pub fn render(&mut self, world: &mut World, interactions: &Interactions) {
        let mut encoder = self
            .render_resources
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        if !self.camera.upload(&self.render_resources.queue) {
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

        self.molecular_pass.run(
            &mut encoder,
            world.fragments(),
            self.fragment_transforms.inner_buffer(),
            &self.per_fragment,
        );

        // if interactions.selected_fragments.len() != 0 {
        //     log::warn!("trying to render to stencil");
        //     // currently broken
        //     self.render_fragments_to_stencil(
        //         world,
        //         &mut encoder,
        //         interactions.selected_fragments.iter().copied(),
        //     );
        // }

        // run compute passes
        {
            let mut cpass = encoder.begin_compute_pass();

            self.fxaa_pass.run(&mut cpass);
        }

        // blit to screen
        self.blit_pass.run(&mut encoder, &frame.output.view);

        self.render_resources.queue.submit(Some(encoder.finish()));
    }

    /// Immediately calls resize on the supplied camera.
    pub fn set_camera<C: Camera + 'static>(&mut self, camera: C) {
        self.camera.set_camera(camera, self.size);
    }

    pub fn camera(&mut self) -> &mut RenderCamera {
        &mut self.camera
    }

    // pub fn update_render_config(&mut self, enabled: bool) {

    // }
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
            &self.render_resources,
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
                &self.render_resources,
                buffer_offset,
                transform.as_bytes(),
            );
        }
    }

    /// Render selected objects to the stencil buffer so they can be outlined post-process.
    // fn render_fragments_to_stencil(
    //     &self,
    //     world: &World,
    //     encoder: &mut wgpu::CommandEncoder,
    //     fragments: impl Iterator<Item = FragmentId>,
    // ) {
    //     let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    //         color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
    //             attachment: &self.unprocessed_texture,
    //             resolve_target: None,
    //             ops: wgpu::Operations {
    //                 load: wgpu::LoadOp::Load,
    //                 store: false,
    //             },
    //         }],
    //         depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
    //             attachment: &self.stencil_texture,
    //             depth_ops: None,
    //             stencil_ops: Some(wgpu::Operations {
    //                 load: wgpu::LoadOp::Clear(0),
    //                 store: true,
    //             }),
    //         }),
    //     });

    //     rpass.set_pipeline(&self.atom_render_pipeline);
    //     rpass.set_bind_group(0, &self.global_bg, &[]);

    //     let transform_buffer = self.fragment_transforms.inner_buffer();

    //     // TODO: This should probably be multithreaded.
    //     for fragment in fragments.map(|id| &world.fragments[&id]) {
    //         // TODO: set vertex buffer to the right matrices.
    //         let transform_offset = self.per_fragment[&fragment.id()].1;
    //         rpass.set_vertex_buffer(
    //             0,
    //             transform_buffer.slice(
    //                 transform_offset..transform_offset + mem::size_of::<ultraviolet::Mat4>() as u64,
    //             ),
    //         );

    //         rpass.set_bind_group(1, &fragment.atoms().bind_group(), &[]);
    //         rpass.draw(0..(fragment.atoms().len() * 3).try_into().unwrap(), 0..1)
    //     }
    // }

    fn create_texture(
        device: &wgpu::Device,
        size: PhysicalSize<u32>,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsage,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
        })
    }
}
