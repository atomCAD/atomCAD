// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

//! # Entity Component System
//!
//! Currently little more than the simplest possible wrapper over the [`bevy_ecs`] crate.  Most of
//! this crate is literally just `pub use bevy_ecs::*`, so all the generated docs you see are copied
//! from [`bevy_ecs`].  In the future we might choose to roll our own ECS, or to use a simpler and
//! lightweight ECS crate like apecs.  Therefore we may change this crate to be an actual wrapper of
//! the Bevy ECS system instead of just re-exporting it, as the first step towards replacing it.
//!
//! The following traits are original to this crate, in addition to the re-exported Bevy types:
//!
//! - [`ResourceManager`] is trait containing the application interface for managing resources in an
//!   ECS [`World`](prelude::World).
//! - [`NonSendManager`] is a similar trait for managing non-Send resources in an ECS
//!   [`World`](prelude::World).
//! - [`ContainsWorld`] is a trait for objects that contain an ECS [`World`](prelude::World),
//!   exposing the [`world()`](ContainsWorld::world) and [`world_mut()`](ContainsWorld::world_mut)
//!   methods for getting references to the ECS [`World`](prelude::World).  Implementing
//!   [`ContainsWorld`] automatically makes default implementations of [`ResourceManager`] and
//!   [`NonSendManager`] available.
//!
//! See [`bevy_ecs`] for more information on what an `Entity Component System` is and how to use it.
pub use bevy_ecs::*;

mod atomcad;
pub use atomcad::*;

/// Most commonly used types, suitable for glob import.
pub mod prelude {
    pub use bevy_ecs::prelude::*;
    // atomcad re-exports must come after bevy_ecs re-exports
    pub use crate::atomcad::*;
}

// End of File
