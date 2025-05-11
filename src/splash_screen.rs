// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use gui::window::WindowManager;
use render::{RenderContext, RenderContextError};
use winit::{event::WindowEvent, event_loop::ActiveEventLoop, window::WindowId};

/// The SplashScreen is a special type of window that is shown when the application is first
/// started.  It represents the entry point into the program's UI/UX, and is responsible for
/// initializing the application state and resources.  It is typically used to show a loading
/// screen, a splash screen, or a login screen.  In document/workspace-oriented user interfaces, the
/// SplashScreen also provides a way to open or create new workspaces in their own Workspace's.
pub struct SplashScreen {
    title: String,
    blueprint: Option<menu::Blueprint>,
    render_context: Option<RenderContext>,
    running: bool,
}

impl SplashScreen {
    pub fn new(title: String, blueprint: Option<menu::Blueprint>) -> Self {
        Self {
            title,
            blueprint,
            render_context: None,
            running: false,
        }
    }

    pub fn render(&mut self) -> Result<(), RenderContextError> {
        if self.render_context.is_none() {
            return Err(RenderContextError::NoRenderContext);
        }
        let rc = self.render_context.as_mut().unwrap();
        let frame = rc.frame_encoder(Some("SplashScreen encoder"))?;

        // Clear the buffer to a solid color
        let (encoder, view) = frame.encoder_view_mut();
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SplashScreen::render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Present the frame
        rc.present_frame();

        Ok(())
    }
}

impl WindowManager for SplashScreen {
    fn set_title(&mut self, title: String) {
        self.title = title;
        if let Some(rc) = self.render_context.as_ref() {
            rc.window().set_title(&self.title);
        }
    }

    fn get_title(&self) -> &str {
        &self.title
    }

    fn get_window_id(&self) -> Option<WindowId> {
        self.render_context.as_ref().map(|rc| rc.window().id())
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, event: &WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => match self.render() {
                Ok(_) => {}
                Err(error) => {
                    log::error!("Failed to render: {error}");
                }
            },
            WindowEvent::Resized(requested_size) => {
                if let Some(rc) = self.render_context.as_mut() {
                    rc.resize_surface(*requested_size);
                }
            }
            _ => {}
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.render_context.is_none() {
            match WindowManager::create(self, event_loop) {
                Err(error) => {
                    log::error!("Failed to create window: {error}");
                    return;
                }
                Ok(window) => {
                    let render_context = match RenderContext::new(window) {
                        Ok(render_context) => render_context,
                        Err(error) => {
                            log::error!("Failed to create render context: {error}");
                            return;
                        }
                    };
                    self.render_context = Some(render_context);
                }
            };

            if let (Some(blueprint), Some(render_context)) =
                (self.blueprint.as_ref(), self.render_context.as_ref())
            {
                menu::attach_menubar_to_window(render_context.window(), blueprint);
            }
        }

        self.running = true;
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        // Currently unused.
        let _ = event_loop;
        // Destroy the window (it will be recreated when resumed).
        self.running = false;
        self.render_context = None;
    }
}

// End of File
