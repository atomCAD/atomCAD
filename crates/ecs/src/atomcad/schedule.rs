// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use crate::atomcad::world::ContainsWorld;
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

// End of File
