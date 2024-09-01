// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use crate::{
    exit::ExitCondition,
    system::{exit_on_all_closed, exit_on_primary_closed},
};
use app::prelude::*;

/// A plugin providing a generic / platform-independent API for interfacing with the windowing
/// system.  Used primarily to create new windows, and to cause the application to terminate when
/// its windows have been closed (see [`exit_condition`](Self::exit_condition)).
#[derive(Default)]
pub struct WindowPlugin {
    /// The conditions under which to automatically exit the application.  Defaults to
    /// [`OnAllClosed`](ExitCondition::OnAllClosed) which causes the [`exit_on_all_closed`] system
    /// to be added to the [`PostUpdate`] schedule.
    pub exit_condition: ExitCondition,
}

impl WindowPlugin {
    /// Create a new [`WindowPlugin`] with the given [`ExitCondition`].
    pub fn new(exit_condition: ExitCondition) -> Self {
        Self { exit_condition }
    }
}

impl Plugin for WindowPlugin {
    /// Adds the appropriate systems to the [`App`] based on the
    /// [`exit_condition`](Self::exit_condition), to detect when the exit condition has been met and
    /// send an [`AppExit`](app::AppExit) event, terminating the application at the end of the
    /// update cycle.
    fn register(&self, app: &mut App) {
        match self.exit_condition {
            ExitCondition::OnPrimaryClosed => {
                app.add_systems(PostUpdate, exit_on_primary_closed);
            }
            ExitCondition::OnAllClosed => {
                app.add_systems(PostUpdate, exit_on_all_closed);
            }
            ExitCondition::DoNotExit => {}
        }
    }
}

// End of File
