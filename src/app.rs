// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::{Arc, Mutex, mpsc};

use crate::{APP_NAME, CadViewPlugin, LoadingPlugin, SplashScreenPlugin};
use bevy::{ecs::system::NonSendMarker, prelude::*};
use menu::prelude::*;

// We use States to separate logic
// See https://bevy-cheatbook.github.io/programming/states.html
// Or https://github.com/bevyengine/bevy/blob/main/examples/ecs/state.rs
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash)]
pub enum AppState {
    // During the loading State the LoadingPlugin will load our assets
    #[default]
    Loading,
    // Here the “Get Started” prompt is drawn and we wait for user interaction.
    SplashScreen,
    // During this State the scene graph is rendered and the user can interact
    // with the camera.
    CadView,
}

#[derive(Clone, Copy)]
enum MenuAction {
    Quit,
}

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        // When the user interacts with the menu, the selected menu item's handler will be called.
        // This handler will use menu_tx to send a message to the following channel, which will be
        // processed at the next frame update.
        let (menu_tx, menu_rx) = mpsc::channel();
        let menu_tx = Arc::new(menu_tx);
        let menu_rx = Mutex::new(menu_rx);

        let menubar = menu::Blueprint {
            title: APP_NAME.into(),
            items: vec![
                menu::Item::SubMenu(menu::Blueprint {
                    title: "".into(),
                    items: vec![
                        menu::Item::Entry {
                            title: format!("About {}", APP_NAME),
                            shortcut: menu::Shortcut::None,
                            action: show_about_panel(),
                        },
                        menu::Item::Separator,
                        menu::Item::Entry {
                            title: "Settings...".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::Preferences),
                            action: menu::Action::System(menu::SystemAction::LaunchPreferences),
                        },
                        menu::Item::Separator,
                        menu::Item::Entry {
                            title: "Services".into(),
                            shortcut: menu::Shortcut::None,
                            action: menu::Action::System(menu::SystemAction::ServicesMenu),
                        },
                        menu::Item::Separator,
                        menu::Item::Entry {
                            title: format!("Hide {}", APP_NAME),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::HideApp),
                            action: menu::Action::System(menu::SystemAction::HideApp),
                        },
                        menu::Item::Entry {
                            title: "Hide Others".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::HideOthers),
                            action: menu::Action::System(menu::SystemAction::HideOthers),
                        },
                        menu::Item::Entry {
                            title: "Show All".into(),
                            shortcut: menu::Shortcut::None,
                            action: menu::Action::System(menu::SystemAction::ShowAll),
                        },
                        menu::Item::Separator,
                        {
                            let menu_tx = menu_tx.clone();
                            menu::Item::Entry {
                                title: format!("Quit {}", APP_NAME),
                                shortcut: menu::Shortcut::System(menu::SystemShortcut::QuitApp),
                                action: menu::Action::User(Arc::new(move || {
                                    if menu_tx.send(MenuAction::Quit).is_err() {
                                        error!("Failed to send quit message; exiting.");
                                        std::process::exit(-1);
                                    };
                                    info!("Quit requested by menu selection.");
                                })),
                            }
                        },
                    ],
                }),
                menu::Item::SubMenu(menu::Blueprint {
                    title: "File".into(),
                    items: vec![],
                }),
                menu::Item::SubMenu(menu::Blueprint {
                    title: "Edit".into(),
                    items: vec![
                        menu::Item::Entry {
                            title: "Undo".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::Undo),
                            action: menu::Action::User(Arc::new(|| {
                                info!("Undo requested by menu selection.");
                            })),
                        },
                        menu::Item::Entry {
                            title: "Redo".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::Redo),
                            action: menu::Action::User(Arc::new(|| {
                                info!("Redo requested by menu selection.");
                            })),
                        },
                        menu::Item::Separator,
                        menu::Item::Entry {
                            title: "Cut".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::Cut),
                            action: menu::Action::User(Arc::new(|| {
                                info!("Cut requested by menu selection.");
                            })),
                        },
                        menu::Item::Entry {
                            title: "Copy".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::Copy),
                            action: menu::Action::User(Arc::new(|| {
                                info!("Copy requested by menu selection.");
                            })),
                        },
                        menu::Item::Entry {
                            title: "Paste".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::Paste),
                            action: menu::Action::User(Arc::new(|| {
                                info!("Paste requested by menu selection.");
                            })),
                        },
                        menu::Item::Entry {
                            title: "Delete".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::Delete),
                            action: menu::Action::User(Arc::new(|| {
                                info!("Delete requested by menu selection.");
                            })),
                        },
                        menu::Item::Entry {
                            title: "Select All".into(),
                            shortcut: menu::Shortcut::System(menu::SystemShortcut::SelectAll),
                            action: menu::Action::User(Arc::new(|| {
                                info!("Select All requested by menu selection.");
                            })),
                        },
                    ],
                }),
                menu::Item::SubMenu(menu::Blueprint {
                    title: "Go".into(),
                    items: vec![],
                }),
                menu::Item::SubMenu(menu::Blueprint {
                    title: "View".into(),
                    items: vec![],
                }),
                menu::Item::SubMenu(menu::Blueprint {
                    title: "Window".into(),
                    items: vec![],
                }),
                menu::Item::SubMenu(menu::Blueprint {
                    title: "Help".into(),
                    items: vec![],
                }),
            ],
        };

        app.init_state::<AppState>()
            .add_systems(First, move |mut ev_app_exit: MessageWriter<AppExit>| {
                if let Ok(receiver) = menu_rx.try_lock()
                    && let Ok(action) = receiver.try_recv()
                {
                    match action {
                        MenuAction::Quit => {
                            ev_app_exit.write(AppExit::Success);
                        }
                    }
                }
            })
            .add_plugins((
                MenubarPlugin::new(menubar),
                LoadingPlugin,
                SplashScreenPlugin,
                CadViewPlugin,
            ))
            .add_systems(Startup, set_window_icon);
    }
}

/// Sets the icon on Windows and X11.  The icon on macOS is sourced from the enclosing bundle, and
/// is set in the Info.plist file.  That would be highly platform-specific code, and handled prior
/// to bevy startup, not here.
#[cfg(not(target_os = "macos"))]
use bevy::window::PrimaryWindow;
#[cfg(not(target_os = "macos"))]
pub fn set_window_icon(
    _non_send_marker: NonSendMarker,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    use bevy::winit::WINIT_WINDOWS;
    // with_borrow_mut() will panic if WINIT_WINDOWS is borrowed elsewhere,
    // but all borrowings should be in the main thread,
    // and we must run in the main thread because of NonSendMarker.
    // So this should be safe.
    WINIT_WINDOWS.with_borrow_mut(|windows| {
        use std::io::Cursor;
        use winit::window::Icon;
        let primary_entity = match primary_window.single() {
            Ok(primary_entity) => primary_entity,
            Err(_) => return,
        };
        let primary = match windows.get_window(primary_entity) {
            Some(primary) => primary,
            None => return,
        };
        let icon_buf = Cursor::new(include_bytes!(env!("ATOMCAD_ICON_PATH")));
        if let Ok(image) = image::load(icon_buf, image::ImageFormat::Png) {
            let image = image.into_rgba8();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();
            let icon = Icon::from_rgba(rgba, width, height).unwrap();
            primary.set_window_icon(Some(icon));
        };
    });
}

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSImage;
#[cfg(target_os = "macos")]
fn get_app_icon() -> Retained<NSImage> {
    static ICON_DATA: &[u8] = include_bytes!(env!("ATOMCAD_ICNS_PATH"));
    use objc2::AllocAnyThread;
    use objc2_app_kit::NSImage;
    use objc2_foundation::NSData;
    use std::{os::raw::c_void, ptr::NonNull};
    let data = unsafe {
        NSData::dataWithBytesNoCopy_length(
            NonNull::new_unchecked(ICON_DATA.as_ptr() as *mut c_void),
            ICON_DATA.len(),
        )
    };
    let image = NSImage::alloc();
    NSImage::initWithData(image, &data).expect("Failed to create NSImage from data.")
}

#[cfg(target_os = "macos")]
pub fn set_window_icon(_non_send_marker: NonSendMarker) {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;
    let image = get_app_icon();
    let mtm = MainThreadMarker::new().expect("Must run on main thread.");
    let app = NSApplication::sharedApplication(mtm);
    unsafe {
        app.setApplicationIconImage(Some(&image));
    }
}

#[cfg(not(target_os = "macos"))]
fn show_about_panel() -> menu::Action {
    menu::Action::System(menu::SystemAction::LaunchAboutWindow)
}

#[cfg(target_os = "macos")]
fn show_about_panel() -> menu::Action {
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2_app_kit::{
        NSAboutPanelOptionApplicationIcon, NSAboutPanelOptionApplicationName,
        NSAboutPanelOptionApplicationVersion, NSAboutPanelOptionCredits, NSAboutPanelOptionVersion,
        NSApplication,
    };
    use objc2_foundation::{MainThreadMarker, NSAttributedString, NSMutableDictionary, NSString};

    menu::Action::User(Arc::new(|| {
        // This code will display the About panel with the correct icon and version
        // The icon is set by set_app_icon() and the version is read from Info.plist
        let mtm = MainThreadMarker::new().expect("Must run on main thread.");
        let app = NSApplication::sharedApplication(mtm);

        let icon = get_app_icon();
        let app_name = NSString::from_str(APP_NAME);
        let build = NSString::from_str("1");
        let credits = NSAttributedString::from_nsstring(&NSString::from_str("Credits"));
        let version = NSString::from_str(env!("CARGO_PKG_VERSION"));

        unsafe {
            let options_dictionary = NSMutableDictionary::new();
            options_dictionary.setObject_forKey(
                &*Retained::into_raw(icon) as &AnyObject,
                ProtocolObject::from_ref(NSAboutPanelOptionApplicationIcon),
            );
            options_dictionary.setObject_forKey(
                &*Retained::into_raw(app_name) as &AnyObject,
                ProtocolObject::from_ref(NSAboutPanelOptionApplicationName),
            );
            options_dictionary.setObject_forKey(
                &*Retained::into_raw(version) as &AnyObject,
                ProtocolObject::from_ref(NSAboutPanelOptionApplicationVersion),
            );
            options_dictionary.setObject_forKey(
                &*Retained::into_raw(build) as &AnyObject,
                ProtocolObject::from_ref(NSAboutPanelOptionVersion),
            );
            options_dictionary.setObject_forKey(
                &*Retained::into_raw(credits) as &AnyObject,
                ProtocolObject::from_ref(NSAboutPanelOptionCredits),
            );
            app.orderFrontStandardAboutPanelWithOptions(&options_dictionary);
        }
    }))
}

// End of File
