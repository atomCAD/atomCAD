// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use app::prelude::*;
use ecs::prelude::*;
use logging::prelude::*;
use window::prelude::*;
use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};
use winit_runner::{WinitEventLoop, WinitPlugin};

pub const APP_NAME: &str = "atomCAD";

struct Application {
    app: App,
}

impl Application {
    fn new(app: App) -> Self {
        Self { app }
    }
}

impl ApplicationHandler<()> for Application {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
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
            event_loop.exit();
        }
    }
}

fn runner(mut app: App) -> AppExit {
    let WinitEventLoop(event_loop) = app
        .remove_non_send::<WinitEventLoop<()>>()
        .expect("Cannot find EventLoop in world");
    let mut app = Application::new(app);
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

fn hello_world(mut events: EventWriter<AppExit>) {
    log::info!("Hello, World!");
    events.send(AppExit::Success);
}

pub fn start() -> AppExit {
    App::new(APP_NAME.into())
        .add_plugin(LoggingPlugin::new(vec![
            env!("CARGO_PKG_NAME"),
            "atomcad_app",
            "atomcad_ecs",
            "atomcad_keyboard",
            "atomcad_logging",
            "atomcad_window",
            "atomcad_winit_runner",
        ]))
        .add_plugin(WindowPlugin::new(ExitCondition::DoNotExit))
        .add_plugin(WinitPlugin::<()>::default())
        .add_systems(Startup, hello_world)
        .set_runner(runner)
        .run()
}

// End of File
