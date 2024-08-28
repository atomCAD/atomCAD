// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

//! # atomCAD's Application Runner
//!
//! This crate contains the core application runner logic for atomCAD projects. The actual
//! application logic is implemented by the caller, but sensible defaults are provided that cover
//! most use cases with minimal configuration.

mod app;
pub use app::{run_once, App, AppExit};

mod platform;
mod platform_impl;
pub use platform::PanicHandlerPlugin;

mod plugin;
pub use plugin::Plugin;

mod schedule;
pub use schedule::{
    First, Last, MainSchedulePlugin, PostStartup, PostUpdate, PreStartup, PreUpdate, Startup,
    Update,
};

/// Most commonly used types, suitable for glob import.
pub mod prelude {
    pub use crate::{
        app::{App, AppExit},
        plugin::Plugin,
        schedule::{First, Last, PostStartup, PostUpdate, PreStartup, PreUpdate, Startup, Update},
    };
    pub use ecs::{ContainsWorld, EventManager, NonSendManager, ResourceManager, ScheduleManager};
}

// End of File
