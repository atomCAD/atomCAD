// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use bevy_ecs::prelude::*;

/// The `ContainsWorld` trait provides an interface for objects which contain an ECS [`World`] to
/// expose access to the [`World`] via the [`world()`](ContainsWorld::world) and
/// [`world_mut()`](ContainsWorld::world_mut) methods.
pub trait ContainsWorld {
    /// Gets an immutable reference to the ECS [`World`].
    fn world(&self) -> &World;

    /// Gets a mutable reference to the ECS [`World`].
    fn world_mut(&mut self) -> &mut World;
}

// End of File
