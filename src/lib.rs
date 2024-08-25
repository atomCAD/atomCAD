// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use app::prelude::*;
use std::process::ExitCode;

pub const APP_NAME: &str = "atomCAD";

fn hello_world(app: App) -> AppExit {
    let _ = app;
    println!("Hello, world!");
    AppExit::Success
}

pub fn start() -> ExitCode {
    match App::new(APP_NAME.into()).set_runner(hello_world).run() {
        AppExit::Error(code) => {
            eprintln!("{}: ExitCode: {}", APP_NAME, code.get());
            ExitCode::from(code.get())
        }
        AppExit::Success => {
            println!("{}: Success", APP_NAME);
            ExitCode::SUCCESS
        }
    }
}

// End of File
