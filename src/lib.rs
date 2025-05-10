// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use app::prelude::*;
use logging::prelude::*;

pub const APP_NAME: &str = "atomCAD";

fn hello_world(app: &mut App) -> AppExit {
    let _ = app;
    log::info!("Hello, World!");
    app.update();
    app.should_exit().unwrap_or(AppExit::Success)
}

pub fn start() -> AppExit {
    App::new(APP_NAME.into())
        .add_plugin(LoggingPlugin::new(vec![
            env!("CARGO_PKG_NAME"),
            "atomcad_app",
            "atomcad_ecs",
            "atomcad_keyboard",
            "atomcad_logging",
        ]))
        .set_runner(hello_world)
        .run()
}

// End of File
