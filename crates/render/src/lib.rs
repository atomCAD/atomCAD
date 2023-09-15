// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub use crate::{
    atom_buffer::{AtomBuffer, AtomKind, AtomRepr},
    bond_buffer::{BondBuffer, BondRepr},
    camera::{Camera, CameraRepr, RenderCamera},
};
use crate::{bind_groups::AsBindingResource as _, buffer_vec::BufferVec};
use common::AsBytes as _;
use periodic_table::PeriodicTable;
use std::rc::Rc;
use ultraviolet::Vec2;
use wgpu::util::DeviceExt as _;
use winit::{dpi::PhysicalSize, window::Window};

mod atom_buffer;
mod bind_groups;
mod bond_buffer;
mod buffer_vec;
mod camera;
mod passes;

#[macro_export]
macro_rules! include_spirv {
    ($name:literal) => {
        wgpu::include_spirv!(concat!(env!("OUT_DIR"), "/shaders/", $name))
    };
}

const SWAPCHAIN_FORMAT: wgpu::TextureFormat =
    if cfg!(any(target_os = "android", target_arch = "wasm32")) {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Bgra8UnormSrgb
    };

#[derive(Default)]
pub struct Interactions {
    // pub selected_fragments: HashSet<FragmentId>,
}

pub struct GlobalRenderResources {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) atom_bgl: wgpu::BindGroupLayout,
    pub(crate) bond_bgl: wgpu::BindGroupLayout,
    pub(crate) linear_sampler: wgpu::Sampler,
    // pub(crate) staging_belt: Arc<Mutex<wgpu::util::StagingBelt>>,
}

pub struct RenderOptions {
    pub fxaa: Option<()>,         // to be filled out with fxaa configuration options
    pub attempt_gpu_driven: bool, // Will attempt to drive rendering, culling, etc on gpu if supported by the adapter
}

#[repr(C, align(16))]
struct MolecularVertexConsts {
    // Note: Each vertex is padded to 16 bytes to comply with WGSL layout
    // rules for uniform variables.  This means that each Vec2<f32> is
    // actually stored as a Vec4<f32> in the shader, with the last two entries
    // as padding.
    array: [Vec2; 6],
}
impl MolecularVertexConsts {
    fn new(a: Vec2, b: Vec2, c: Vec2) -> Self {
        let pad = Vec2::zero();
        Self {
            array: [a, pad, b, pad, c, pad],
        }
    }
}

#[allow(dead_code)]
pub struct Renderer {
    surface_config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface,
    render_resources: Rc<GlobalRenderResources>,
    size: PhysicalSize<u32>,

    vertex_contants: MolecularVertexConsts,
    vertex_contants_buffer: wgpu::Buffer,
    periodic_table: PeriodicTable,
    periodic_table_buffer: wgpu::Buffer,
    camera: RenderCamera,

    molecular_pass: passes::MolecularPass,
    fxaa_pass: passes::FxaaPass,
    blit_pass: passes::BlitPass,

    fragment_transforms: BufferVec<(), ultraviolet::Mat4>,

    gpu_driven_rendering: bool,
    options: RenderOptions,
}

impl Renderer {
    pub async fn new(window: &Window, options: RenderOptions) -> (Self, Rc<GlobalRenderResources>) {
        let size = window.inner_size();

        // The instance is a handle to our GPU.
        // Backends::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY | wgpu::Backends::GL,
            dx12_shader_compiler: Default::default(),
        });

        // # Safety
        //
        // The surface needs to live as long as the window that created it.
        let surface = unsafe { instance.create_surface(window) }
            .expect("failed to retrieve surface for window");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: !options.attempt_gpu_driven,
            })
            .await
            .expect("failed to find an appropriate adapter");

        let software_driven_features = wgpu::Features::empty();
        let gpu_driven_features = software_driven_features
            | wgpu::Features::VERTEX_WRITABLE_STORAGE
            | wgpu::Features::MULTI_DRAW_INDIRECT_COUNT;
        let gpu_driven_rendering;

        let requested_features =
            if options.attempt_gpu_driven && adapter.features().contains(gpu_driven_features) {
                // we can do culling and draw calls directly on gpu
                // Hopefully massive performance boost
                gpu_driven_rendering = true;
                gpu_driven_features
            } else {
                gpu_driven_rendering = false;
                software_driven_features
            };

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: requested_features,
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web we'll have to disable some.
                    limits: if cfg!(target_family = "wasm") {
                        wgpu::Limits::downlevel_webgl2_defaults()
                    } else {
                        wgpu::Limits::default()
                    },
                },
                None,
            )
            .await
            .expect("failed to create device");

        let camera = RenderCamera::new_empty(&device, 0.7, 0.1);

        let periodic_table = PeriodicTable::new();

        let vertex_contants = MolecularVertexConsts::new(
            Vec2::new(1.73, -1.0),
            Vec2::new(-1.73, -1.0),
            Vec2::new(0.0, 2.0),
        );
        let vertex_contants_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: vertex_contants.array.as_bytes(),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let periodic_table_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: periodic_table.element_reprs.as_bytes(),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: SWAPCHAIN_FORMAT,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![SWAPCHAIN_FORMAT],
        };

        surface.configure(&device, &surface_config);

        let atom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    // AtomPos
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    // AtomKind
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let bond_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    // Bond atom 1
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    // Bond atom 2
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    // Bond order
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        let render_resources = Rc::new(GlobalRenderResources {
            device,
            queue,
            atom_bgl,
            bond_bgl,
            linear_sampler,
        });

        let fragment_transforms =
            BufferVec::new(&render_resources.device, wgpu::BufferUsages::VERTEX, ());

        let (molecular_pass, color_texture) = passes::MolecularPass::new(
            &render_resources,
            camera.as_binding_resource(),
            &vertex_contants_buffer,
            &periodic_table_buffer,
            size,
        );
        let (fxaa_pass, fxaa_texture) =
            passes::FxaaPass::new(&render_resources, size, &color_texture);
        let blit_pass = passes::BlitPass::new(&render_resources, &fxaa_texture);

        (
            Self {
                surface_config,
                surface,
                render_resources: Rc::clone(&render_resources),
                size,

                vertex_contants,
                vertex_contants_buffer,
                periodic_table,
                periodic_table_buffer,
                camera,

                molecular_pass,
                fxaa_pass,
                blit_pass,

                fragment_transforms,

                gpu_driven_rendering,
                options,
            },
            render_resources,
        )
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;

        self.surface
            .configure(&self.render_resources.device, &self.surface_config);

        let (color_texture, _normals_texture) =
            self.molecular_pass.update(&self.render_resources, new_size);
        let fxaa_texture = self
            .fxaa_pass
            .update(&self.render_resources, color_texture, new_size);
        self.blit_pass.update(&self.render_resources, fxaa_texture);

        self.camera.resize(new_size);
    }

    pub fn upload_transforms(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        transforms: Vec<ultraviolet::Mat4>,
    ) {
        self.fragment_transforms.clear();
        let _ =
            self.fragment_transforms
                .push_small(&self.render_resources, encoder, &transforms[..]);
    }

    pub fn render<'a>(
        &mut self,
        atoms: &[&'a AtomBuffer],
        bonds: &[Option<&'a BondBuffer>],
        transforms: Vec<ultraviolet::Mat4>,
    ) {
        let mut encoder = self
            .render_resources
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        self.camera.update(common::InputEvent::BeginningFrame);
        if !self.camera.upload(&self.render_resources.queue) {
            log::warn!("no camera is set");
            // no camera is set, so no reason to do rendering.
            return;
        }

        self.upload_transforms(&mut encoder, transforms);
        // self.upload_new_transforms(&mut encoder, world);
        // self.update_transforms(&mut encoder, world);

        let frame = self
            .surface
            .get_current_texture()
            .map(|mut frame| {
                if frame.suboptimal {
                    // try again
                    frame = self
                        .surface
                        .get_current_texture()
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
            atoms,
            bonds,
            self.fragment_transforms.inner_buffer(),
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

        // run fxaa pass
        self.fxaa_pass.run(&mut encoder);

        // blit to screen
        self.blit_pass.run(
            &mut encoder,
            &frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );

        self.render_resources.queue.submit(Some(encoder.finish()));
        frame.present();
    }

    // pub fn render(
    //     &mut self,
    //     world: &mut World,
    //     #[allow(unused_variables)] interactions: &Interactions,
    // ) {
    //     let mut encoder = self
    //         .render_resources
    //         .device
    //         .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    //     self.camera.update(common::InputEvent::BeginningFrame);
    //     if !self.camera.upload(&self.render_resources.queue) {
    //         log::warn!("no camera is set");
    //         // no camera is set, so no reason to do rendering.
    //         return;
    //     }

    //     self.upload_new_transforms(&mut encoder, world);
    //     self.update_transforms(&mut encoder, world);

    //     let frame = self
    //         .surface
    //         .get_current_texture()
    //         .map(|mut frame| {
    //             if frame.suboptimal {
    //                 // try again
    //                 frame = self
    //                     .surface
    //                     .get_current_texture()
    //                     .expect("could not retrieve swapchain on second try");
    //                 if frame.suboptimal {
    //                     log::warn!("suboptimal swapchain frame");
    //                 }
    //             }
    //             frame
    //         })
    //         .expect("failed to get next swapchain");

    //     self.molecular_pass.run(
    //         &mut encoder,
    //         world.fragments(),
    //         self.fragment_transforms.inner_buffer(),
    //         &self.per_fragment,
    //     );

    //     // if interactions.selected_fragments.len() != 0 {
    //     //     log::warn!("trying to render to stencil");
    //     //     // currently broken
    //     //     self.render_fragments_to_stencil(
    //     //         world,
    //     //         &mut encoder,
    //     //         interactions.selected_fragments.iter().copied(),
    //     //     );
    //     // }

    //     // run fxaa pass
    //     self.fxaa_pass.run(&mut encoder);

    //     // blit to screen
    //     self.blit_pass.run(
    //         &mut encoder,
    //         &frame
    //             .texture
    //             .create_view(&wgpu::TextureViewDescriptor::default()),
    //     );

    //     self.render_resources.queue.submit(Some(encoder.finish()));
    //     frame.present();
    // }

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
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[format],
        })
    }
}

// End of File
