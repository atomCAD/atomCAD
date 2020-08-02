// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use anyhow::{Context, Result};
use futures::executor::LocalPool;
use std::{sync::Arc, thread};
use winit::dpi::PhysicalSize;

use crate::{
    most_recent::{self, Receiver, RecvError, Sender},
    scene::{Scene, DEFAULT_FORMAT},
    EncoderWrapper, Resize, SceneEvent, Tripper,
};

#[derive(Default)]
struct EventMsg {
    pub resize: Option<Resize>,
    pub events: Vec<SceneEvent>,
}

enum Msg {
    Events(EventMsg),
    Exit,
}

pub struct SceneHandle {
    input_tx: Sender<Msg>,
    output_rx: Receiver<Result<(wgpu::CommandBuffer, Tripper)>>,
    scene_thread: Option<thread::JoinHandle<()>>,
}

impl SceneHandle {
    /// Spawn the scene thread and return a handle to it, as well as the first texture view.
    pub fn create_scene(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        size: PhysicalSize<u32>,
    ) -> (SceneHandle, wgpu::TextureView) {
        let mut scene = Scene::new(&device, size);

        let (input_tx, input_rx) = most_recent::channel();
        let (output_tx, output_rx) = most_recent::channel();

        let texture_view = scene.render_texture.create_default_view();

        let scene_thread = thread::spawn(move || {
            let mut local_pool = LocalPool::new();
            let spawner = local_pool.spawner();

            loop {
                let events = match input_rx.recv() {
                    Ok(Msg::Events(events)) => events,
                    Ok(Msg::Exit) // the sending side has requested the scene thread to shut down.
                    | Err(RecvError) // The sending side has disconnected, time to shut down.
                        => break,
                };

                let mut cmd_encoder = EncoderWrapper::new(&device);

                match scene.render_frame(
                    &device,
                    &queue,
                    &mut cmd_encoder,
                    events.events,
                    events.resize,
                    &spawner,
                ) {
                    Ok(_) => {
                        let (cmd_encoder, tripper) = cmd_encoder.inner();
                        let cmd_buffer = cmd_encoder.finish();
                        output_tx
                            .send(Ok((cmd_buffer, tripper)))
                            .expect("unable to send command buffer to main thread");
                    }
                    Err(e) => output_tx
                        .send(Err(e))
                        .expect("unable to send error to main thread"),
                }

                local_pool.run_until_stalled();
            }

            log::info!("scene thread is shutting down");
        });

        let scene_handle = SceneHandle {
            input_tx,
            output_rx,
            scene_thread: Some(scene_thread),
        };

        (scene_handle, texture_view)
    }

    /// Send a collection of events to the scene thread.
    ///
    /// This returns a new texture view in the case that the window resized.
    pub fn apply_events(
        &mut self,
        device: &wgpu::Device,
        events: Vec<SceneEvent>,
        new_size: Option<PhysicalSize<u32>>,
    ) -> Result<Option<(PhysicalSize<u32>, wgpu::TextureView)>> {
        let new_texture = new_size.map(|size| (size, build_render_texture(device, size)));
        let new_texture_view = new_texture
            .as_ref()
            .map(|(size, texture)| (*size, texture.create_default_view()));

        let resize = new_texture.map(|(size, new_texture)| Resize { new_texture, size });

        self.input_tx
            .send(Msg::Events(EventMsg { resize, events }))
            .context("failed to send item to scene thread")?;

        Ok(new_texture_view)
    }

    pub fn recv_cmd_buffer(&mut self) -> Result<(wgpu::CommandBuffer, Tripper)> {
        // self.output_rx.try_recv()
        //     .context("didn't retrieve the command buffer from the scene thread in time")?
        //     .context("the scene thread reported an error")
        self.output_rx
            .recv()
            .context("unable to retrieve a command buffer from the scene thread")?
            .context("the scene thread reported an error")
    }
}

impl Drop for SceneHandle {
    fn drop(&mut self) {
        self.input_tx.send(Msg::Exit).unwrap();
        self.scene_thread.take().unwrap().join().unwrap();
    }
}

fn build_render_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEFAULT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
        label: if cfg!(build = "debug") {
            Some("scene render texture")
        } else {
            None
        },
    })
}
