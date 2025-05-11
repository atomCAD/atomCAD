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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicI32, AtomicUsize, Ordering},
    };

    /// A simple test message with no data
    #[derive(Message)]
    struct TestMessage;

    /// An message carrying data
    #[derive(Message)]
    struct DataMessage {
        value: i32,
    }

    /// A world wrapper that implements ContainsWorld
    /// This mimics how a real application might wrap the ECS World
    struct TestApp {
        world: World,
        schedule: Schedule,
    }

    impl TestApp {
        fn new() -> Self {
            Self {
                world: World::new(),
                schedule: Schedule::default(),
            }
        }

        // For testing message sending
        fn send_message<T: Message>(&mut self, message: T) {
            let mut messages = self.world.resource_mut::<Messages<T>>();
            messages.write(message);
        }

        // Add a system that runs every update
        fn add_system<M>(&mut self, system: impl IntoSystem<(), (), M>) {
            self.schedule.add_systems(system);
        }

        // Run a single update of the schedule
        fn update(&mut self) {
            self.schedule.run(&mut self.world);
        }

        // Update the message buffers to simulate frame boundaries
        fn update_messages<T: Message>(&mut self) {
            let mut messages = self.world.resource_mut::<Messages<T>>();
            messages.update();
        }

        // For testing message receiving in a one-shot system
        fn run_system<S: IntoSystem<(), (), Marker>, Marker>(&mut self, system: S) {
            let mut system = IntoSystem::into_system(system);
            system.initialize(&mut self.world);
            system
                .run((), &mut self.world)
                .expect("System failed to run");
        }
    }

    impl ContainsWorld for TestApp {
        fn world(&self) -> &World {
            &self.world
        }

        fn world_mut(&mut self) -> &mut World {
            &mut self.world
        }
    }

    // Helper function to count messages using a fresh reader
    fn count_messages<T: Message>(app: &mut TestApp) -> usize {
        let message_count = Arc::new(AtomicUsize::new(0));
        let count = message_count.clone();

        app.run_system(move |mut messages: MessageReader<T>| {
            let count_seen = messages.read().count();
            count.fetch_add(count_seen, Ordering::Relaxed);
        });

        message_count.load(Ordering::Relaxed)
    }

    /// Tests basic message registration and checks that the resource exists
    ///
    /// WHY: This test verifies the foundational behavior of MessageManager:
    /// ensuring that calling add_message() correctly creates the Messages<T> resource.
    /// This is critical because without this resource, messages can't be sent or received.
    #[test]
    fn test_basic_message_registration() {
        let mut app = TestApp::new();

        app.add_message::<TestMessage>();
        assert!(app.world.contains_resource::<Messages<TestMessage>>());

        // Adding the same message type again should be harmless
        app.add_message::<TestMessage>();
        assert!(app.world.contains_resource::<Messages<TestMessage>>());
    }

    /// Tests registering multiple different message types
    ///
    /// WHY: In a real application, you'll typically have many different message types.
    /// This test ensures that the MessageManager can handle multiple message types,
    /// which is essential for a functioning message system.
    #[test]
    fn test_multiple_message_types() {
        let mut app = TestApp::new();

        app.add_message::<TestMessage>()
            .add_message::<DataMessage>();

        assert!(app.world.contains_resource::<Messages<TestMessage>>());
        assert!(app.world.contains_resource::<Messages<DataMessage>>());
    }

    /// Tests the basic send/receive message flow with a one-shot system
    ///
    /// WHY: This test demonstrates the fundamental message flow: registration → sending → receiving.
    /// It validates that messages not only get registered but can actually be used for communication,
    /// which is their primary purpose.
    #[test]
    fn test_send_receive_messages() {
        let mut app = TestApp::new();
        app.add_message::<DataMessage>();

        // Send a few messages
        app.send_message(DataMessage { value: 42 });
        app.send_message(DataMessage { value: 100 });

        // Count messages and accumulate values
        let count = Arc::new(AtomicI32::new(0));
        let sum = Arc::new(AtomicI32::new(0));

        let count_clone = count.clone();
        let sum_clone = sum.clone();
        app.run_system(move |mut messages: MessageReader<DataMessage>| {
            for message in messages.read() {
                count_clone.fetch_add(1, Ordering::Relaxed);
                sum_clone.fetch_add(message.value, Ordering::Relaxed);
            }
        });

        assert_eq!(count.load(Ordering::Relaxed), 2);
        assert_eq!(sum.load(Ordering::Relaxed), 142); // 42 + 100
    }

    /// Tests how messages are visible to systems across frames
    ///
    /// WHY: This test demonstrates how messages flow through Bevy's double-buffered message system
    /// and how reader state is tracked by systems. Understanding this is crucial for proper
    /// message handling in games where messages might be produced and consumed across frame boundaries.
    #[test]
    fn test_message_visibility_across_frames() {
        let mut app = TestApp::new();
        app.add_message::<TestMessage>();

        // Set up a persistent system that counts messages
        let seen_messages = Arc::new(AtomicI32::new(0));
        let seen_clone = seen_messages.clone();
        app.add_system(move |mut messages: MessageReader<TestMessage>| {
            for _ in messages.read() {
                seen_clone.fetch_add(1, Ordering::Relaxed);
            }
        });

        // ---- FRAME 1 ----
        // Send an message and run the system
        app.send_message(TestMessage);

        // Run the system - it sees the message
        app.update();
        assert_eq!(seen_messages.load(Ordering::Relaxed), 1);

        // A fresh reader also sees the message
        let count = count_messages::<TestMessage>(&mut app);
        assert_eq!(count, 1);

        // Swap buffers - moving first message to previous buffer
        app.update_messages::<TestMessage>();

        // A fresh reader can still see the message in the previous buffer
        let count = count_messages::<TestMessage>(&mut app);
        assert_eq!(count, 1);

        // ---- FRAME 2 ----
        // Send a second message
        app.send_message(TestMessage);

        // A fresh reader sees both messages
        // (first message in previous buffer, second message in current buffer)
        let count = count_messages::<TestMessage>(&mut app);
        assert_eq!(count, 2);

        // Swap buffers
        // - First message is dropped
        // - Second message moves to previous buffer
        app.update_messages::<TestMessage>();

        // A fresh reader can now only see the second message
        let count = count_messages::<TestMessage>(&mut app);
        assert_eq!(count, 1);

        // Run the persistent system - it only sees the second message
        // (it already saw the first message in Frame 1)
        app.update();
        assert_eq!(seen_messages.load(Ordering::Relaxed), 2);

        // ---- FRAME 3 ----
        // Send a third message
        app.send_message(TestMessage);

        // A fresh reader sees both the second message (in previous buffer)
        // and the third message (in current buffer)
        let count = count_messages::<TestMessage>(&mut app);
        assert_eq!(count, 2);

        // Swap buffers
        // - Second message is dropped
        // - Third message moves to previous buffer
        app.update_messages::<TestMessage>();

        // A fresh reader only sees the third message now
        let count = count_messages::<TestMessage>(&mut app);
        assert_eq!(count, 1);

        // Run persistent system - it only sees the third message
        app.update();
        assert_eq!(seen_messages.load(Ordering::Relaxed), 3);

        // ---- FRAME 4 ----
        // No new messages are sent

        // Final buffer swap (third message is dropped)
        app.update_messages::<TestMessage>();

        // A fresh reader sees no messages
        let count = count_messages::<TestMessage>(&mut app);
        assert_eq!(count, 0);

        // Run persistent system - it doesn't see any new messages
        app.update();
        assert_eq!(seen_messages.load(Ordering::Relaxed), 3);
    }

    /// Tests that multiple systems can all observe the same messages
    ///
    /// WHY: Messages must work reliably for multiple observers. This test confirms
    /// that different systems can all observe the same messages independently,
    /// demonstrating the broadcast nature of the message system.
    #[test]
    fn test_multiple_systems() {
        let mut app = TestApp::new();
        app.add_message::<TestMessage>();

        // Set up two independent systems
        let system1_count = Arc::new(AtomicI32::new(0));
        let system2_count = Arc::new(AtomicI32::new(0));

        let s1_count = system1_count.clone();
        app.add_system(move |mut messages: MessageReader<TestMessage>| {
            for _ in messages.read() {
                s1_count.fetch_add(1, Ordering::Relaxed);
            }
        });

        let s2_count = system2_count.clone();
        app.add_system(move |mut messages: MessageReader<TestMessage>| {
            for _ in messages.read() {
                s2_count.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Send messages and update
        app.send_message(TestMessage);
        app.send_message(TestMessage);
        app.update();

        // Both systems should see both messages
        assert_eq!(system1_count.load(Ordering::Relaxed), 2);
        assert_eq!(system2_count.load(Ordering::Relaxed), 2);
    }
}

// End of File
