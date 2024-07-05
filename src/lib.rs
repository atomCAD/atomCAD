// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This is the main application crate for atomCAD.  It contains the main
//! windowing event loop, implementations of user interface elements and
//! associated application logic, and the platform-specific code for
//! initializing the application and handling events.  It also contains a fair
//! amount of other functionality that has not yet been moved into separate
//! crates.
//!
//! atomCAD is implemented as a single-window application, with a 3D view
//! showing the molecular parts and aseemblies being edited, and an overlay of
//! various tool widgets optimized for multi-touch interfaces.  The 3D view is
//! implemented using the [wgpu] crate, and the overlay is implemented using
//! [rui].  Native APIs are used whenever possible for other required user
//! interface elements.
//!
//! As of this writing, the application is still in the early stages of
//! development, and is not yet usable for any practical purpose.  The
//! following features are currently implemented:
//!
//! * A basic 3D view, with a camera that can be controlled using the mouse
//!   and keyboard.
//!
//! * A basic menu bar, with a File menu that can be used to open PDB files.
//!
//! As is common with binary applications, the main entry point is in the
//! `main.rs` file, and the rest of the application is implemented in this
//! crate, so that it is accessible to integration tests.
//!
//! [wgpu]: https://crates.io/crates/wgpu
//! [rui]: https://crates.io/crates/rui

/// The API for controlling the camera in the 3D view, and having it respond
/// to user events.
pub mod camera;
/// A platform-independent abstraction over the windowing system's interface
/// for menus and menubars.  Used to setup the application menubar on startup.
pub mod menubar;

// This module is not public.  It is a common abstraction over the various
// platform-specific APIs.  For example, `platform::menubar` exposes an API
// for taking a platform-independent `menubar::Menu` and instantiating it in
// the windowing system and attaching it to either the window or application
// object, as required.
//
// The APIs exposed by this module are meant to be called from the rest of the
// `atomCAD` crate.
pub(crate) mod platform;
// This module contains the platform-specific native API code used by
// `platform`.  It is not intended to be used directly by any other code.  In
// the future it may be moved to be a private submodule of `platform`.
pub(crate) mod platform_impl;

/// The user-visible name of the application, used for window titles and such.
pub const APP_NAME: &str = "atomCAD";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_LICENSE: &str = env!("CARGO_PKG_LICENSE");

use camera::ArcballCamera;
use common::InputEvent;
use menubar::Menu;
use molecule::{
    edit::{Edit, PdbData},
    MoleculeEditor,
};
use render::{GlobalRenderResources, Interactions, RenderOptions, Renderer};
use scene::{Assembly, Component};

use std::rc::Rc;
use std::sync::Arc;
use ultraviolet::{Mat4, Vec3};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceEvent, DeviceId, ElementState, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopBuilder},
    keyboard::KeyCode,
    window::{Window, WindowId},
};

#[allow(dead_code)]
fn make_pdb_demo_scene() -> MoleculeEditor {
    MoleculeEditor::from_feature(Edit::PdbImport(PdbData {
        name: "Neon Pump".into(),
        contents: include_str!("../assets/neon_pump_imm.pdb").into(),
    }))
}

#[allow(dead_code)]
fn make_salt_demo_scene() -> MoleculeEditor {
    let mut molecule =
        MoleculeEditor::from_feature(Edit::RootAtom(periodic_table::Element::Sodium));

    molecule.insert_edit(Edit::BondedAtom(molecule::edit::BondedAtom {
        target: common::ids::AtomSpecifier::new(0),
        element: periodic_table::Element::Chlorine,
    }));

    molecule.apply_all_edits();
    molecule
}

#[derive(Default)]
struct EventHandler {
    running: bool,
    menu: Menu,
    // On mobile platforms the window is destroyed when the application is
    // suspended, so we need to be able to drop these resources and recreate
    // as necessary.
    renderer: Option<Renderer>,
    gpu_resources: Option<Rc<GlobalRenderResources>>,
    world: Option<Assembly>,
    interactions: Option<Interactions>,
    cursor_pos: PhysicalPosition<f64>,
    // The event handling loop is terminated when the main window is closed.
    // We can trigger this by dropping the window, so we wrap it in the Option
    // type.  This is a bit of a hack, but it works.
    window: Option<Arc<Window>>,
}

impl EventHandler {
    fn new(menu: Menu) -> Self {
        Self {
            menu,
            ..Default::default()
        }
    }

    fn resume_renderer(&mut self) {
        if let Some(window) = &self.window {
            let (r, g, w, i) = futures::executor::block_on(async {
                let (mut renderer, gpu_resources) = Renderer::new(
                    window.clone(),
                    RenderOptions {
                        fxaa: Some(()), // placeholder
                        attempt_gpu_driven: true,
                    },
                )
                .await;
                renderer.set_camera(ArcballCamera::new(Vec3::zero(), 100.0, 1.0));
                let world = Assembly::from_components([Component::from_molecule(
                    make_pdb_demo_scene(),
                    Mat4::default(),
                )]);
                let interactions = Interactions::default();
                (renderer, gpu_resources, world, interactions)
            });
            self.renderer = Some(r);
            self.gpu_resources = Some(g);
            self.world = Some(w);
            self.interactions = Some(i);
        }
    }
}

impl ApplicationHandler for EventHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Called when the application is brought into focus.  The GPU
        // resources need to be reallocated on resume.

        // On some platforms, namely wasm32 + webgl2, the window is not yet
        // ready to create the rendering surface when Event::Resumed is
        // received.  We therefore just record the fact that the we're in the
        // running state.
        self.running = true;

        // Create the main window.
        let window_attributes = Window::default_attributes().with_title(APP_NAME);
        self.window = match event_loop.create_window(window_attributes) {
            Err(e) => {
                println!("Failed to create window: {}", e);
                std::process::exit(1);
            }
            Ok(window) => Some(Arc::new(window)),
        };

        // Perform window customization required on web.
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = &mut self.window {
            // Winit prevents sizing with CSS, so we have to set
            // the size manually when on web.
            use winit::dpi::PhysicalSize;
            let width = web_sys::window()
                .and_then(|win| win.inner_width().ok())
                .and_then(|w| w.as_f64())
                .unwrap_or(800.0);
            let height = web_sys::window()
                .and_then(|win| win.inner_height().ok())
                .and_then(|h| h.as_f64())
                .unwrap_or(600.0);
            let scale_factor = window.scale_factor();
            let _ = window.request_inner_size(PhysicalSize::new(
                width * scale_factor,
                height * scale_factor,
            ));
            // On wasm, append the canvas to the document body
            use winit::platform::web::WindowExtWebSys;
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.get_element_by_id("app-container"))
                .and_then(|dst| {
                    dst.append_child(&web_sys::Element::from(window.canvas()?))
                        .ok()
                })
                .expect("Couldn't append canvas to document body.");
        }

        // Add the menu bar to the window / application instance, using native
        // APIs.
        menubar::attach_menu_bar(self.window.as_ref().unwrap(), &self.menu);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // When we are done handling this event, suspend until the next event.
        event_loop.set_control_flow(ControlFlow::Wait);

        // The event system does not expose the cursor position on-demand. We
        // track all the mouse movement events to make this easier to access
        // later.
        if let WindowEvent::CursorMoved { position, .. } = event {
            self.cursor_pos = position
        }

        // Check that we've received Event::Resumed, and the window's inner
        // dimensions are defined.  (Prevents a panic on wasm32 + webgl2).
        if self.running && self.renderer.is_none() {
            if let Some(window) = &self.window {
                let size = window.inner_size();
                if size.width > 0 && size.height > 0 {
                    self.resume_renderer();
                }
            }
        }

        // Handle events.
        match event {
            WindowEvent::Resized(new_size) => {
                // TODO: Remove this once we upgrade winit to a version with the fix
                #[cfg(target_os = "macos")]
                if new_size.width == u32::MAX || new_size.height == u32::MAX {
                    // HACK to fix a bug on Macos 14
                    // https://github.com/rust-windowing/winit/issues/2876
                    return;
                }

                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size);
                }
            }
            WindowEvent::CloseRequested => {
                // The user has requested to close the window.
                // Drop the window to fire the `Destroyed` event.
                self.renderer = None;
                self.window = None;
            }
            WindowEvent::Destroyed => {
                // The window has been destroyed, time to exit stage left.
                event_loop.exit();
            }
            _ => {
                if let Some(renderer) = &mut self.renderer {
                    match event {
                        WindowEvent::KeyboardInput { event: key, .. } => {
                            if key.physical_key == KeyCode::Space
                                && key.state == ElementState::Released
                            {
                                if let Some(window) = &mut self.window {
                                    match renderer
                                        .camera()
                                        .get_ray_from(&self.cursor_pos, &window.inner_size())
                                    {
                                        Some((ray_origin, ray_direction)) => {
                                            self.world.as_mut().unwrap().walk_mut(|molecule, _| {
                                                if let Some(hit) = molecule
                                                    .repr
                                                    .get_ray_hit(ray_origin, ray_direction)
                                                {
                                                    println!("Atom {:?} clicked!", hit);
                                                    // molecule.push_feature(AtomFeature {
                                                    //     target: hit,
                                                    //     element: periodic_table::Element::Carbon,
                                                    // });
                                                    // molecule.apply_all_features();
                                                    // molecule.reupload_atoms(
                                                    //     gpu_resources.as_ref().unwrap(),
                                                    // );
                                                }
                                            });
                                        }
                                        None => {
                                            println!("failed to create ray!");
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            renderer.camera().update(InputEvent::Window(event));
                        }
                    }
                }
            }
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: StartCause) {
        // Will be called once when the event loop starts.
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {}

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(renderer) = &mut self.renderer {
            renderer.camera().update(InputEvent::Device(event));
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // The event queue is empty, so we can safely redraw the window.
        if self.window.is_some() {
            // Winit prevents sizing with CSS, so we have to set
            // the size manually when on web.
            #[cfg(target_arch = "wasm32")]
            (|| {
                use winit::dpi::PhysicalSize;
                log::error!("Resizing window");
                let win = web_sys::window()?;
                let width = win.inner_width().ok()?.as_f64()?;
                let height = win.inner_height().ok()?.as_f64()?;
                self.window.as_ref().map(|window| {
                    let scale_factor = window.scale_factor();
                    let new_size = PhysicalSize::new(
                        (width * scale_factor) as u32,
                        (height * scale_factor) as u32,
                    );
                    window.request_inner_size(new_size)?;
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(new_size);
                    }
                    Some(())
                })
            })();
            if let Some(renderer) = &mut self.renderer {
                if let Some(world) = &mut self.world {
                    if let Some(_interactions) = &mut self.interactions {
                        if let Some(gpu_resources) = &mut self.gpu_resources {
                            world.synchronize_buffers(&*gpu_resources);
                        }
                        let (atoms, bonds, transforms) = world.collect_rendering_primitives();
                        renderer.render(&atoms, &bonds, transforms);
                    }
                }
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Called on iOS, Android, and web when the application / browser tab
        // is sent to the background.  We preemptively destroy the window and
        // any used GPU resources as the system might take them from us.
        self.running = false;
        self.interactions = None;
        self.world = None;
        self.gpu_resources = None;
        self.renderer = None;
        self.window = None;
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // The event loop has been destroyed, so we can safely terminate
        // the application.  This is the very last event we will ever
        // receive, so we can safely perform final rites.
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        // On Android and iOS, called when the device has run out of memory
        // and the application is about to be killed.  We must free as much
        // memory as possible or risk being terminated.
    }
}

fn run(event_loop: EventLoop<()>, menu: Menu) {
    // Run the event loop.
    let mut event_handler = EventHandler::new(menu);
    if let Err(e) = event_loop.run_app(&mut event_handler) {
        eprintln!("Error during event loop: {}", e);
    };
}

pub fn start(event_loop_builder: &mut EventLoopBuilder<()>) {
    let menu = menubar::setup_menu_bar(event_loop_builder);
    let event_loop = match event_loop_builder.build() {
        Err(e) => {
            println!("Failed to create event loop: {}", e);
            std::process::exit(1);
        }
        Ok(event_loop) => event_loop,
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        #[cfg(not(target_os = "android"))]
        {
            env_logger::init();
        }
        #[cfg(target_os = "android")]
        {
            android_logger::init_once(
                android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
            );
        }
        run(event_loop, menu);
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        run(event_loop, menu);
    }
}

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    start(EventLoopBuilder::with_user_event().with_android_app(app))
}

// End of File
