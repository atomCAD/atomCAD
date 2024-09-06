// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use crate::window::WindowManager;
use std::sync::Arc;
use winit::{
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

/// The SplashScreen is a special type of window that is shown when the application is first
/// started.  It represents the entry point into the program's UI/UX, and is responsible for
/// initializing the application state and resources.  It is typically used to show a loading
/// screen, a splash screen, or a login screen.  In document/workspace-oriented user interfaces, the
/// SplashScreen also provides a way to open or create new workspaces in their own Workspace's.
pub struct SplashScreen {
    title: String,
    blueprint: Option<menu::Blueprint>,
    window: Option<Arc<Window>>,
    running: bool,
}

impl SplashScreen {
    pub fn new(title: String, blueprint: Option<menu::Blueprint>) -> Self {
        Self {
            title,
            blueprint,
            window: None,
            running: false,
        }
    }
}

impl WindowManager for SplashScreen {
    fn set_title(&mut self, title: String) {
        self.title = title;
        if let Some(window) = self.window.as_ref() {
            window.set_title(&self.title);
        }
    }

    fn get_title(&self) -> &str {
        &self.title
    }

    fn get_window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|w| w.id())
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            match WindowManager::create(self, event_loop) {
                Err(error) => {
                    log::error!("Failed to create window: {}", error);
                    return;
                }
                Ok(window) => self.window = Some(Arc::new(window)),
            };

            if let (Some(blueprint), Some(window)) = (self.blueprint.as_ref(), self.window.as_ref())
            {
                menu::attach_menubar_to_window(window, blueprint);
            }
        }

        self.running = true;
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        // Currently unused.
        let _ = event_loop;
        // Destroy the window (it will be recreated when resumed).
        self.running = false;
        self.window = None;
    }
}

// End of File
