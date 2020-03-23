use winit::{
    event::{self, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    dpi::{PhysicalSize, PhysicalPosition},
    window::Window,
};
use std::{
    sync::mpsc::{channel, Sender, Receiver},
    thread,
    mem,
    time::Instant,
};
use anyhow::{Result, Context};
use tokio::runtime::Runtime;
use crate::init;

mod core;
mod worker;

pub use self::core::Gpu;

#[derive(Debug)]
pub enum MouseInteractions {
    Click(PhysicalPosition<u32>),
    Drag {
        start: PhysicalPosition<u32>,
        end: PhysicalPosition<u32>,
    },
}

#[derive(Debug, Default)]
pub struct RendererEvents {
    pub resize: Option<PhysicalSize<u32>>,
    pub mouse: Option<MouseInteractions>,
}

impl RendererEvents {
    pub fn take(&mut self) -> Self {
        mem::replace(self, Self::default())
    }
}

enum RendererMsg {
    Events(RendererEvents),
    Shutdown,
}

pub struct GpuHandle {
    to_gpu_main: Sender<RendererMsg>,
    // Orchestrates the other gpu threads
    // and runs asyncronous machinery.
    gpu_main: Option<thread::JoinHandle<()>>,
}

impl GpuHandle {
    pub fn send(&self, events: RendererEvents) -> Result<()> {
        self.to_gpu_main.send(RendererMsg::Events(events))
            .context("The main gpu thread has already closed")
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(handle) = self.gpu_main.take() {
            self.to_gpu_main.send(RendererMsg::Shutdown)?;
            handle.join()
                .map_err(|_| anyhow::Error::msg("The gpu main thread panicked at some point"))
        } else {
            Err(anyhow::Error::msg("The main gpu thread has already been shutdown"))
        }
    }
}

impl Gpu {
    pub fn spawn(window: &Window, surface: wgpu::Surface) -> Result<GpuHandle> {
        let mut fleeting_runtime = Runtime::new()?;

        let (device, queue) = fleeting_runtime.block_on(init::initialize_gpu())
            .context("Failed to initialize gpu")?;

        let window_size = window.inner_size();

        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: window_size.width,
            height: window_size.height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let (to_gpu_main, from_main) = channel();

        let handle = thread::Builder::new().name("gpu main".to_string()).spawn(move || {
            core::gpu_thread(
                surface,
                device,
                queue,
                sc_desc,
                from_main,
            ).unwrap();
        }).context("Failed to spawn main gpu thread")?;

        Ok(GpuHandle {
            to_gpu_main,
            gpu_main: Some(handle),
        })
    }
}