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
/// A module for loading and parsing PDB files.
///
/// TODO: Should probably be abstracted into its own crate.
pub mod pdb;

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

use camera::ArcballCamera;
use common::InputEvent;
use periodic_table::Element;
use render::{
    AtomKind, AtomRepr, Fragment, GlobalRenderResources, Interactions, Part, RenderOptions,
    Renderer, World,
};

use scene::Molecule;

use std::sync::Arc;
use ultraviolet::Vec3;
use winit::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

async fn resume_renderer(
    window: &Window,
) -> (Renderer, Arc<GlobalRenderResources>, World, Interactions) {
    let (renderer, gpu_resources) = Renderer::new(
        window,
        RenderOptions {
            fxaa: Some(()), // placeholder
            attempt_gpu_driven: true,
        },
    )
    .await;

    let mut world = World::new();
    let fragment = Fragment::from_atoms(
        &gpu_resources,
        vec![
            AtomRepr {
                pos: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                kind: AtomKind::new(Element::Carbon),
            },
            AtomRepr {
                pos: Vec3 {
                    x: 5.0,
                    y: 0.0,
                    z: 0.0,
                },
                kind: AtomKind::new(Element::Sulfur),
            },
        ],
    );
    let mut part = Part::from_fragments(&mut world, "test part", vec![fragment]);
    // part.move_to(0.0, 0.0, 0.0);
    world.spawn_part(part);

    // The PDB parser lib3dmol does not parse connectivity information.
    // Because of this, we cannot build a molecule graph out of a PDB,
    // and so for now we use this hardcoded molecule to test the graph
    // implementation:

    // let mut neon_pump = pdb::load_from_pdb_str(
    //     &gpu_resources,
    //     "Neon Pump",
    //     include_str!("../assets/neon_pump_imm.pdb"),
    // )
    // .expect("failed to load pdb");
    // println!(
    //     "Loaded {} parts and {} fragments",
    //     neon_pump.parts().len(),
    //     neon_pump.fragments().len()
    // );
    // // Center the neon pump around the origin, so that the rotating arcball
    // // camera will be centered on it.
    // for part in neon_pump.parts_mut() {
    //     // This doesn't let the world know that this part is going to be
    //     // updated, but we're adding them for the first time, so it'll work
    //     // anyhow.
    //     part.move_to(0.0, 0.0, 0.0);
    // }
    // world.merge(neon_pump);

    let interactions = Interactions::default();

    (renderer, gpu_resources, world, interactions)
}

fn handle_event(
    event: Event<()>,
    control_flow: &mut ControlFlow,
    window: &mut Option<Window>,
    renderer: &mut Option<Renderer>,
    _gpu_resources: &mut Option<Arc<GlobalRenderResources>>,
    world: &mut Option<World>,
    interactions: &mut Option<Interactions>,
) {
    match event {
        Event::NewEvents(StartCause::Init) => {
            // Will be called once when the event loop starts.
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(new_size),
            ..
        } => {
            if let Some(renderer) = renderer {
                renderer.resize(new_size);
            }
        }
        Event::MainEventsCleared => {
            // The event queue is empty, so we can safely redraw the window.
            if window.is_some() {
                // Winit prevents sizing with CSS, so we have to set
                // the size manually when on web.
                #[cfg(target_arch = "wasm32")]
                (|| {
                    use winit::dpi::PhysicalSize;
                    log::error!("Resizing window");
                    let win = web_sys::window()?;
                    let width = win.inner_width().ok()?.as_f64()?;
                    let height = win.inner_height().ok()?.as_f64()?;
                    window.as_ref().map(|window| {
                        let scale_factor = window.scale_factor();
                        let new_size = PhysicalSize::new(
                            (width * scale_factor) as u32,
                            (height * scale_factor) as u32,
                        );
                        window.set_inner_size(new_size);
                        if let Some(renderer) = renderer {
                            renderer.resize(new_size);
                        }
                        Some(())
                    })
                })();
                if let Some(renderer) = renderer {
                    if let Some(world) = world {
                        if let Some(interactions) = interactions {
                            renderer.render(world, interactions);
                        }
                    }
                }
            }
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            // The user has requested to close the window.
            // Drop the window to fire the `Destroyed` event.
            *window = None;
        }
        Event::WindowEvent {
            event: WindowEvent::Destroyed,
            ..
        } => {
            // The window has been destroyed, time to exit stage left.
            *control_flow = ControlFlow::ExitWithCode(0);
        }
        Event::LoopDestroyed => {
            // The event loop has been destroyed, so we can safely terminate
            // the application.  This is the very last event we will ever
            // receive, so we can safely perform final rites.
        }
        Event::WindowEvent { event, .. } => {
            if let Some(renderer) = renderer {
                renderer.camera().update(InputEvent::Window(event));
            }
        }
        Event::DeviceEvent { event, .. } => {
            if let Some(renderer) = renderer {
                renderer.camera().update(InputEvent::Device(event));
            }
        }
        _ => {
            // Unknown event; do nothing.
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run(event_loop: EventLoop<()>, mut window: Option<Window>) {
    // The event handling loop is terminated when the main window is closed.
    // We can trigger this by dropping the window, so we wrap it in the Option
    // type.  This is a bit of a hack, but it works.  We require that we are
    // called with a valid window, however.
    window.as_ref().expect("window should exist");

    // On mobile platforms the window is destroyed when the application is
    // suspended, so we need to be able to drop these resources and recreate
    // as necessary.
    let mut renderer: Option<Renderer> = None;
    let mut gpu_resources: Option<Arc<GlobalRenderResources>> = None;
    let mut world: Option<World> = None;
    let mut interactions: Option<Interactions> = None;

    // Run the event loop.
    event_loop.run(move |event, _, control_flow| {
        // When we are done handling this event, suspend until the next event.
        *control_flow = ControlFlow::Wait;

        // Handle events.
        match event {
            Event::Resumed => {
                // Called on iOS or Android when the application is brought
                // into focus.  We must (re-)create the window and any GPU
                // resources, because they don't persist across application
                // suspensions.
                futures::executor::block_on(async {
                    let (mut r, g, w, i) = resume_renderer(window.as_ref().unwrap()).await;
                    r.set_camera(ArcballCamera::new(Vec3::zero(), 100.0, 1.0));
                    renderer = Some(r);
                    gpu_resources = Some(g);
                    world = Some(w);
                    interactions = Some(i);
                });
            }
            Event::Suspended => {
                // Called on iOS or Android when the application is sent to
                // the background.  We preemptively destroy the window and any
                // used GPU resources as the system might take them from us.
                interactions = None;
                world = None;
                gpu_resources = None;
                renderer = None;
                window = None;
            }
            _ => {
                // Process all other events.
                handle_event(
                    event,
                    control_flow,
                    &mut window,
                    &mut renderer,
                    &mut gpu_resources,
                    &mut world,
                    &mut interactions,
                );
            }
        }
    })
}

#[cfg(target_arch = "wasm32")]
async fn run(event_loop: EventLoop<()>, mut window: Option<Window>) {
    // The event handling loop is terminated when the main window is closed.
    // We can trigger this by dropping the window, so we wrap it in the Option
    // type.  This is a bit of a hack, but it works.  We require that we are
    // called with a valid window, however.
    window.as_ref().expect("window should exist");

    // These resources are *supposed* to be created after receiving the
    // Event::Resumed message within the event loop.  However on since async
    // support on wasm is wonky, we can't call `resume_renderer` from within
    // the event loop.  There seems to be no problem with calling it here and
    // then never dropping the resources on Event::Suspended.
    let (mut r, g, w, i) = resume_renderer(window.as_ref().unwrap()).await;
    r.set_camera(ArcballCamera::new(Vec3::zero(), 100.0, 1.0));
    let mut renderer = Some(r);
    let mut gpu_resources = Some(g);
    let mut world = Some(w);
    let mut interactions = Some(i);

    // Run the event loop.
    event_loop.run(move |event, _, control_flow| {
        // When we are done handling this event, suspend until the next event.
        *control_flow = ControlFlow::Wait;

        // Handle events.
        match event {
            // Ignore these messages (see above).
            Event::Resumed => {}
            Event::Suspended => {}

            // Process all other events.
            _ => {
                handle_event(
                    event,
                    control_flow,
                    &mut window,
                    &mut renderer,
                    &mut gpu_resources,
                    &mut world,
                    &mut interactions,
                );
            }
        }
    })
}

pub fn start(event_loop: winit::event_loop::EventLoop<()>) {
    // Create the main window.
    let window = match WindowBuilder::new().with_title(APP_NAME).build(&event_loop) {
        Err(e) => {
            println!("Failed to create window: {}", e);
            std::process::exit(1);
        }
        Ok(window) => window,
    };

    // Add the menu bar to the window / application instance, using native
    // APIs.
    menubar::setup_menu_bar(&window);

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
        run(event_loop, Some(window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
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
        window.set_inner_size(PhysicalSize::new(
            width * scale_factor,
            height * scale_factor,
        ));
        // On wasm, append the canvas to the document body
        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("app-container")?;
                let canvas = web_sys::Element::from(window.canvas()?);
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
        wasm_bindgen_futures::spawn_local(run(event_loop, Some(window)));
    }
}

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    use winit::event_loop::EventLoopBuilder;
    use winit::platform::android::EventLoopBuilderExtAndroid;
    start(
        EventLoopBuilder::with_user_event()
            .with_android_app(app)
            .build(),
    )
}

// End of File
