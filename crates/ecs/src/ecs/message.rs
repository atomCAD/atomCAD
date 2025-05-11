// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use super::ContainsWorld;
use bevy_ecs::{message::MessageRegistry, prelude::*};

pub trait MessageManager {
    /// Initializes handling of messages of type `T` by inserting an [message queue resource](Messages<T>)
    /// and scheduling [`message_update_system`](crate::message::message_update_system) to run early on in
    /// the system schedule.  See [`Messages`] for more information on how to define and use messages.
    fn add_message<T: Message>(&mut self) -> &mut Self;
}

impl<W> MessageManager for W
where
    W: ContainsWorld,
{
    fn add_message<T: Message>(&mut self) -> &mut Self {
        if !self.world().contains_resource::<Messages<T>>() {
            MessageRegistry::register_message::<T>(self.world_mut());
        }
        self
    }
}

// End of File
