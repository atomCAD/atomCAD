// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

/// A menubar is a hierarchical list of actions with attached titles and/or keyboard shortcuts.  It
/// is attached to either the application instance (macOS), the main window (Windows/Linux), or
/// fully emulated (mobile/web).  On platforms that lack per-window menubars, the application must
/// switch the global menubar based on the active window.
///
/// Menus can also be contextual (e.g. a popup right-click menu) or accessed from the system tray.
pub struct Blueprint {
    pub title: String,
    pub items: Vec<Item>,
}

impl Blueprint {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            items: Vec::new(),
        }
    }
}

/// A menu item is either an action (with an optional keyboard shortcut) or a submenu.  The
/// Separator is a visual divider between groups of related menu items.
pub enum Item {
    Separator,
    Entry {
        title: String,
        shortcut: Shortcut,
        action: Action,
    },
    SubMenu(Blueprint),
}

/// A keyboard shortcut is a combination of modifier keys (e.g. Shift, Option, Alt, etc.) and the
/// key to press (indicated by a unicode character).  Some shortcuts for common actions like copy,
/// paste, quit, etc. are system-wide and cannot be overridden by the application.
#[derive(Clone, Copy)]
pub enum Shortcut {
    None,
    System(SystemShortcut),
}

/// Common actions like copy-paste, file-open, and quit are usually bound to shortcuts that vary
/// from platform to platform, but are expected to remain consistent across all apps on that
/// platform.
#[derive(Clone, Copy)]
pub enum SystemShortcut {
    Preferences,
    HideApp,
    HideOthers,
    QuitApp,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Delete,
}

/// A menu action is a callback that is invoked when the menu item is selected.  It can be either an
/// internal, application-defined action, or a system response implemented by the operating system.
pub enum Action {
    System(SystemAction),
    User(Arc<dyn Fn() + 'static>),
}

/// System actions are predefined actions that are implemented by the operating system.  They are
/// usually used for common actions like showing the preferences window, hiding the app, etc.
pub enum SystemAction {
    LaunchAboutWindow,
    LaunchPreferences,
    ServicesMenu,
    HideApp,
    HideOthers,
    ShowAll,
    Terminate,
}

// End of File
