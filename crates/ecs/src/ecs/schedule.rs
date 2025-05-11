// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use super::ContainsWorld;
use bevy_ecs::{prelude::*, schedule::ScheduleLabel, world::error::TryRunScheduleError};

/// Schedules are stored within the [`Schedules`] resource in the [`World`].
pub trait ScheduleManager {
    /// Initializes a new [`Schedule`] with the given label.  If a schedule with the same label
    /// already exists, this method will do nothing.
    fn init_schedule(&mut self, label: impl ScheduleLabel) -> &mut Self;

    /// Inserts a new [`Schedule`], returning the old schedule if it existed.
    fn insert_schedule(&mut self, schedule: Schedule) -> Option<Schedule>;

    /// Removes the [`Schedule`] with the provided `label`, returning it if it existed.
    fn remove_schedule(&mut self, label: impl ScheduleLabel) -> Option<Schedule>;

    /// Gets a read-only reference to the [`Schedule`] with the provided `label`, if it exists.
    fn get_schedule(&self, label: impl ScheduleLabel) -> Option<&Schedule>;

    /// Gets a mutable reference to the [`Schedule`] with the provided `label`, if it exists.
    fn get_schedule_mut(&mut self, label: impl ScheduleLabel) -> Option<&mut Schedule>;

    /// Temporarily removes the [`Schedule`] with the provided `label` from the ECS [`World`], runs
    /// the provided closure, re-adds the schedule to the world, and returns the result of the
    /// closure to the callee.
    ///
    /// # Errors
    /// Return [`TryRunScheduleError`] if a [`Schedule`] with the provided `label` does not exist.
    fn schedule_scope<U>(
        &mut self,
        label: impl ScheduleLabel,
        f: impl FnOnce(&mut World, &mut Schedule) -> U,
    ) -> Result<U, TryRunScheduleError>;

    /// Checks if a [`Schedule`] with the provided `label` exists.
    fn contains_schedule(&self, label: impl ScheduleLabel) -> bool {
        self.get_schedule(label).is_some()
    }

    /// Gets an immutable reference to the [`Schedule`] with the provided `label`.
    ///
    /// # Panics
    /// Panics if there is no [`Schedule`] with the provided `label`.
    fn schedule(&self, label: impl ScheduleLabel) -> &Schedule {
        let label = label.intern();
        self.get_schedule(label).unwrap_or_else(|| {
            panic!(
                "Requested schedule with label {:?} not found",
                label.intern()
            )
        })
    }

    /// Gets a mutable reference to the [`Schedule`] with the provided `label`.
    ///
    /// # Panics
    /// Panics if there is no [`Schedule`] with the provided `label`.
    fn schedule_mut(&mut self, label: impl ScheduleLabel) -> &mut Schedule {
        let label = label.intern();
        self.get_schedule_mut(label).unwrap_or_else(|| {
            panic!(
                "Requested schedule with label {:?} not found",
                label.intern()
            )
        })
    }

    /// Adds the passed-in [`Schedule`].
    ///
    /// # Warning
    /// This method will silently overwrite (and drop) any existing schedule with the same label. To
    /// avoid this behavior use [`init_schedule`](ScheduleManager::init_schedule) or
    /// [`insert_schedule`](ScheduleManager::insert_schedule) instead.
    fn add_schedule(&mut self, schedule: Schedule) -> &mut Self {
        if let Some(old) = self.insert_schedule(schedule) {
            log::debug!("Overwriting existing schedule with label {:?}", old.label());
        }
        self
    }

    /// Applies the provided closure to the [`Schedule`] (read-only) with the given `label`.
    ///
    /// **Note:** If the schedule does not exist, it will be created.
    fn visit_schedule(
        &mut self,
        label: impl ScheduleLabel,
        f: impl FnOnce(&Schedule),
    ) -> &mut Self {
        let label = label.intern();
        let schedule = match self.get_schedule(label) {
            Some(schedule) => schedule,
            None => self.init_schedule(label).get_schedule(label).unwrap(),
        };
        f(schedule);
        self
    }

    /// Applies the provided closure to the [`Schedule`] (mutable) with the given `label`.
    ///
    /// **Note:** If the schedule does not exist, it will be created.
    fn edit_schedule(
        &mut self,
        label: impl ScheduleLabel,
        f: impl FnOnce(&mut Schedule),
    ) -> &mut Self {
        // If the schedule doesn't exist, create it.
        let label = label.intern();
        let schedule = match self.get_schedule_mut(label) {
            Some(schedule) => schedule,
            None => self.init_schedule(label).get_schedule_mut(label).unwrap(),
        };

        // Call the provided closure with a mutable reference to the schedule.
        f(schedule);

        self
    }
}

impl<T> ScheduleManager for T
where
    T: ContainsWorld,
{
    fn init_schedule(&mut self, label: impl ScheduleLabel) -> &mut Self {
        let label = label.intern();
        let mut schedules = self.world_mut().resource_mut::<Schedules>();
        if !schedules.contains(label) {
            schedules.insert(Schedule::new(label));
        }
        self
    }

    fn insert_schedule(&mut self, schedule: Schedule) -> Option<Schedule> {
        let mut schedules = self.world_mut().resource_mut::<Schedules>();
        schedules.insert(schedule)
    }

    fn get_schedule(&self, label: impl ScheduleLabel) -> Option<&Schedule> {
        let schedules = self.world().resource::<Schedules>();
        schedules.get(label)
    }

    fn get_schedule_mut(&mut self, label: impl ScheduleLabel) -> Option<&mut Schedule> {
        let schedules = self.world_mut().resource_mut::<Schedules>();
        // The [`Mut`] smart point is designed to not be automatically dereferenced by the Rust
        // borrow checker, as doing so has a side effect (updating the last-modified tick).
        schedules.into_inner().get_mut(label)
    }

    fn remove_schedule(&mut self, label: impl ScheduleLabel) -> Option<Schedule> {
        let label = label.intern();
        let mut schedules = self.world_mut().resource_mut::<Schedules>();
        schedules.remove(label)
    }

    fn schedule_scope<U>(
        &mut self,
        label: impl ScheduleLabel,
        f: impl FnOnce(&mut World, &mut Schedule) -> U,
    ) -> Result<U, TryRunScheduleError> {
        self.world_mut().try_schedule_scope(label, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::AssertUnwindSafe;

    /// Custom schedule labels for testing
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    enum TestSchedule {
        Setup,
        Update,
        Render,
    }

    /// Simple test system that increments a counter
    #[derive(Resource, Default)]
    struct Counter {
        count: usize,
    }

    // For use as a system
    fn increment_counter(mut counter: ResMut<Counter>) {
        counter.count += 1;
    }

    /// A test app implementation that satisfies ContainsWorld
    struct TestApp {
        world: World,
    }

    impl TestApp {
        fn new() -> Self {
            let mut world = World::new();
            // All worlds need a Schedules resource to work with ScheduleManager
            world.init_resource::<Schedules>();
            Self { world }
        }

        fn run_schedule(&mut self, label: impl ScheduleLabel) {
            self.world.run_schedule(label);
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

    /// Tests basic schedule initialization
    ///
    /// WHY: This test verifies that init_schedule correctly creates a new schedule
    /// only when one doesn't exist. This ensures initialization is idempotent,
    /// protecting existing schedules from being overwritten accidentally.
    #[test]
    fn test_init_schedule() {
        let mut app = TestApp::new();

        // Schedule doesn't exist yet
        assert!(!app.contains_schedule(TestSchedule::Update));

        // First initialization creates it
        app.init_schedule(TestSchedule::Update);
        assert!(app.contains_schedule(TestSchedule::Update));

        // Add a system to the schedule so we can detect if it gets reset
        app.world_mut().init_resource::<Counter>();
        app.edit_schedule(TestSchedule::Update, |schedule| {
            schedule.add_systems(increment_counter);
        });

        // Second initialization shouldn't create a new schedule or reset the existing one
        app.init_schedule(TestSchedule::Update);
        assert!(app.contains_schedule(TestSchedule::Update));

        // Verify the system we added is still there by running the schedule
        assert_eq!(app.world().resource::<Counter>().count, 0);
        app.run_schedule(TestSchedule::Update);
        assert_eq!(app.world().resource::<Counter>().count, 1);
    }

    /// Tests inserting schedules with different approaches
    ///
    /// WHY: This test demonstrates the difference between insert_schedule and add_schedule.
    /// Understanding this distinction helps prevent accidentally losing schedules.
    #[test]
    fn test_insert_add_schedule() {
        let mut app = TestApp::new();

        // Insert returns None for a new schedule
        let schedule = Schedule::new(TestSchedule::Update);
        let previous = app.insert_schedule(schedule);
        assert!(previous.is_none());
        assert!(app.contains_schedule(TestSchedule::Update));

        // Insert returns the previous schedule when one exists
        let new_schedule = Schedule::new(TestSchedule::Update);
        let previous = app.insert_schedule(new_schedule);
        assert!(previous.is_some());
        assert_eq!(
            previous.unwrap().label().intern(),
            TestSchedule::Update.intern()
        );

        // Add_schedule also replaces but doesn't return the previous schedule
        let schedule = Schedule::new(TestSchedule::Render);
        app.add_schedule(schedule);
        assert!(app.contains_schedule(TestSchedule::Render));

        // Chaining works with add_schedule
        app.add_schedule(Schedule::new(TestSchedule::Setup))
            .add_schedule(Schedule::new(TestSchedule::Update));
        assert!(app.contains_schedule(TestSchedule::Setup));
        assert!(app.contains_schedule(TestSchedule::Update));
    }

    /// Tests removing schedules
    ///
    /// WHY: Schedule removal is important for cleanup and memory management,
    /// especially when transitioning between different states in an application.
    #[test]
    fn test_remove_schedule() {
        let mut app = TestApp::new();

        // Add a schedule first
        app.add_schedule(Schedule::new(TestSchedule::Update));
        assert!(app.contains_schedule(TestSchedule::Update));

        // Remove returns the schedule and it's no longer in the world
        let removed = app.remove_schedule(TestSchedule::Update);
        assert!(removed.is_some());
        assert_eq!(
            removed.unwrap().label().intern(),
            TestSchedule::Update.intern()
        );
        assert!(!app.contains_schedule(TestSchedule::Update));

        // Remove on a non-existent schedule returns None
        let not_found = app.remove_schedule(TestSchedule::Update);
        assert!(not_found.is_none());
    }

    /// Tests accessing schedules with different methods
    ///
    /// WHY: There are multiple ways to access schedules, with different error handling
    /// behaviors. Understanding these differences helps prevent panics in production.
    #[test]
    fn test_schedule_access() {
        let mut app = TestApp::new();

        // Add a schedule to work with
        app.add_schedule(Schedule::new(TestSchedule::Update));

        // get_schedule returns Option<&Schedule>
        let schedule = app.get_schedule(TestSchedule::Update);
        assert!(schedule.is_some());
        assert_eq!(
            schedule.unwrap().label().intern(),
            TestSchedule::Update.intern()
        );

        // schedule returns &Schedule directly but panics if not found
        let schedule = app.schedule(TestSchedule::Update);
        assert_eq!(schedule.label().intern(), TestSchedule::Update.intern());

        // Accessing a non-existent schedule via get_schedule returns None
        assert!(app.get_schedule(TestSchedule::Render).is_none());

        // Visit_schedule safely creates the schedule if it doesn't exist
        let mut visited = false;
        app.visit_schedule(TestSchedule::Render, |schedule| {
            visited = true;
            assert_eq!(schedule.label().intern(), TestSchedule::Render.intern());
        });
        assert!(visited);
        assert!(app.contains_schedule(TestSchedule::Render));

        // Demonstrate panic behavior on non-existent schedule
        // SAFETY: we don't use app after this point
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.schedule(TestSchedule::Setup);
        }));
        assert!(result.is_err());
    }

    /// Tests mutable schedule access
    ///
    /// WHY: Mutable schedule access is essential for adding systems and configuring
    /// how they run. Understanding these patterns helps build maintainable applications.
    #[test]
    fn test_mutable_schedule_access() {
        let mut app = TestApp::new();

        // Add a schedule to work with
        app.add_schedule(Schedule::new(TestSchedule::Update));
        app.world_mut().init_resource::<Counter>();

        // get_schedule_mut returns Option<&mut Schedule>
        if let Some(schedule) = app.get_schedule_mut(TestSchedule::Update) {
            schedule.add_systems(increment_counter);
        }
        app.run_schedule(TestSchedule::Update);
        assert_eq!(app.world().resource::<Counter>().count, 1);

        // schedule_mut returns &mut Schedule directly but panics if not found
        {
            let schedule = app.schedule_mut(TestSchedule::Update);
            // Add another increment system
            schedule.add_systems(increment_counter);
            // Drop mutable reference
        }
        app.run_schedule(TestSchedule::Update);
        assert_eq!(app.world().resource::<Counter>().count, 3);

        // edit_schedule provides safe modification and creates the schedule if needed
        app.edit_schedule(TestSchedule::Render, |schedule| {
            schedule.add_systems(increment_counter);
        });
        assert!(app.contains_schedule(TestSchedule::Render));
        app.run_schedule(TestSchedule::Render);
        assert_eq!(app.world().resource::<Counter>().count, 4);

        // edit_schedule on an existing schedule just modifies it
        app.edit_schedule(TestSchedule::Render, |schedule| {
            schedule.add_systems(increment_counter);
        });
        app.run_schedule(TestSchedule::Render);
        assert_eq!(app.world().resource::<Counter>().count, 6);

        // Demonstrate panic behavior
        // SAFETY: we don't use app after this point
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.schedule_mut(TestSchedule::Setup);
        }));
        assert!(result.is_err());
    }

    /// Tests schedule_scope for accessing schedule and world together
    ///
    /// WHY: Sometimes we need to modify both a schedule and the world at the same time.
    /// schedule_scope provides a safe way to do this by temporarily removing the schedule
    /// from the world to avoid borrowing conflicts.
    #[test]
    fn test_schedule_scope() {
        let mut app = TestApp::new();

        // Add necessary schedules and resources
        app.add_schedule(Schedule::new(TestSchedule::Update));
        app.world_mut().init_resource::<Counter>();

        // Use schedule_scope to safely modify both world and schedule
        let result = app.schedule_scope(TestSchedule::Update, |world, schedule| {
            // Can modify the schedule
            schedule.add_systems(increment_counter);

            // Can also modify the world
            let mut counter = world.resource_mut::<Counter>();
            counter.count = 5;

            // Can return a value from the scope
            "success"
        });

        // Successful result
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");

        // Check that the world changes took effect
        assert_eq!(app.world().resource::<Counter>().count, 5);

        // Check that schedule changes took effect by running it
        app.run_schedule(TestSchedule::Update);
        assert_eq!(app.world().resource::<Counter>().count, 6);

        // schedule_scope returns error for non-existent schedule
        let error_result = app.schedule_scope(TestSchedule::Render, |_, _| "should not get here");
        assert!(error_result.is_err());
    }
}

// End of File
