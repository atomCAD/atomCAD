use winit::{
    event_loop::EventLoop,
    window::Window,
};
use anyhow::{Result, Context};

#[derive(Debug, thiserror::Error)]
pub enum InitErr {
    #[error("unable to request an adapter (options: {options:?}, backends: {backends:?})")]
    BadRequest {
        options: wgpu::RequestAdapterOptions,
        backends: wgpu::BackendBit,
    },
}

pub async fn initialize_gpu() -> Result<(wgpu::Device, wgpu::Queue)> {
    // Request a display adapter.
    let adapter = {
        let options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
        };
        let backends = wgpu::BackendBit::PRIMARY;
        
        wgpu::Adapter::request(
            &options,
            backends,
        )
        .await
        .context(InitErr::BadRequest { options, backends })?
    };

    info!("Successfully requested adapter");

    // Request the actual gpu.
    let device_and_queue = adapter.request_device(
        &wgpu::DeviceDescriptor {
            extensions: wgpu::Extensions {
                anisotropic_filtering: false,
            },
            limits: wgpu::Limits::default(),
        }
    ).await;

    info!("Successfully requested GPU");

    Ok(device_and_queue)
}

/// Create an event loop and window.
/// This may take some time to run.
pub fn initialize_display() -> Result<(EventLoop<()>, Window, wgpu::Surface)> {
    let event_loop = EventLoop::new();
    let window = Window::new(&event_loop)
        .context("Unable to create a window")?;
    
    let surface = wgpu::Surface::create(&window);

    Ok((event_loop, window, surface))
}
