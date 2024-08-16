// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

//! # Entity Component System
//!
//! Currently nothing more than the simplest possible wrapper over the [`bevy_ecs`] crate.  It is
//! literally just `pub use bevy_ecs::*;`, so all the generated docs are copied from [`bevy_ecs`].
//! In the future we might choose to roll our own ECS, or to use a simpler and lightweight ECS crate
//! like apecs.  Therefore in the future we may change this crate to be an actual wrapper of the
//! Bevy ECS system instead of just re-exporting it, as the first step towards replacing it.
//!
//! See [`bevy_ecs`] for more information on what an `Entity Component System` is and how to use it.
pub use bevy_ecs::*;

// End of File
