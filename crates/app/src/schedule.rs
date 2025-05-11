// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::Plugin;
use ecs::{
    prelude::*,
    schedule::{ExecutorKind, InternedScheduleLabel, ScheduleLabel},
};

/// Plugin to set up the main schedule for the application.  This schedule is run every iteration of
/// the main loop, and is responsible for running the startup and update phases of the application.
/// The order in which systems are run is determined by the internal `MainScheduleOrder` resource,
/// which specifies a fixed sequence of schedules to run.
///
/// - [`PreStartup`], [`Startup`], and [`PostStartup`] are run once on the first iteration of the
///   main loop.
/// - [`First`], [`PreUpdate`], [`Update`], [`PostUpdate`], and [`Last`] are run on every iteration
///   of the main loop.
pub struct MainSchedulePlugin;

impl Plugin for MainSchedulePlugin {
    fn register(&self, app: &mut crate::App) {
        let mut schedules = app.resource_mut::<Schedules>();

        let main_schedule_order = MainScheduleOrder::default();
        for label in main_schedule_order.run_once.iter() {
            let label = label.intern();
            if !schedules.contains(label) {
                let new_schedule = Schedule::new(label);
                schedules.insert(new_schedule);
            }
        }
        for label in main_schedule_order.run_always.iter() {
            let label = label.intern();
            if !schedules.contains(label) {
                let new_schedule = Schedule::new(label.intern());
                schedules.insert(new_schedule);
            }
        }

        let mut main = Schedule::new(Main);
        main.set_executor_kind(ExecutorKind::SingleThreaded);
        schedules.insert(main);

        app.set_update_schedule(Main)
            .add_resource(main_schedule_order)
            .add_systems(Main, Main::run_main);
    }
}

#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Main;

impl Main {
    pub(crate) fn run_main(world: &mut World) {
        world.resource_scope(|world, order: Mut<MainScheduleOrder>| {
            // Run the schedules in the order specified by the `MainScheduleOrder` resource.
            for label in order.run_once.iter() {
                if let Err(e) = world.try_run_schedule(*label) {
                    log::error!("Error running schedule {label:?}: {e:?}");
                }
            }
            for label in order.run_always.iter() {
                if let Err(e) = world.try_run_schedule(*label) {
                    log::error!("Error running schedule {label:?}: {e:?}");
                }
            }

            // Clear the `run_once` list so that these systems are not run again the next time.
            if !order.run_once.is_empty() {
                std::mem::take(&mut order.into_inner().run_once);
            }
        });
    }
}

/// Schedule: run once on first iteration of the main loop, before [`Startup`] or [`PostStartup`].
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreStartup;

/// Schedule: run once on first iteration of the main loop, after [`PreStartup`] and before [`PostStartup`].
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Startup;

/// Schedule: run once on first iteration of the main loop, after [`PreStartup`] and [`Startup`].
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PostStartup;

/// Schedule: run every iteration of the main loop, before any other systems.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct First;

/// Schedule: run every iteration of the main loop, after [`First`] and before [`Update`] or [`PostUpdate`].
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreUpdate;

/// Schedule: run every iteration of the main loop, after [`PreUpdate`] and before [`PostUpdate`].
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Update;

/// Schedule: run every iteration of the main loop, after [`Update`] and before [`Last`].
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PostUpdate;

/// Schedule: run every iteration of the main loop, after all other systems.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Last;

#[derive(Resource)]
pub struct MainScheduleOrder {
    /// The labels to run for the startup phase of the [`Main`] schedule, in the order in which they
    /// will be run.  Once run, they will be removed from this list so as to not run on future
    /// iterations of the main loop runner.
    pub run_once: Vec<InternedScheduleLabel>,
    /// The labels to run for the update phase of the [`Main`] schedule, in the order in which they
    /// will be run.  These will be run every iteration of the main loop runner.
    pub run_always: Vec<InternedScheduleLabel>,
}

impl Default for MainScheduleOrder {
    fn default() -> Self {
        Self {
            run_once: vec![PreStartup.intern(), Startup.intern(), PostStartup.intern()],
            run_always: vec![
                First.intern(),
                PreUpdate.intern(),
                Update.intern(),
                PostUpdate.intern(),
                Last.intern(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use std::sync::{Arc, Mutex};

    /// Verify the execution order of schedules in a single App run.
    #[test]
    fn test_schedule_execution_order() {
        // Record the order in which schedules are executed
        let execution_order = Arc::new(Mutex::new(Vec::new()));

        // Create our app with the MainSchedulePlugin
        let mut app = App::new("ScheduleOrderTest".into());

        // Add a tracing system to each schedule to track execution order.
        // Note: We need to use each schedule as a separate variable to avoid type mismatches.
        let order = execution_order.clone();
        app.add_systems(PreStartup, move || {
            order.lock().unwrap().push("PreStartup".to_string());
        });

        let order = execution_order.clone();
        app.add_systems(Startup, move || {
            order.lock().unwrap().push("Startup".to_string());
        });

        let order = execution_order.clone();
        app.add_systems(PostStartup, move || {
            order.lock().unwrap().push("PostStartup".to_string());
        });

        let order = execution_order.clone();
        app.add_systems(First, move || {
            order.lock().unwrap().push("First".to_string());
        });

        let order = execution_order.clone();
        app.add_systems(PreUpdate, move || {
            order.lock().unwrap().push("PreUpdate".to_string());
        });

        let order = execution_order.clone();
        app.add_systems(Update, move || {
            order.lock().unwrap().push("Update".to_string());
        });

        let order = execution_order.clone();
        app.add_systems(PostUpdate, move || {
            order.lock().unwrap().push("PostUpdate".to_string());
        });

        let order = execution_order.clone();
        app.add_systems(Last, move || {
            order.lock().unwrap().push("Last".to_string());
        });

        // Run the app once - this should trigger one full execution of all schedules
        app.update();

        // Verify all schedules ran in the correct order
        let mut execution = execution_order.lock().unwrap();

        // Verify the schedules actually ran (this helps debug if test is hanging)
        assert!(!execution.is_empty(), "No schedules were executed");
        assert_eq!(execution.len(), 8);

        // Verify the three startup schedules ran in order
        assert_eq!(execution[0], "PreStartup");
        assert_eq!(execution[1], "Startup");
        assert_eq!(execution[2], "PostStartup");

        // Verify the five regular schedules ran in order
        assert_eq!(execution[3], "First");
        assert_eq!(execution[4], "PreUpdate");
        assert_eq!(execution[5], "Update");
        assert_eq!(execution[6], "PostUpdate");
        assert_eq!(execution[7], "Last");

        // Clear the execution log
        execution.clear();

        // Drop mutex guard to release lock
        drop(execution);

        // Run the app again - only the "run_always" schedules should execute
        app.update();

        // Only run_always schedules ran this time
        let execution = execution_order.lock().unwrap();
        assert_eq!(execution.len(), 5); // Just the 5 regular schedules

        // Verify the five regular schedules ran in order
        assert_eq!(execution[0], "First");
        assert_eq!(execution[1], "PreUpdate");
        assert_eq!(execution[2], "Update");
        assert_eq!(execution[3], "PostUpdate");
        assert_eq!(execution[4], "Last");
    }

    /// Demonstrate when to use each schedule type through a practical example.
    #[test]
    fn test_practical_schedule_usage() {
        #[derive(Resource, Default)]
        struct GameState {
            initialized: bool,
            assets_loaded: bool,
            players_count: usize,
            frame_count: usize,
            physics_updates: usize,
            render_updates: usize,
            cleanup_runs: usize,
        }

        let mut app = App::new("GameScheduleDemo".into());
        app.insert_resource(GameState::default());

        // PreStartup: Initialize fundamental systems
        app.add_systems(PreStartup, |mut state: ResMut<GameState>| {
            // Initialize core engine systems
            state.initialized = true;
        });

        // Startup: Load game assets, create initial entities
        app.add_systems(Startup, |mut state: ResMut<GameState>| {
            // Load assets, initialize basic game state
            state.assets_loaded = true;
            state.players_count = 2;
        });

        // PostStartup: Setup game world after all assets are loaded
        app.add_systems(PostStartup, |state: Res<GameState>| {
            // Validate initial state
            assert!(state.initialized);
            assert!(state.assets_loaded);
            assert_eq!(state.players_count, 2);
        });

        // First: Run at the beginning of each frame
        app.add_systems(First, |mut state: ResMut<GameState>| {
            // Start a new frame, collect input
            state.frame_count += 1;
        });

        // PreUpdate: Prepare for main game update
        app.add_systems(PreUpdate, |state: Res<GameState>| {
            // Prepare for physics update
            assert!(state.initialized);
            assert!(state.assets_loaded);
        });

        // Update: Main game logic
        app.add_systems(Update, |mut state: ResMut<GameState>| {
            // Run main game logic, physics, AI, etc.
            state.physics_updates += 1;
        });

        // PostUpdate: Handle things that depend on the main update
        app.add_systems(PostUpdate, |mut state: ResMut<GameState>| {
            // Prepare rendering, handle events created during Update
            state.render_updates += 1;
        });

        // Last: Final cleanup at end of frame
        app.add_systems(Last, |mut state: ResMut<GameState>| {
            // End-of-frame operations, cleanup
            state.cleanup_runs += 1;
        });

        // Make sure we're using the Main schedule to run our app
        app.set_update_schedule(Main);

        // Run the first frame - all schedules should execute
        app.update();

        // Verify all systems ran correctly
        let state = app.resource::<GameState>();
        assert!(state.initialized);
        assert!(state.assets_loaded);
        assert_eq!(state.players_count, 2);
        assert_eq!(state.frame_count, 1);
        assert_eq!(state.physics_updates, 1);
        assert_eq!(state.render_updates, 1);
        assert_eq!(state.cleanup_runs, 1);

        // Run a second frame - only the regular schedules should run
        app.update();

        // Check that startup schedules didn't run again
        let state = app.resource::<GameState>();
        assert_eq!(state.frame_count, 2); // Incremented again
        assert_eq!(state.physics_updates, 2); // Incremented again
        assert_eq!(state.render_updates, 2); // Incremented again
        assert_eq!(state.cleanup_runs, 2); // Incremented again
    }

    /// System execution order can be configured with SystemSets.
    #[test]
    fn test_system_ordering_within_schedule() {
        // Define system sets to control execution order
        #[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
        enum GameSystems {
            Input,
            Physics,
            Rendering,
        }

        // Track execution order
        let execution_order = Arc::new(Mutex::new(Vec::new()));

        let mut app = App::new("SystemOrderingTest".into());

        // Add systems in reverse order to prove ordering works regardless of add order
        // Add rendering system first
        let order = execution_order.clone();
        app.add_systems(
            Update,
            (move || {
                order.lock().unwrap().push("Rendering".to_string());
            })
            .in_set(GameSystems::Rendering),
        );

        // Add physics system second
        let order = execution_order.clone();
        app.add_systems(
            Update,
            (move || {
                order.lock().unwrap().push("Physics".to_string());
            })
            .in_set(GameSystems::Physics),
        );

        // Add input system last
        let order = execution_order.clone();
        app.add_systems(
            Update,
            (move || {
                order.lock().unwrap().push("Input".to_string());
            })
            .in_set(GameSystems::Input),
        );

        // Establish dependencies between system sets using direct access to the schedule.
        // Make input run before physics, and physics run before rendering.
        {
            let mut schedules = app.world_mut().resource_mut::<Schedules>();
            if let Some(update_schedule) = schedules.get_mut(Update.intern()) {
                // Set up the chain: Input -> Physics -> Rendering
                update_schedule.configure_sets(
                    (
                        GameSystems::Input,
                        GameSystems::Physics,
                        GameSystems::Rendering,
                    )
                        .chain(),
                );
            }
        }

        // Run the app once to execute all the systems
        app.update();

        // Check that the systems ran in the correct order
        let execution = execution_order.lock().unwrap();
        assert_eq!(execution.len(), 3);
        assert_eq!(execution[0], "Input");
        assert_eq!(execution[1], "Physics");
        assert_eq!(execution[2], "Rendering");
    }

    /// Create a custom schedule ordering.
    #[test]
    fn test_custom_schedule_order() {
        // Define a custom schedule
        #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
        struct CustomSchedule;

        // Record execution order
        let execution_order = Arc::new(Mutex::new(Vec::new()));

        // Create an empty app to avoid MainSchedulePlugin, a default plugin.
        let mut app = App::empty("CustomScheduleTest".into());

        // Create our own schedule setup instead of using MainSchedulePlugin
        let mut schedules = Schedules::default();

        // Create a custom main schedule
        let main = Schedule::new(Main);
        schedules.insert(main);

        // Add our custom schedule
        let custom = Schedule::new(CustomSchedule);
        schedules.insert(custom);

        // Replace the default schedule
        app.insert_resource(schedules);
        app.set_update_schedule(Main);

        // Add our own main schedule runner
        let order = execution_order.clone();
        app.add_systems(Main, move |world: &mut World| {
            order.lock().unwrap().push("MainSchedule".to_string());

            // Manually run the custom schedule
            world
                .try_run_schedule(CustomSchedule)
                .expect("Error running custom schedule");
        });

        // Add a system to our custom schedule
        let order = execution_order.clone();
        app.add_systems(CustomSchedule, move || {
            order.lock().unwrap().push("CustomSchedule".to_string());
        });

        // Run the app
        app.update();

        // Verify execution order
        let execution = execution_order.lock().unwrap();
        assert_eq!(execution.len(), 2);
        assert_eq!(execution[0], "MainSchedule");
        assert_eq!(execution[1], "CustomSchedule");
    }
}

// End of File
