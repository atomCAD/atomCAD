// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

mod splash_screen;
mod window_manager;

pub mod window {
    pub use super::splash_screen::SplashScreen;

    pub use super::window_manager::WindowManager;
}

// End of File
