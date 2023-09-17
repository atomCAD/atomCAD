// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::platform::menubar::{attach_menu, configure_event_loop, Menu};
use crate::APP_NAME;
use winit::event_loop::EventLoopBuilder;
use winit::window::Window;

// A menubar is a hierarchical list of actions with attached titles and/or
// keyboard shortcuts.  It is attached to either the application instance
// (macOS) or the main window (Windows/Linux).
//
// Menus can also be contextual (e.g. a popup right-click menu) or accessed
// from the system tray.
pub struct MenuSpec {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Default for MenuSpec {
    fn default() -> Self {
        MenuSpec::new(APP_NAME)
            .and_then(MenuItem::SubMenu(MenuSpec::new("File").and_then(
                MenuItem::new(
                    &format!("Quit {APP_NAME}"),
                    MenuShortcut::System(SystemShortcut::QuitApp),
                    MenuAction::System(SystemAction::QuitApp),
                ),
            )))
            .and_then(MenuItem::SubMenu(
                MenuSpec::new("Window")
                    .and_then(MenuItem::new(
                        "Minimize",
                        MenuShortcut::System(SystemShortcut::MinizeApp),
                        MenuAction::System(SystemAction::MinizeApp),
                    ))
                    .and_then(MenuItem::new(
                        "Maximize",
                        MenuShortcut::System(SystemShortcut::MaximizeApp),
                        MenuAction::System(SystemAction::MaximizeApp),
                    )),
            ))
            .and_then(MenuItem::SubMenu(MenuSpec::new("Help").and_then(
                MenuItem::new(
                    &format!("About {APP_NAME}"),
                    MenuShortcut::None,
                    MenuAction::System(SystemAction::LaunchAboutWindow),
                ),
            )))
    }
}

impl MenuSpec {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            items: Vec::new(),
        }
    }

    pub fn and_then(mut self, item: MenuItem) -> Self {
        self.items.push(item);
        self
    }
}

// A menu item is either an action (with an optional keyboard shortcut) or a
// submenu.  The Separator is a visual divider between groups of related menu
// items.
pub enum MenuItem {
    Separator,
    Entry(String, MenuShortcut, MenuAction),
    SubMenu(MenuSpec),
}

impl MenuItem {
    pub fn new(title: &str, shortcut: MenuShortcut, action: MenuAction) -> Self {
        Self::Entry(title.to_owned(), shortcut, action)
    }
}

// A keyboard shortcut is a combination of modifier keys (e.g. Shift, Option,
// Alt, etc.) and the key to press (indicated by a unicode character).
#[derive(Clone, Copy)]
pub enum MenuShortcut {
    None,
    System(SystemShortcut),
}

// Common actions like copy-paste, file-open, and quit are usually bound to
// shortcuts that vary from platform to platform, but are expected to remain
// consistent across all apps on that platform.
#[derive(Clone, Copy)]
pub enum SystemShortcut {
    Preferences,
    MinizeApp,
    MaximizeApp,
    QuitApp,
}

// A menu action is a callback that is invoked when the menu item is selected.
// There are also a number of important platform-specific actions that can be
// invoked.
pub enum MenuAction {
    System(SystemAction),
}

pub enum SystemAction {
    LaunchAboutWindow,
    LaunchPreferences,
    MinizeApp,
    MaximizeApp,
    QuitApp,
}

pub fn setup_menu_bar<T: 'static>(event_loop_builder: &mut EventLoopBuilder<T>) -> Menu {
    // Do the platform-dependent work of configuring the event loop and
    // build the menu.
    configure_event_loop(event_loop_builder)
}

pub fn attach_menu_bar(window: &Window, menu: &Menu) {
    // Do the platform-dependent work of constructing the menubar and
    // attaching it to the application object or main window.
    attach_menu(window, menu);
}

// End of File
