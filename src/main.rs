#[macro_use]
extern crate static_assertions;

mod parts;
mod utils;
mod elements;
mod camera;
mod bind_groups;
mod atoms;

// use crate::elements::Element;
use crate::{
    utils::AsBytes as _,
    elements::Element,
    bind_groups::{BindGroupLayouts, AsBindingResource as _},
};

use std::{
    iter,
    mem,
    convert::TryInto as _,
};
use winit::{
    event::{Event, WindowEvent, DeviceEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
use wgpu::util::DeviceExt as _;

macro_rules! include_spirv {
    ($name:literal) => {
        wgpu::include_spirv!(concat!(env!("OUT_DIR"), "/shaders/", $name))
    };
}

pub enum InputEvent<'a> {
    Window(WindowEvent<'a>),
    Device(DeviceEvent),
}

async fn run(event_loop: EventLoop<()>, window: Window, swapchain_format: wgpu::TextureFormat) {
    let size = window.inner_size();
    let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
    let surface = unsafe { instance.create_surface(&window) };
    
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

    let mut camera = camera::Camera::new(&device, size, 0.7, 0.1);

    let parts = parts::Part::load_from_pdb(&device, &bgl, "Neon Pump", "data/neon_pump_imm.pdb").unwrap();

    for part in &parts {
        println!("part center: {:?}", part.center());
    }

    let shader_runtime_config_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
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
                }
            },
        ]
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[
            &bgl.global,
            &bgl.atoms,
        ],
        push_constant_ranges: &[],
    });

    let billboard_vert_shader = device.create_shader_module(include_spirv!("billboard.vert"));
    let billboard_frag_shader = device.create_shader_module(include_spirv!("billboard.frag"));

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &billboard_vert_shader,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &billboard_frag_shader,
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
            vertex_buffers: &[
                wgpu::VertexBufferDescriptor {
                    stride: mem::size_of::<ultraviolet::Mat4>() as _,
                    step_mode: wgpu::InputStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        // part and fragment transform matrix
                        0 => Float4,
                        1 => Float4,
                        2 => Float4,
                        3 => Float4,
                    ],
                },
            ],
        },
        sample_count: 1, // TODO: Try multisampling?
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    });

    let mut sc_desc = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Mailbox,
    };

    let mut swap_chain = device.create_swap_chain(&surface, &sc_desc);
    let mut depth_texture = device.create_texture(&wgpu::TextureDescriptor {
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
    }).create_view(&wgpu::TextureViewDescriptor::default());

    event_loop.run(move |event, _, control_flow| {
        // Have the closure take ownership of the resources.
        // `event_loop.run` never returns, therefore we must do this to ensure
        // the resources are properly cleaned up.
        let _ = (
            &instance,
            &adapter,
            &billboard_vert_shader,
            &billboard_frag_shader,
            &pipeline_layout,
        );

        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                sc_desc.width = size.width;
                sc_desc.height = size.height;
                swap_chain = device.create_swap_chain(&surface, &sc_desc);

                depth_texture = device.create_texture(&wgpu::TextureDescriptor {
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
                }).create_view(&wgpu::TextureViewDescriptor::default());

                camera.update(InputEvent::Window(WindowEvent::Resized(size)));
            },
            Event::MainEventsCleared => {
                camera.finalize(&queue);

                let frame = swap_chain
                    .get_current_frame()
                    .expect("failed to acquire next swapchain texture");

                if frame.suboptimal {
                    log::warn!("suboptimal swapchain texture acquired");
                }

                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                // TODO: Upload all transformation matricies (maybe quats + offset in the future)?
                // Should I instead only send up a patch and have a compute shader rewrite the
                // correct matrices?
                
                // TODO: Also, add instancing for drawing multiple copies of a fragment. Maybe a part as well?

                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[
                            wgpu::RenderPassColorAttachmentDescriptor {
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
                            }
                        ],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                            attachment: &depth_texture,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(0.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        }),
                    });

                    rpass.set_pipeline(&render_pipeline);
                    rpass.set_bind_group(0, &global_bg, &[]);

                    for part in &parts {
                        // TODO: set vertex buffer to the right matrixes.

                        for fragment in part.fragments() {
                            rpass.set_bind_group(1, &fragment.atoms().bind_group(), &[]);
                            rpass.draw(0..(fragment.atoms().len() * 3).try_into().unwrap(), 0..1)
                        }
                    }
                }

                queue.submit(iter::once(encoder.finish()));
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::WindowEvent { event, .. } => {
                camera.update(InputEvent::Window(event));
            },
            Event::DeviceEvent { event, .. } => {
                camera.update(InputEvent::Device(event));
            }
            _ => {}
        }
    })
}

fn main() {
    let event_loop = EventLoop::new();
    let window = Window::new(&event_loop).unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    {
        subscriber::initialize_default_subscriber(None);
        futures::executor::block_on(run(event_loop, window, wgpu::TextureFormat::Bgra8UnormSrgb));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        use winit::platform::web::WindowExtWebSys;
        // On wasm, append the canvas to the document body
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body())
            .and_then(|body| {
                body.append_child(&web_sys::Element::from(window.canvas()))
                    .ok()
            })
            .expect("couldn't append canvas to document body");
        wasm_bindgen_futures::spawn_local(run(event_loop, window, wgpu::TextureFormat::Bgra8Unorm));
    }
}