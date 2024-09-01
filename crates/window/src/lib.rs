// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

mod exit;
pub use exit::ExitCondition;

mod plugin;
pub use plugin::WindowPlugin;

mod system;
pub use system::{exit_on_all_closed, exit_on_primary_closed};

mod window;
pub use window::{PrimaryWindow, Window};

/// Most commonly used types, suitable for glob import.
pub mod prelude {
    pub use crate::{
        exit::ExitCondition,
        plugin::WindowPlugin,
        window::{PrimaryWindow, Window},
    };
}

// End of File
