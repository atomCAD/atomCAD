use crate::{
    gpu::{
        RendererMsg,
        RendererEvents,
        worker::Worker,
    },
    fps::{Fps, FpsGet},
};
use std::{
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};
use anyhow::{Result, Context};


const TARGET_FPS: usize = 60;

pub struct Gpu {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,

    sc_desc: wgpu::SwapChainDescriptor,
    swapchain: wgpu::SwapChain,

    from_main: Receiver<RendererMsg>,

    workers: Vec<Worker>,
}

pub(super) fn gpu_thread(
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    sc_desc: wgpu::SwapChainDescriptor,
    from_main: Receiver<RendererMsg>,
) -> Result<()> {
    let swapchain = device.create_swap_chain(&surface, &sc_desc);
    let mut gpu = Gpu {
        surface,
        device,
        queue,
        sc_desc,
        swapchain,
        from_main,
        workers: vec![],
    };

    let (mut fps, fps_get) = Fps::create();
    let mut events = Vec::new();

    gpu.workers.push(Worker::spawn(|local_set| {
        local_set.spawn_local(
            fps_logging(fps_get)
        );
    })?);

    loop {
        let swapchain_output = gpu.swapchain.get_next_texture()
            .map_err(|_| anyhow!("Unable to get the next swapchain output"))?;

        // println!("Just got a new swapchain output");
        // Tell the fps counter that we just got a new frame.
        fps.tick();
        
        if let ReceivedEvents::Shutdown = recieve_events(&gpu, &mut events)? {
            return Ok(());
        }

        // Do something :P
        drop(swapchain_output);

        let encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });
        gpu.queue.submit(&[encoder.finish()]);

        events.clear();
    }
}

async fn fps_logging(fps: FpsGet) {
    use tokio::stream::StreamExt;

    let mut interval = tokio::time::interval_at(
        (Instant::now() + Duration::from_secs(1)).into(),
        Duration::from_secs(3),
    );

    while let Some(_time) = interval.next().await {
        info!("FPS is currently {} frames per second", fps.get());
    }
}

enum ReceivedEvents {
    Ok,
    Shutdown,
}

fn recieve_events(gpu: &Gpu, buffer: &mut Vec<RendererEvents>) -> Result<ReceivedEvents> {
    loop {
        let event = match gpu.from_main.try_recv() {
            Ok(msg) => match msg {
                RendererMsg::Events(events) => events,
                RendererMsg::Shutdown => {
                    info!("Shutting down main gpu thread");
                    return Ok(ReceivedEvents::Shutdown);
                },
            }
            Err(TryRecvError::Empty) => return Ok(ReceivedEvents::Ok),
            Err(TryRecvError::Disconnected) => {
                // Sender has disconnected,
                // so shut down this thread and log that
                // something went bad on the main thread.
                error!("The main thread disconnected from the gpu thread without warning.");
                return Err(anyhow!("The main thread disconnected from the gpu thread without warning."));
            }
        };

        buffer.push(event);
    }
}