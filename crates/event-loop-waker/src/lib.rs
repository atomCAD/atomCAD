// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod platform;
mod platform_impl;
pub use platform::{EventLoopWaker, EventLoopWakerPlugin, force_exit_after, setup_ctrlc_handler};

// End of File
