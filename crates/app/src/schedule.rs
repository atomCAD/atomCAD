// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

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

        app.add_resource(main_schedule_order)
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
                    log::error!("Error running schedule {:?}: {:?}", label, e);
                }
            }
            for label in order.run_always.iter() {
                if let Err(e) = world.try_run_schedule(*label) {
                    log::error!("Error running schedule {:?}: {:?}", label, e);
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

// End of File
