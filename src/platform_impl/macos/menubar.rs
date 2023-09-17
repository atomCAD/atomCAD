// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{
    menubar::{MenuAction, MenuItem, MenuSpec, SystemAction},
    APP_LICENSE, APP_NAME, APP_VERSION,
};

// Export muda's Menu type as the Menu type for this platform
pub use muda::Menu;
use muda::{AboutMetadata, PredefinedMenuItem, Submenu};
use winit::{event_loop::EventLoopBuilder, window::Window};

pub fn configure_event_loop<T: 'static>(event_loop_builder: &mut EventLoopBuilder<T>) -> Menu {
    let menu_bar_spec: MenuSpec = MenuSpec::default();
    let menu_bar: Menu = build_menu(&menu_bar_spec);

    use winit::platform::macos::EventLoopBuilderExtMacOS;
    event_loop_builder.with_default_menu(false);

    menu_bar
}

pub fn attach_menu(_window: &Window, menu_bar: &Menu) {
    // Attach the menubar to the app window
    menu_bar.init_for_nsapp();
}

fn build_menu(menu_spec: &MenuSpec) -> Menu {
    let menu_bar = Menu::new();

    // Add the MacOS-specific app menu
    let app_menu = Submenu::new(APP_NAME, true);
    app_menu
        .append_items(&[
            &PredefinedMenuItem::about(
                None,
                Some(AboutMetadata {
                    name: Some(APP_NAME.to_string()),
                    version: Some(APP_VERSION.to_string()),
                    license: Some(APP_LICENSE.to_string()),
                    ..Default::default()
                }),
            ),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .expect("Appending items to the app menu shouldn't cause an error!");
    app_menu.set_as_windows_menu_for_nsapp();
    menu_bar
        .append(&app_menu)
        .expect("Appending the app menu shouldn't return an error.");

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
                        Some(title),
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
                        Some(title),
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

// End of File
