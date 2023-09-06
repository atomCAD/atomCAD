// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

use objc::rc::autoreleasepool;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};

use winit::{
    event_loop::EventLoopBuilder, platform::macos::EventLoopBuilderExtMacOS, window::Window,
};

use crate::menubar::{
    MenuAction, MenuItem, MenuShortcut, MenuSpec, ModifierKeys, SystemAction, SystemShortcut,
};

fn nsstring(s: &str) -> *mut Object {
    unsafe {
        let cls = class!(NSString);
        let bytes = s.as_ptr();
        let len = s.len();
        let encoding = 4; // UTF-8
        let obj: *mut Object = msg_send![cls, alloc];
        let obj: *mut Object = msg_send![obj, initWithBytes:bytes length:len encoding:encoding];
        let obj: *mut Object = msg_send![obj, autorelease];
        obj
    }
}

unsafe fn build_menu(
    _app: *mut Object,
    services_menu: *mut Object,
    menu_spec: &MenuSpec,
) -> *mut Object {
    // Create root menu bar.
    let menuobj: *mut Object = msg_send![class![NSMenu], alloc];
    let menuobj: *mut Object = msg_send![menuobj, initWithTitle: nsstring(&menu_spec.title)];
    let menuobj: *mut Object = msg_send![menuobj, autorelease];

    for menuitem in menu_spec.items.iter() {
        match menuitem {
            MenuItem::Separator => {
                let item: *mut Object = msg_send![class![NSMenuItem], separatorItem];
                let _: () = msg_send![menuobj, addItem: item];
            }
            MenuItem::Entry(title, shortcut, action) => {
                let title = nsstring(&title);
                let mut is_service_menu = false;
                let action = match action {
                    MenuAction::System(action) => match action {
                        SystemAction::LaunchAboutWindow => {
                            Some(sel!(orderFrontStandardAboutPanel:))
                        }
                        SystemAction::LaunchPreferences => Some(sel!(orderFrontPreferencesPanel:)),
                        SystemAction::ServicesMenu => {
                            is_service_menu = true;
                            None
                        }
                        SystemAction::HideApp => Some(sel!(hide:)),
                        SystemAction::HideOthers => Some(sel!(hideOtherApplications:)),
                        SystemAction::ShowAll => Some(sel!(unhideAllApplications:)),
                        SystemAction::Terminate => Some(sel!(terminate:)),
                    },
                };
                let shortcutkey = match shortcut {
                    MenuShortcut::None => nsstring(""),
                    MenuShortcut::System(shortcut) => match shortcut {
                        SystemShortcut::Preferences => nsstring(","),
                        SystemShortcut::HideApp => nsstring("h"),
                        SystemShortcut::HideOthers => nsstring("h"),
                        SystemShortcut::QuitApp => nsstring("q"),
                    },
                };
                let shotcutmodifiers = match shortcut {
                    MenuShortcut::None => ModifierKeys::NONE,
                    MenuShortcut::System(shortcut) => match shortcut {
                        SystemShortcut::Preferences => ModifierKeys::COMMAND,
                        SystemShortcut::HideApp => ModifierKeys::COMMAND,
                        SystemShortcut::HideOthers => ModifierKeys::COMMAND | ModifierKeys::OPTION,
                        SystemShortcut::QuitApp => ModifierKeys::COMMAND,
                    },
                };
                let mut item: *mut Object = msg_send![class![NSMenuItem], alloc];
                if let Some(action) = action {
                    item = msg_send![item,
                                     initWithTitle: title
                                     action: action
                                     keyEquivalent: shortcutkey];
                } else {
                    item = msg_send![item,
                                     initWithTitle: title
                                     action: 0
                                     keyEquivalent: shortcutkey];
                }
                if shotcutmodifiers != ModifierKeys::NONE {
                    let mut modifiermask = 0usize;
                    if shotcutmodifiers.contains(ModifierKeys::CAPSLOCK) {
                        modifiermask |= 1 << 16; // NSEventModifierFlagCapsLock
                    }
                    if shotcutmodifiers.contains(ModifierKeys::SHIFT) {
                        modifiermask |= 1 << 17; // NSEventModifierFlagShift
                    }
                    if shotcutmodifiers.contains(ModifierKeys::CONTROL) {
                        modifiermask |= 1 << 18; // NSEventModifierFlagControl
                    }
                    if shotcutmodifiers.contains(ModifierKeys::OPTION) {
                        modifiermask |= 1 << 19; // NSEventModifierFlagOption
                    }
                    if shotcutmodifiers.contains(ModifierKeys::COMMAND) {
                        modifiermask |= 1 << 20; // NSEventModifierFlagCommand
                    }
                    if shotcutmodifiers.contains(ModifierKeys::NUMPAD) {
                        modifiermask |= 1 << 21; // NSEventModifierFlagNumericPad
                    }
                    if shotcutmodifiers.contains(ModifierKeys::HELP) {
                        modifiermask |= 1 << 22; // NSEventModifierFlagHelp
                    }
                    if shotcutmodifiers.contains(ModifierKeys::FUNCTION) {
                        modifiermask |= 1 << 23; // NSEventModifierFlagFunction
                    }
                    let _: () = msg_send![item, setKeyEquivalentModifierMask: modifiermask];
                }
                item = msg_send![item, autorelease];
                if is_service_menu {
                    let _: () = msg_send![item, setSubmenu: services_menu];
                }
                let _: () = msg_send![menuobj, addItem: item];
            }
            MenuItem::SubMenu(submenu) => {
                let item: *mut Object = msg_send![class![NSMenuItem], alloc];
                let item: *mut Object = msg_send![item, init];
                let item: *mut Object = msg_send![item, autorelease];
                let submenu = build_menu(_app, services_menu, &submenu);
                let _: () = msg_send![item, setSubmenu: submenu];
                let _: () = msg_send![menuobj, addItem: item];
            }
        }
    }

    // Return the menu object to the caller.
    menuobj
}

// Placeholder struct to allow compilation.
pub struct Menu;

pub fn configure_event_loop<T: 'static>(event_loop_builder: &mut EventLoopBuilder<T>) -> Menu {
    event_loop_builder.with_default_menu(false);
    Menu
}

pub fn attach_menu(
    // On some platforms, e.g. Windows and Linux, the menu bar is part of the
    // window itself, and we need to add it to each individual window.  But
    // for macOS the menu bar is a property of the NSApplication instance
    // shared by the entire process, so we only need to set it once and don't
    // use the _window parameter.
    _window: &Window,
    // On some platforms the Menu type would need access to the Window, e.g Windows.
    _menu: &Menu,
) {
    // Create the menubar spec
    let menu_bar_spec: MenuSpec = MenuSpec::default();
    // Create the menu on macOS using Cocoa APIs.
    autoreleasepool(|| unsafe {
        // Get the application object.
        let app: *mut Object = msg_send![class![NSApplication], sharedApplication];

        // Create and register the services menu.
        let services_menu: *mut Object = msg_send![class![NSMenu], alloc];
        let services_menu: *mut Object = msg_send![services_menu, init];
        let services_menu: *mut Object = msg_send![services_menu, autorelease];
        let _: () = msg_send![app, setServicesMenu: services_menu];

        // Turn the menubar description into a Cocoa menu.
        let obj = build_menu(app, services_menu, &menu_bar_spec);

        // Register the menu with the NSApplication object.
        let _: () = msg_send![app, setMainMenu: obj];
    });
}

// End of File
