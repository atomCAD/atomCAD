// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.
use crate::{
    menubar::{MenuAction, MenuItem, MenuSpec, SystemAction},
    APP_LICENSE, APP_NAME, APP_VERSION,
};

pub use muda::Menu;
use muda::{AboutMetadata, PredefinedMenuItem, Submenu};
use winit::{event_loop::EventLoopBuilder, window::Window};

pub fn configure_event_loop<T: 'static>(event_loop_builder: &mut EventLoopBuilder<T>) -> Menu {
    let menu_bar_spec: MenuSpec = MenuSpec::default();
    let menu_bar: Menu = build_menu(&menu_bar_spec);

    use winit::platform::windows::EventLoopBuilderExtWindows;
    {
        let menu_bar_: Menu = menu_bar.clone();
        // Add the accelator table
        event_loop_builder.with_msg_hook(move |msg| {
            use windows_sys::Win32::UI::WindowsAndMessaging::{TranslateAcceleratorW, MSG};
            unsafe {
                let msg = msg as *const MSG;
                let translated = TranslateAcceleratorW((*msg).hwnd, menu_bar_.haccel(), msg);
                translated == 1
            }
        });
    }

    menu_bar
}

pub fn attach_menu(window: &Window, menu_bar: &Menu) {
    use winit::platform::windows::WindowExtWindows;
    menu_bar
        .init_for_hwnd(window.hwnd() as _)
        .expect("Initializing the menubar shouldn't return an error");
}

fn build_menu(menu_spec: &MenuSpec) -> Menu {
    let menu_bar = Menu::new();

    for menu_item in &menu_spec.items {
        match menu_item {
            MenuItem::Entry(title, _shortcut, action) => {
                match action {
                    MenuAction::System(SystemAction::MinizeApp) => {
                        menu_bar.append(&PredefinedMenuItem::minimize(None)).expect(
                            "Appending the 'MinizeApp' sub-menu item shouldn't return an error.",
                        );
                    }

                    MenuAction::System(SystemAction::MaximizeApp) => {
                        menu_bar.append(&PredefinedMenuItem::maximize(None)).expect(
                            "Appending the 'MaximizeApp' sub-menu item shouldn't return an error.",
                        );
                    }
                    MenuAction::System(SystemAction::LaunchAboutWindow) => {
                        menu_bar.append(&PredefinedMenuItem::about(
                        Some(&title),
                        Some(AboutMetadata {
                            name: Some(APP_NAME.to_string()),
                            version: Some(APP_VERSION.to_string()),
                            license: Some(APP_LICENSE.to_string()),
                            ..Default::default()
                        }),
                    ))
                    .expect("Appending the 'LaunchAboutWindow' sub-menu item shouldn't return an error.");
                    }
                    MenuAction::System(SystemAction::QuitApp) => {
                        menu_bar
                        .append(&PredefinedMenuItem::close_window(Some(&format!("Quit {APP_NAME}"))))
                        .expect(
                            "Appending the 'Terminate' sub-menu item shouldn't return an error.",
                        );
                    }
                    // Unsupported
                    MenuAction::System(SystemAction::LaunchPreferences) => continue,
                }
            }
            MenuItem::Separator => {
                menu_bar
                    .append(&PredefinedMenuItem::separator())
                    .expect("Appending sub-menu 'Separator' shouldn't return an error.");
            }
            MenuItem::SubMenu(sub_menu_spec) => {
                menu_bar
                    .append(&build_sub_menu(sub_menu_spec))
                    .expect("Appending a sub-menu to the menubar shouldn't");
            }
        }
    }

    menu_bar
}

// Necessary because `Menu` and `Submenu` are
fn build_sub_menu(sub_menu_spec: &MenuSpec) -> Submenu {
    let sub_menu = Submenu::new(&sub_menu_spec.title, true);

    for menu_item in &sub_menu_spec.items {
        match menu_item {
            MenuItem::Entry(title, _shortcut, action) => {
                match action {
                    MenuAction::System(SystemAction::MinizeApp) => {
                        sub_menu.append(&PredefinedMenuItem::minimize(None)).expect(
                            "Appending the 'MinizeApp' sub-menu item shouldn't return an error.",
                        );
                    }
                    MenuAction::System(SystemAction::MaximizeApp) => {
                        sub_menu.append(&PredefinedMenuItem::maximize(None)).expect(
                            "Appending the 'MaximizeApp' sub-menu item shouldn't return an error.",
                        );
                    }
                    MenuAction::System(SystemAction::LaunchAboutWindow) => {
                        sub_menu.append(&PredefinedMenuItem::about(
                        Some(&title),
                        Some(AboutMetadata {
                            name: Some(APP_NAME.to_string()),
                            version: Some(APP_VERSION.to_string()),
                            license: Some(APP_LICENSE.to_string()),
                            ..Default::default()
                        }),
                    ))
                    .expect("Appending the 'LaunchAboutWindow' sub-menu item shouldn't return an error.");
                    }
                    MenuAction::System(SystemAction::QuitApp) => {
                        sub_menu
                        .append(&PredefinedMenuItem::close_window(Some(&format!("Quit {APP_NAME}"))))
                        .expect(
                            "Appending the 'Terminate' sub-menu item shouldn't return an error.",
                        );
                    }
                    // Unsupported
                    MenuAction::System(SystemAction::LaunchPreferences) => continue,
                }
            }
            MenuItem::Separator => {
                sub_menu
                    .append(&PredefinedMenuItem::separator())
                    .expect("Appending sub-menu 'Separator' shouldn't return an error.");
            }
            MenuItem::SubMenu(sub_menu_spec) => {
                sub_menu
                    .append(&build_sub_menu(sub_menu_spec))
                    .expect("Appending a sub-menu to the menubar shouldn't");
            }
        }
    }

    sub_menu
}
