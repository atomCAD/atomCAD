// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use crate::window::{PrimaryWindow, Window};
use app::prelude::*;
use ecs::prelude::*;

/// System that sends an [`AppExit`] event when all windows are closed.  When run, it will query if
/// any windows are open.  If not a single windows are open, it will send an [`AppExit::Success`]
/// event, which will cause the application to exit at the end up the update cycle.
pub fn exit_on_all_closed(mut app_exit_events: EventWriter<AppExit>, windows: Query<&Window>) {
    if windows.is_empty() {
        log::info!("All windows closed: sending AppExit event.");
        app_exit_events.send(AppExit::Success);
    }
}

/// System that sends an [`AppExit`] event when all primary windows have been closed.  The primary
/// window is a [`Window`] entity with the [`PrimaryWindow`] component.  This is useful for
/// applications which operate with a document-oriented interface, where the primary windows
/// represent open documents / projects but there may be other inspector windows open.  Once all the
/// document windows are closed, the user expectation is that the application itself should close
/// the inspector windows automatically and terminate.
pub fn exit_on_primary_closed(
    mut app_exit_events: EventWriter<AppExit>,
    windows: Query<(), (With<Window>, With<PrimaryWindow>)>,
) {
    if windows.is_empty() {
        log::info!("Primary window closed: sending AppExit event.");
        app_exit_events.send(AppExit::Success);
    }
}

// End of File
