// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{self as menu, SystemAction, SystemShortcut};
use bevy::{prelude::*, winit::WinitWindows};
use keyboard::ModifierKeys;
use objc2::{
    DeclaredClass, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{NSObjectProtocol, Sel},
    sel,
};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSString};
use std::sync::Arc;

struct AtomCadMenuItemIvars {
    callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

define_class!(
    #[unsafe(super(NSMenuItem))]
    #[name = "AtomCadMenuItem"]
    #[ivars = AtomCadMenuItemIvars]
    struct AtomCadMenuItem;

    impl AtomCadMenuItem {
        #[unsafe(method(doCustomAction))]
        fn do_custom_action(&self) {
            if let Some(action) = self.ivars().callback.as_ref() {
                action();
            }
        }
    }

    unsafe impl NSObjectProtocol for AtomCadMenuItem {}
);

#[allow(non_snake_case)]
impl AtomCadMenuItem {
    unsafe fn initWithTitle_action_keyEquivalent(
        this: Allocated<Self>,
        string: &NSString,
        selector: Option<Sel>,
        char_code: &NSString,
        callback: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Retained<Self> {
        let this = this.set_ivars(AtomCadMenuItemIvars { callback });
        unsafe {
            msg_send![super(this), initWithTitle:string, action:selector, keyEquivalent:char_code]
        }
    }
}

fn build_menu(
    mtm: MainThreadMarker,
    services_menu: &NSMenu,
    blueprint: &menu::Blueprint,
) -> Retained<NSMenu> {
    // Create root menu representing the menubar itself.
    let menu = mtm.alloc::<NSMenu>();
    let menu = NSMenu::initWithTitle(menu, &NSString::from_str(&blueprint.title));

    // Add each item in the blueprint to the menu, recursing into submenus as required.
    for item in blueprint.items.iter() {
        match item {
            menu::Item::Separator => {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
            menu::Item::Entry {
                title,
                shortcut,
                action,
            } => {
                let title = NSString::from_str(title);
                let mut is_service_menu = false;
                let mut callback = None;
                let action = match action {
                    menu::Action::System(action) => match action {
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
                    menu::Action::User(inner) => {
                        callback = Some(inner.clone());
                        Some(sel!(doCustomAction))
                    }
                };
                let shortcutkey = match shortcut {
                    menu::Shortcut::None => NSString::from_str(""),
                    menu::Shortcut::System(shortcut) => match shortcut {
                        SystemShortcut::Preferences => NSString::from_str(","),
                        SystemShortcut::HideApp => NSString::from_str("h"),
                        SystemShortcut::HideOthers => NSString::from_str("h"),
                        SystemShortcut::QuitApp => NSString::from_str("q"),
                        SystemShortcut::Undo => NSString::from_str("z"),
                        SystemShortcut::Redo => NSString::from_str("Z"),
                        SystemShortcut::Cut => NSString::from_str("x"),
                        SystemShortcut::Copy => NSString::from_str("c"),
                        SystemShortcut::Paste => NSString::from_str("v"),
                        SystemShortcut::SelectAll => NSString::from_str("a"),
                        SystemShortcut::Delete => NSString::from_str("\u{8}"),
                    },
                };
                let shotcutmodifiers = match shortcut {
                    menu::Shortcut::None => ModifierKeys::empty(),
                    menu::Shortcut::System(shortcut) => match shortcut {
                        SystemShortcut::Preferences => ModifierKeys::COMMAND,
                        SystemShortcut::HideApp => ModifierKeys::COMMAND,
                        SystemShortcut::HideOthers => ModifierKeys::COMMAND | ModifierKeys::OPTION,
                        SystemShortcut::QuitApp => ModifierKeys::COMMAND,
                        SystemShortcut::Undo => ModifierKeys::COMMAND,
                        SystemShortcut::Redo => ModifierKeys::COMMAND | ModifierKeys::SHIFT,
                        SystemShortcut::Cut => ModifierKeys::COMMAND,
                        SystemShortcut::Copy => ModifierKeys::COMMAND,
                        SystemShortcut::Paste => ModifierKeys::COMMAND,
                        SystemShortcut::SelectAll => ModifierKeys::COMMAND,
                        SystemShortcut::Delete => ModifierKeys::COMMAND,
                    },
                };
                let is_custom_action = callback.is_some();
                let item = mtm.alloc::<AtomCadMenuItem>();
                let item = unsafe {
                    AtomCadMenuItem::initWithTitle_action_keyEquivalent(
                        item,
                        &title,
                        action,
                        &shortcutkey,
                        callback,
                    )
                };
                if is_custom_action {
                    unsafe {
                        item.setTarget(Some(&item));
                    }
                }
                if shotcutmodifiers != ModifierKeys::empty() {
                    let mut key_equivalent_modifier_mask = NSEventModifierFlags::empty();
                    if shotcutmodifiers.contains(ModifierKeys::CAPSLOCK) {
                        key_equivalent_modifier_mask.insert(NSEventModifierFlags::CapsLock);
                    }
                    if shotcutmodifiers.contains(ModifierKeys::SHIFT) {
                        key_equivalent_modifier_mask.insert(NSEventModifierFlags::Shift);
                    }
                    if shotcutmodifiers.contains(ModifierKeys::CONTROL) {
                        key_equivalent_modifier_mask.insert(NSEventModifierFlags::Control);
                    }
                    if shotcutmodifiers.contains(ModifierKeys::OPTION) {
                        key_equivalent_modifier_mask.insert(NSEventModifierFlags::Option);
                    }
                    if shotcutmodifiers.contains(ModifierKeys::COMMAND) {
                        key_equivalent_modifier_mask.insert(NSEventModifierFlags::Command);
                    }
                    if shotcutmodifiers.contains(ModifierKeys::NUMPAD) {
                        key_equivalent_modifier_mask.insert(NSEventModifierFlags::NumericPad);
                    }
                    if shotcutmodifiers.contains(ModifierKeys::HELP) {
                        key_equivalent_modifier_mask.insert(NSEventModifierFlags::Help);
                    }
                    if shotcutmodifiers.contains(ModifierKeys::FUNCTION) {
                        key_equivalent_modifier_mask.insert(NSEventModifierFlags::Function);
                    }
                    item.setKeyEquivalentModifierMask(key_equivalent_modifier_mask);
                }
                if is_service_menu {
                    item.setSubmenu(Some(services_menu));
                }
                menu.addItem(&item);
            }
            menu::Item::SubMenu(blueprint) => {
                let item = mtm.alloc::<NSMenuItem>();
                let item = NSMenuItem::init(item);
                let submenu = build_menu(mtm, services_menu, blueprint);
                item.setSubmenu(Some(&submenu));
                menu.addItem(&item);
            }
        }
    }

    // Return the root menu object to the caller.
    menu
}

pub fn configure_event_loop(windows: NonSend<WinitWindows>) {
    //event_loop_builder.with_default_menu(false);
    let _ = windows;
}

/// Create the menu on macOS using Cocoa APIs.
pub fn attach_to_window(
    // The actual layout of the menubar spec.
    blueprint: &menu::Blueprint,
) {
    // Create a marker to ensure this function is only called from the main thread.
    let mtm =
        MainThreadMarker::new().expect("Error: build_menu must be called from the main thread.");

    // Get the application object.
    let app = NSApplication::sharedApplication(mtm);

    // Create and register the services menu.
    let services_menu = mtm.alloc::<NSMenu>();
    let services_menu = NSMenu::init(services_menu);
    app.setServicesMenu(Some(&services_menu));

    // Turn the menubar description into a Cocoa menu.
    let main_menu = build_menu(mtm, &services_menu, blueprint);

    // Register the menu with the NSApplication object.
    app.setMainMenu(Some(&main_menu));

    // Change the name of the application menu to match the blueprint.
    if let Some(menubar) = app.mainMenu()
        && let Some(app_menu) = menubar.itemAtIndex(0)
    {
        app_menu.setTitle(&NSString::from_str(&blueprint.title));
    }
}

// End of File
