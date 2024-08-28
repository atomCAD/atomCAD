// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use crate::atomcad::ContainsWorld;
use bevy_ecs::{event::EventRegistry, prelude::*};

pub trait EventManager {
    /// Initializes handling of events of type `T` by inserting an [event queue resource](Events<T>)
    /// and scheduling [`event_update_system`](crate::event::event_update_system) to run early on in
    /// the system schedule.  See [`Events`] for more information on how to define and use events.
    fn add_event<T: Event>(&mut self) -> &mut Self;
}

impl<W> EventManager for W
where
    W: ContainsWorld,
{
    fn add_event<T: Event>(&mut self) -> &mut Self {
        if !self.world().contains_resource::<Events<T>>() {
            EventRegistry::register_event::<T>(self.world_mut());
        }
        self
    }
}

// End of File
