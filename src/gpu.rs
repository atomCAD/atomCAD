use winit::{
    event::{self, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    dpi::{PhysicalSize, PhysicalPosition},
    window::Window,
};
use std::{
    sync::mpsc::{Sender, Receiver},
    thread,
    mem,
};
use anyhow::{Result, Context};
use async_std::task;
use crate::init;

#[derive(Debug)]
pub enum MouseInteraction {
    Click(PhysicalPosition<u32>),
    Drag {
        start: PhysicalPosition<u32>,
        end: PhysicalPosition<u32>,
    },
}

#[derive(Debug)]
pub enum RendererEvents {
    Render {
        resize: Option<PhysicalSize<u32>>,
        mouse: Option<MouseInteraction>,
    },
    Empty,
    Shutdown,
}

impl RendererEvents {
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if let RendererEvents::Render { resize, .. } = self {
            *resize = Some(new_size);
        } else {
            *self = RendererEvents::Render {
                resize: Some(new_size),
                mouse: None,
            };
        }
    }

    pub fn shutdown(&mut self) {
        *self = RendererEvents::Shutdown;
    }

    // Sets self to `RendererEvents::Empty` and returns original value.
    pub fn take(&mut self) -> Self {
        mem::replace(self, RendererEvents::Empty)
    }
}

pub struct GpuHandle {
    tx: Sender<RendererEvents>,
}

impl GpuHandle {
    pub fn send(&self, events: RendererEvents) -> Result<()> {
        self.tx.send(events)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Gpu {
    swap_chain: wgpu::SwapChain,
    device: wgpu::Device,
    queue: wgpu::Queue,
    gpu_thread: thread::JoinHandle<()>,
}


impl Gpu {
    pub fn spawn(window: &Window, surface: &wgpu::Surface) -> Result<GpuHandle> {
        let (device, queue) = task::block_on(init::initialize_gpu())
            .context("Failed to initialize gpu")?;

        let window_size = window.inner_size();

        let mut sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: window_size.width,
            height: window_size.height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let swap_chain = device.create_swap_chain(surface, &sc_desc);

        let handle = thread::spawn(|| {
            
        });

        unimplemented!()
    }
}