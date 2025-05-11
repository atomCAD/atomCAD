// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use super::ContainsWorld;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicI32, AtomicUsize, Ordering},
        Arc,
    };

    /// A simple test event with no data
    #[derive(Event)]
    struct TestEvent;

    /// An event carrying data
    #[derive(Event)]
    struct DataEvent {
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

        // For testing event sending
        fn send_event<T: Event>(&mut self, event: T) {
            let mut events = self.world.resource_mut::<Events<T>>();
            events.send(event);
        }

        // Add a system that runs every update
        fn add_system<M>(&mut self, system: impl IntoSystem<(), (), M>) {
            self.schedule.add_systems(system);
        }

        // Run a single update of the schedule
        fn update(&mut self) {
            self.schedule.run(&mut self.world);
        }

        // Update the event buffers to simulate frame boundaries
        fn update_events<T: Event>(&mut self) {
            let mut events = self.world.resource_mut::<Events<T>>();
            events.update();
        }

        // For testing event receiving in a one-shot system
        fn run_system<S: IntoSystem<(), (), Marker>, Marker>(&mut self, system: S) {
            let mut system = IntoSystem::into_system(system);
            system.initialize(&mut self.world);
            system.run((), &mut self.world);
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

    // Helper function to count events using a fresh reader
    fn count_events<T: Event>(app: &mut TestApp) -> usize {
        let event_count = Arc::new(AtomicUsize::new(0));
        let count = event_count.clone();

        app.run_system(move |mut events: EventReader<T>| {
            let count_seen = events.read().count();
            count.fetch_add(count_seen, Ordering::Relaxed);
        });

        event_count.load(Ordering::Relaxed)
    }

    /// Tests basic event registration and checks that the resource exists
    ///
    /// WHY: This test verifies the foundational behavior of EventManager:
    /// ensuring that calling add_event() correctly creates the Events<T> resource.
    /// This is critical because without this resource, events can't be sent or received.
    #[test]
    fn test_basic_event_registration() {
        let mut app = TestApp::new();

        app.add_event::<TestEvent>();
        assert!(app.world.contains_resource::<Events<TestEvent>>());

        // Adding the same event type again should be harmless
        app.add_event::<TestEvent>();
        assert!(app.world.contains_resource::<Events<TestEvent>>());
    }

    /// Tests registering multiple different event types
    ///
    /// WHY: In a real application, you'll typically have many different event types.
    /// This test ensures that the EventManager can handle multiple event types,
    /// which is essential for a functioning event system.
    #[test]
    fn test_multiple_event_types() {
        let mut app = TestApp::new();

        app.add_event::<TestEvent>().add_event::<DataEvent>();

        assert!(app.world.contains_resource::<Events<TestEvent>>());
        assert!(app.world.contains_resource::<Events<DataEvent>>());
    }

    /// Tests the basic send/receive event flow with a one-shot system
    ///
    /// WHY: This test demonstrates the fundamental event flow: registration → sending → receiving.
    /// It validates that events not only get registered but can actually be used for communication,
    /// which is their primary purpose.
    #[test]
    fn test_send_receive_events() {
        let mut app = TestApp::new();
        app.add_event::<DataEvent>();

        // Send a few events
        app.send_event(DataEvent { value: 42 });
        app.send_event(DataEvent { value: 100 });

        // Count events and accumulate values
        let count = Arc::new(AtomicI32::new(0));
        let sum = Arc::new(AtomicI32::new(0));

        let count_clone = count.clone();
        let sum_clone = sum.clone();
        app.run_system(move |mut events: EventReader<DataEvent>| {
            for event in events.read() {
                count_clone.fetch_add(1, Ordering::Relaxed);
                sum_clone.fetch_add(event.value, Ordering::Relaxed);
            }
        });

        assert_eq!(count.load(Ordering::Relaxed), 2);
        assert_eq!(sum.load(Ordering::Relaxed), 142); // 42 + 100
    }

    /// Tests how events are visible to systems across frames
    ///
    /// WHY: This test demonstrates how events flow through Bevy's double-buffered event system
    /// and how reader state is tracked by systems. Understanding this is crucial for proper
    /// event handling in games where events might be produced and consumed across frame boundaries.
    #[test]
    fn test_event_visibility_across_frames() {
        let mut app = TestApp::new();
        app.add_event::<TestEvent>();

        // Set up a persistent system that counts events
        let seen_events = Arc::new(AtomicI32::new(0));
        let seen_clone = seen_events.clone();
        app.add_system(move |mut events: EventReader<TestEvent>| {
            for _ in events.read() {
                seen_clone.fetch_add(1, Ordering::Relaxed);
            }
        });

        // ---- FRAME 1 ----
        // Send an event and run the system
        app.send_event(TestEvent);

        // Run the system - it sees the event
        app.update();
        assert_eq!(seen_events.load(Ordering::Relaxed), 1);

        // A fresh reader also sees the event
        let count = count_events::<TestEvent>(&mut app);
        assert_eq!(count, 1);

        // Swap buffers - moving first event to previous buffer
        app.update_events::<TestEvent>();

        // A fresh reader can still see the event in the previous buffer
        let count = count_events::<TestEvent>(&mut app);
        assert_eq!(count, 1);

        // ---- FRAME 2 ----
        // Send a second event
        app.send_event(TestEvent);

        // A fresh reader sees both events
        // (first event in previous buffer, second event in current buffer)
        let count = count_events::<TestEvent>(&mut app);
        assert_eq!(count, 2);

        // Swap buffers
        // - First event is dropped
        // - Second event moves to previous buffer
        app.update_events::<TestEvent>();

        // A fresh reader can now only see the second event
        let count = count_events::<TestEvent>(&mut app);
        assert_eq!(count, 1);

        // Run the persistent system - it only sees the second event
        // (it already saw the first event in Frame 1)
        app.update();
        assert_eq!(seen_events.load(Ordering::Relaxed), 2);

        // ---- FRAME 3 ----
        // Send a third event
        app.send_event(TestEvent);

        // A fresh reader sees both the second event (in previous buffer)
        // and the third event (in current buffer)
        let count = count_events::<TestEvent>(&mut app);
        assert_eq!(count, 2);

        // Swap buffers
        // - Second event is dropped
        // - Third event moves to previous buffer
        app.update_events::<TestEvent>();

        // A fresh reader only sees the third event now
        let count = count_events::<TestEvent>(&mut app);
        assert_eq!(count, 1);

        // Run persistent system - it only sees the third event
        app.update();
        assert_eq!(seen_events.load(Ordering::Relaxed), 3);

        // ---- FRAME 4 ----
        // No new events are sent

        // Final buffer swap (third event is dropped)
        app.update_events::<TestEvent>();

        // A fresh reader sees no events
        let count = count_events::<TestEvent>(&mut app);
        assert_eq!(count, 0);

        // Run persistent system - it doesn't see any new events
        app.update();
        assert_eq!(seen_events.load(Ordering::Relaxed), 3);
    }

    /// Tests that multiple systems can all observe the same events
    ///
    /// WHY: Events must work reliably for multiple observers. This test confirms
    /// that different systems can all observe the same events independently,
    /// demonstrating the broadcast nature of the event system.
    #[test]
    fn test_multiple_systems() {
        let mut app = TestApp::new();
        app.add_event::<TestEvent>();

        // Set up two independent systems
        let system1_count = Arc::new(AtomicI32::new(0));
        let system2_count = Arc::new(AtomicI32::new(0));

        let s1_count = system1_count.clone();
        app.add_system(move |mut events: EventReader<TestEvent>| {
            for _ in events.read() {
                s1_count.fetch_add(1, Ordering::Relaxed);
            }
        });

        let s2_count = system2_count.clone();
        app.add_system(move |mut events: EventReader<TestEvent>| {
            for _ in events.read() {
                s2_count.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Send events and update
        app.send_event(TestEvent);
        app.send_event(TestEvent);
        app.update();

        // Both systems should see both events
        assert_eq!(system1_count.load(Ordering::Relaxed), 2);
        assert_eq!(system2_count.load(Ordering::Relaxed), 2);
    }
}

// End of File
