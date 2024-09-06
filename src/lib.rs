// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use app::prelude::*;
use gui::window::{SplashScreen, WindowManager};
use logging::prelude::*;
use window::prelude::*;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    window::WindowId,
};
use winit_runner::{WinitEventLoop, WinitPlugin};

pub const APP_NAME: &str = "atomCAD";

#[derive(Default)]
enum StartupAction {
    /// The application is starting up for the first time, in a fresh state.  The splash screen is
    /// shown, and the user is prompted to open or create a new workspace.
    #[default]
    FirstTime,
    // In the future, add variants for other startup behaviors, such as:
    // - Resuming opened windows from a previous session
    // - Opening a specific workspace (e.g. provided as a command-line argument)
    // - Opening a new workspace directly
}

struct Application {
    /// The [application](App) object, which contains the ECS [`World`] and all the resources and
    /// systems needed to run the application.
    app: App,
    /// Whether the application is in the active state (meaning a resume message has been processed
    /// at least once, without an intervening suspend) with the event loop running.
    running: bool,
    /// The behavior of the application at startup/resume.  This determines what windows are created
    /// and shown to the user when the application is first started.  Once this action is performed,
    /// it is cleared (set to None).  Setting this value while the appliation is running will cause
    /// the event handler to perform the specified action again the next time the application is
    /// resumed (as happens on mobile when the application is sent to the background and then
    /// brought back to the foreground).
    startup_action: Option<StartupAction>,
    /// The initial window for the application, opened at startup (unless previously opened work is
    /// resumed).  This is the entry point for user interaction with the application, and typically
    /// performs authentication with cloud services and opening or creating new workspaces.  Stored
    /// as an option so that it can be dropped when the splash screen window is closed.
    splash_screen: Option<SplashScreen>,
}

impl Application {
    fn new(app: App, startup_action: StartupAction) -> Self {
        Self {
            app,
            running: false,
            // Wrapped in an Option so that it can be cleared after the specified action is
            // performed.
            startup_action: Some(startup_action),
            splash_screen: None,
        }
    }
}

impl ApplicationHandler for Application {
    /// Called when there are new events delivered by the OS to the application, but prior to
    /// processing these events.  There may have been a significant gap of time between the last set
    /// of events processed and this one.  This is a chance to set/reset timers, record timestamps,
    /// etc.  Do NOT create new windows or other resources here, as on some platforms the windowing
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if let Some(splash_screen) = &mut self.splash_screen {
            splash_screen.new_events(event_loop, cause);
        }
    }

    /// Called when the document window is first created, and when the application is brought back
    /// into focus on mobile platforms.  The window and its associated GPU resources need to be
    /// (re)created on resume.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Some platforms under some circumstances apparently send redundant (back-to-back) resume
        // events.  We can ignore these.
        if self.startup_action.is_none() && self.running {
            // The window is already created and the event loop is running.  This is a no-op.
            return;
        }

        if let Some(startup_action) = self.startup_action.take() {
            match startup_action {
                StartupAction::FirstTime => {
                    let mut splash_screen =
                        SplashScreen::new(format!("{} — Getting Started", self.app.name));
                    splash_screen.resumed(event_loop);
                    self.splash_screen = Some(splash_screen);
                }
            }
        }

        // Create the main window.
        //if let None = self.window {
        //    let window_attributes = Window::default_attributes().with_title(&self.title);
        //    self.window = match event_loop.create_window(window_attributes) {
        //        Err(error) => {
        //            log::error!("Failed to create window: {}", error);
        //            return;
        //        }
        //        Ok(window) => Some(Arc::new(window)),
        //    };
        //}
        //let _window = self.window.as_ref().unwrap();

        // Setup the rendering context, acquiring GPU resources for the canvas.

        // On some platforms, namely wasm32 + webgl2, the window is not yet in a ready state to
        // create the rendering surface when Event::Resumed is fired.  We therefore just record that
        // we're in the running state.
        //FIXME: the above text isn't accurate?!?
        self.running = true;
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ()) {
        if let Some(splash_screen) = &mut self.splash_screen {
            splash_screen.user_event(event_loop, event);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // ControlFlow::Poll continuously runs the event loop, even if the OS hasn't
        // dispatched any events. This is ideal for games and similar applications.
        // event_loop.set_control_flow(ControlFlow::Poll);

        // ControlFlow::Wait pauses the event loop if no events are available to process.
        // This is ideal for non-game applications that only update in response to user
        // input, and uses significantly less power/CPU time than ControlFlow::Poll.
        event_loop.set_control_flow(ControlFlow::Wait);

        if let Some(splash_screen) = &mut self.splash_screen {
            if splash_screen.get_window_id() == Some(window_id) {
                splash_screen.window_event(event_loop, &event);
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                // The close button was pressed.  By dropping all our window handles we trigger
                // their destruction, which will cause WindowEvent::Destroyed to sent to each and
                // then the event loop to exit.
                //
                // While this may seem redundant with the handling of WindowEvent::Destroyed below
                // (which dropping the window will trigger, and which also drops the window), but
                // that code path could be run without a WindowEvent::CloseRequested event being
                // handled first (e.g. by selecting “Quit” from the app menu), and there is no logic
                // error in clearing the value twice.
                if let Some(mut splash_screen) = self.splash_screen.take() {
                    splash_screen.request_close(event_loop);
                }
            }

            WindowEvent::Destroyed => {
                // The window was destroyed by the OS.  This can be in response to a user action
                // (e.g. clicking the close button) or an OS event.  Since the window destruction
                // might not have been already processed by us, it's possible that our window handle
                // is still valid.  We drop it to clean up any associated resources.
                if let Some(mut splash_screen) = self.splash_screen.take() {
                    splash_screen.destroy(event_loop);
                }

                // And exit the event loop.
                self.running = false;
                event_loop.exit();
            }

            _ => {}
        };
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(splash_screen) = &mut self.splash_screen {
            splash_screen.device_event(event_loop, device_id, &event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(splash_screen) = &mut self.splash_screen {
            splash_screen.about_to_wait(event_loop);
        }
        self.app.update();
        if self.app.should_exit().is_some() {
            // Someone, somewhere has told us to quit.  Exit the event loop.
            //
            // FIXME: Should we close all windows first?
            //
            // FIXME: There should be some way to pass the AppExit value returned by
            //        [`App::should_exit()`] to the event loop, so that the whole process will
            //        terminate with the specified exit code.  It is not clear how to do this with
            //        winit's current API.
            self.running = false;
            event_loop.exit();
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(splash_screen) = &mut self.splash_screen {
            splash_screen.suspended(event_loop);
        }
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(splash_screen) = &mut self.splash_screen {
            splash_screen.exiting(event_loop);
        }
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(splash_screen) = &mut self.splash_screen {
            splash_screen.memory_warning(event_loop);
        }
    }
}

fn runner(mut app: App) -> AppExit {
    let WinitEventLoop(event_loop) = app
        .remove_non_send::<WinitEventLoop<()>>()
        .expect("Cannot find EventLoop in world");
    let mut app = Application::new(app, StartupAction::FirstTime);
    match event_loop.run_app(&mut app) {
        Ok(_) => {
            log::info!("Event loop exited cleanly.");
            AppExit::Success
        }
        Err(err) => {
            log::error!("Event loop exited with error: {}", err);
            AppExit::Error(std::num::NonZeroU8::new(1).unwrap())
        }
    }
}

pub fn start() -> AppExit {
    App::new(APP_NAME.into())
        .add_plugin(LoggingPlugin::new(vec![
            env!("CARGO_PKG_NAME"),
            "atomcad_app",
            "atomcad_ecs",
            "atomcad_gui",
            "atomcad_keyboard",
            "atomcad_logging",
            "atomcad_window",
            "atomcad_winit_runner",
        ]))
        .add_plugin(WindowPlugin::new(ExitCondition::DoNotExit))
        .add_plugin(WinitPlugin::<()>::default())
        .set_runner(runner)
        .run()
}

// End of File
