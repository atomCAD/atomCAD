// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::platform::EVENT_LOOP_PROXY;
use bevy::{app::TerminalCtrlCHandlerPlugin, prelude::*};

pub fn setup_ctrlc_handler() {
    ctrlc::set_handler(move || {
        info!("Received Ctrl-C, instructing bevy to gracefully exit...");
        TerminalCtrlCHandlerPlugin::gracefully_exit();
        info!("Waking up event processing thread to handle the AppExit event...");
        EVENT_LOOP_PROXY.wake_event_loop();
    })
    .expect("Error setting Ctrl-C handler");
}

// End of File
