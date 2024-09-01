// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use ecs::prelude::*;

/// Component that marks an entity as a window, and contains all the relevant cross-platform window
/// state.
#[derive(Component)]
pub struct Window;

/// Tag component that marks a [`Window`] entity as the primary window.  This is primarily useful in
/// identifying the “main window” of an application (if an application has a main window), and is
/// used by the [`exit_on_primary_closed`](crate::system::exit_on_primary_closed) system to
/// determine when the application should exit.
#[derive(Component)]
pub struct PrimaryWindow;

// End of File
