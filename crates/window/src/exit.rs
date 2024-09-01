// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

/// Conditions under which the application should automatically exit.  For user-oriented graphical
/// applications, it is common to exit the application either when the primary window is closed or
/// when *all* windows have been closed.  Otherwise the application would continue running in the
/// background even when no windows are open, which is often not the desired behavior.  Defaults to
/// [`OnAllClosed`](ExitCondition::OnAllClosed).
#[derive(Default, Clone, Copy)]
pub enum ExitCondition {
    /// Keep the application running even after the last open window has been closed.  Be sure to
    /// have your own mechanism in place for sending an [`AppExit`](app::AppExit) event when it is
    /// time for the application to be terminated.
    DoNotExit,
    /// Close all other windows and terminate the application when the primary window is closed.
    ///
    /// [`WindowPlugin`](crate::plugin::WindowPlugin) will add
    /// [`exit_on_primary_closed`](crate::system::exit_on_primary_closed) to the
    /// [`PostUpdate`](app::PostUpdate) schedule.
    OnPrimaryClosed,
    /// Terminate the application once there are no longer any remaining open windows.  Prevents the
    /// application from remaining open / running indefinitely when the user closes the last open
    /// window, which would otherwise be often against user expectations.
    ///
    /// [`WindowPlugin`](crate::plugin::WindowPlugin) will add
    /// [`exit_on_all_closed`](crate::system::exit_on_all_closed) to the
    /// [`PostUpdate`](app::PostUpdate) schedule.
    #[default]
    OnAllClosed,
}

// End of File
