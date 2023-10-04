// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{platform::menubar::attach_menu, APP_NAME};
use bevy::{prelude::*, winit::WinitWindows};

/// This plugin is responsible for the initializing the application's menu
/// bar, attaching it to the primary window, and handling any keyboard
/// shortcut or menu selection events.
pub struct MenuBarPlugin;

impl Plugin for MenuBarPlugin {
    fn build(&self, app: &mut App) {
        // Setup the menu bar, and attach it to the primary window (on Windows
        // or X11), or to the application itself (macOS).
        app.add_systems(Startup, setup_menu_bar);
    }
}

// A menubar is a hierarchical list of actions with attached titles and/or
// keyboard shortcuts.  It is attached to either the application instance
// (macOS) or the main window (Windows/Linux).
//
// Menus can also be contextual (e.g. a popup right-click menu) or accessed
// from the system tray.
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
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
    SubMenu(Menu),
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
    HideApp,
    HideOthers,
    QuitApp,
}

#[derive(Clone, Copy, PartialEq)]
pub struct ModifierKeys(u8);

impl ModifierKeys {
    pub const NONE: ModifierKeys = ModifierKeys(0);
    pub const CAPSLOCK: ModifierKeys = ModifierKeys(1 << 0);
    pub const SHIFT: ModifierKeys = ModifierKeys(1 << 1);
    pub const CONTROL: ModifierKeys = ModifierKeys(1 << 2);
    pub const OPTION: ModifierKeys = ModifierKeys(1 << 3);
    pub const COMMAND: ModifierKeys = ModifierKeys(1 << 4);
    pub const NUMPAD: ModifierKeys = ModifierKeys(1 << 5);
    pub const HELP: ModifierKeys = ModifierKeys(1 << 6);
    pub const FUNCTION: ModifierKeys = ModifierKeys(1 << 7);

    pub fn contains(self, other: ModifierKeys) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl std::ops::BitOr for ModifierKeys {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        ModifierKeys(self.0 | rhs.0)
    }
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
    ServicesMenu,
    HideApp,
    HideOthers,
    ShowAll,
    Terminate,
}

pub fn setup_menu_bar(
    // We have to use `NonSend` here.  This forces this function to be called
    // from the winit thread (which is the main thread on macOS), and after
    // the window has been created.  We don't actually use the window on
    // macOS, but we do need to be in the main (event loop) thread in order to
    // access the macOS APIs we need.
    windows: NonSend<WinitWindows>,
) {
    let menubar = Menu::new(APP_NAME).and_then(MenuItem::SubMenu(
        Menu::new("")
            .and_then(MenuItem::new(
                &format!("About {}", APP_NAME),
                MenuShortcut::None,
                MenuAction::System(SystemAction::LaunchAboutWindow),
            ))
            .and_then(MenuItem::Separator)
            .and_then(MenuItem::new(
                "Settings...",
                MenuShortcut::System(SystemShortcut::Preferences),
                MenuAction::System(SystemAction::LaunchPreferences),
            ))
            .and_then(MenuItem::Separator)
            .and_then(MenuItem::new(
                "Services",
                MenuShortcut::None,
                MenuAction::System(SystemAction::ServicesMenu),
            ))
            .and_then(MenuItem::Separator)
            .and_then(MenuItem::new(
                &format!("Hide {}", APP_NAME),
                MenuShortcut::System(SystemShortcut::HideApp),
                MenuAction::System(SystemAction::HideApp),
            ))
            .and_then(MenuItem::new(
                "Hide Others",
                MenuShortcut::System(SystemShortcut::HideOthers),
                MenuAction::System(SystemAction::HideOthers),
            ))
            .and_then(MenuItem::new(
                "Show All",
                MenuShortcut::None,
                MenuAction::System(SystemAction::ShowAll),
            ))
            .and_then(MenuItem::Separator)
            .and_then(MenuItem::new(
                &format!("Quit {}", APP_NAME),
                MenuShortcut::System(SystemShortcut::QuitApp),
                MenuAction::System(SystemAction::Terminate),
            )),
    ));

    // Do the platform-dependent work of constructing the menubar and
    // attaching it to the application object or main window.
    attach_menu(&windows, &menubar);
}

// End of File
