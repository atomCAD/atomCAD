// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use crate::atomcad::world::ContainsWorld;
use bevy_ecs::prelude::*;

/// The `ResourceManager` trait provides an interface for managing resources in an ECS [`World`],
/// providing a standard way to initialize, insert, remove, get, and edit resources from another
/// object which contains an ECS [`World`].
pub trait ResourceManager {
    /// Initializes a new resource of type `R` with the default value.  If a resource of type `R`
    /// already exists, this method will do nothing.
    fn init_resource<R: Default + Resource>(&mut self) -> &mut Self;

    /// Inserts a new resource of type `R`, returning the existing resource of type `R`, if any.
    fn insert_resource<R: Resource>(&mut self, resource: R) -> Option<R>;

    /// Removes the resource of type `R`, returning it if it existed.
    fn remove_resource<R: Resource>(&mut self) -> Option<R>;

    /// Gets a read-only reference to the resource of type `R`, if it exists.
    fn get_resource<R: Resource>(&self) -> Option<&R>;

    /// Gets a mutable reference to the resource of type `R`, if it exists.
    fn get_resource_mut<R: Resource>(&mut self) -> Option<Mut<R>>;

    /// Temporarily removes the resource of type `R` from the ECS [`World`], runs the provided
    /// closure with the resource and (mutable) world as parameters, re-adds the resource to the
    /// [`World`], and returns the result of the closure to the callee.  This permits safe,
    /// simultaneous access to the resource and the rest of the [`World`].
    ///
    /// **Note:** If you're looking for the equivalent non-Send version of this method, it doesn't
    /// need to exist.  Systems accessing non-Send resources already run with exclusive access, and
    /// are permitted to have mutate [`World`] state simply by taking a input parameter of type
    /// `&mut World`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists.
    fn resource_scope<R: Resource, U>(&mut self, f: impl FnOnce(&mut World, Mut<R>) -> U) -> U;

    /// Checks if a resource of type `R` exists.
    fn contains_resource<R: Resource>(&self) -> bool {
        self.get_resource::<R>().is_some()
    }

    /// Gets an immutable reference to the resource of type `R`.
    ///
    /// # Panics
    /// Panics if there is no resource of type `R`.
    fn resource<R: Resource>(&self) -> &R {
        self.get_resource::<R>().unwrap_or_else(|| {
            panic!(
                "Requested resource of type {:?} not found",
                std::any::type_name::<R>()
            )
        })
    }

    /// Gets an immutable reference to the non-Send resource of type `R`.
    ///
    /// # Panics
    /// Panics if there is no resource of type `R`.
    fn resource_mut<R: Resource>(&mut self) -> Mut<R> {
        self.get_resource_mut::<R>().unwrap_or_else(|| {
            panic!(
                "Requested resource of type {:?} not found",
                std::any::type_name::<R>()
            )
        })
    }

    /// Adds the passed-in resource of type `R`, returning a mutable reference to `self` for
    /// chaining
    ///
    /// # Warning
    /// This method will silently overwrite (and drop) any existing resource of type `R`.  To avoid
    /// this behavior use [`init_resource`](ResourceManager::init_resource) or
    /// [`insert_resource`](ResourceManager::insert_resource) instead.
    fn add_resource<R: Resource>(&mut self, resource: R) -> &mut Self {
        if self.insert_resource(resource).is_some() {
            log::debug!(
                "Overwriting existing resource of type {:?}",
                std::any::type_name::<R>()
            );
        }
        self
    }

    /// Applies the provided closure to the resource of type `R` (read-only), returning a reference
    /// to `self` for chaining.
    ///
    /// **Note:** If the resource does not exist, the closure will not be called.
    fn visit_resource<R: Resource>(&self, f: impl FnOnce(&R)) -> &Self {
        if let Some(resource) = self.get_resource::<R>() {
            f(resource);
        } else {
            log::debug!(
                "Resource of type {:?} not found; unable to visit",
                std::any::type_name::<R>()
            );
        }
        self
    }

    /// Applies the provided closure to the resource of type `R` (mutable), returning a mutable
    /// reference to `self` for chaining.
    ///
    /// **Note:** If the resource does not exist, the closure will not be called.
    fn edit_resource<R: Resource>(&mut self, f: impl FnOnce(Mut<R>)) -> &mut Self {
        if let Some(resource) = self.get_resource_mut::<R>() {
            f(resource);
        } else {
            log::debug!(
                "Resource of type {:?} not found; unable to edit",
                std::any::type_name::<R>()
            );
        }
        self
    }
}

impl<T> ResourceManager for T
where
    T: ContainsWorld,
{
    fn init_resource<R: Default + Resource>(&mut self) -> &mut Self {
        self.world_mut().init_resource::<R>();
        self
    }

    fn insert_resource<R: Resource>(&mut self, resource: R) -> Option<R> {
        let world = self.world_mut();
        let obj = world.remove_resource::<R>();
        world.insert_resource(resource);
        obj
    }

    fn remove_resource<R: Resource>(&mut self) -> Option<R> {
        self.world_mut().remove_resource::<R>()
    }

    fn get_resource<R: Resource>(&self) -> Option<&R> {
        self.world().get_resource::<R>()
    }

    fn get_resource_mut<R: Resource>(&mut self) -> Option<Mut<R>> {
        self.world_mut().get_resource_mut::<R>()
    }

    fn resource_scope<R: Resource, U>(&mut self, f: impl FnOnce(&mut World, Mut<R>) -> U) -> U {
        self.world_mut().resource_scope(f)
    }
}

/// The `NonSendManager` trait provides an interface for managing non-[Send] resources in an ECS
/// [`World`], providing a standard way to initialize, insert, remove, get, and edit resources which
/// do not implement the [Send] and/or [Sync] traits, and therefore cannot be sent between threads.
pub trait NonSendManager {
    /// Initializes a new non-Send resource of type `R` with the default value.  If a non-Send
    /// resource of type `R` already exists, this method should do nothing.
    fn init_non_send<R: Default + 'static>(&mut self) -> &mut Self;

    /// Inserts a new non-Send resource of type `R`, returning the existing non-Send resource of
    /// type `R`, if any.
    fn insert_non_send<R: 'static>(&mut self, resource: R) -> Option<R>;

    /// Removes the non-Send resource of type `R`, returning it if it existed.
    fn remove_non_send<R: 'static>(&mut self) -> Option<R>;

    /// Gets a read-only reference to the non-Send resource of type `R`, if it exists.
    fn get_non_send<R: 'static>(&self) -> Option<&R>;

    /// Gets a mutable reference to the non-Send resource of type `R`, if it exists.
    fn get_non_send_mut<R: 'static>(&mut self) -> Option<Mut<R>>;

    /// Checks if a non-Send resource of type `R` exists.
    fn contains_non_send<R: 'static>(&self) -> bool {
        self.get_non_send::<R>().is_some()
    }

    /// Gets a read-only reference to the non-Send resource of type `R`.
    ///
    /// # Panics
    /// Panics if there is no non-Send resource of type `R`.
    fn non_send<R: Resource>(&self) -> &R {
        self.get_non_send::<R>().unwrap_or_else(|| {
            panic!(
                "Requested non-Send resource of type {:?} not found",
                std::any::type_name::<R>()
            )
        })
    }

    /// Gets a mutable reference to the non-Send resource of type `R`.
    ///
    /// # Panics
    /// Panics if there is no non-Send resource of type `R`.
    fn non_send_mut<R: Resource>(&mut self) -> Mut<R> {
        self.get_non_send_mut::<R>().unwrap_or_else(|| {
            panic!(
                "Requested non-Send resource of type {:?} not found",
                std::any::type_name::<R>()
            )
        })
    }

    /// Adds the passed-in non-Send resource of type `R`, returning a mutable reference to `self`
    /// for chaining purposes.
    ///
    /// # Warning
    /// This method will silently overwrite (and drop) any existing non-Send resource of type `R`.
    /// To avoid this behavior use [`init_non_send`](NonSendManager::init_non_send) or
    /// [`insert_non_send`](NonSendManager::insert_non_send) instead.
    fn add_non_send<R: 'static>(&mut self, resource: R) -> &mut Self {
        if self.insert_non_send(resource).is_some() {
            log::debug!(
                "Overwriting existing non-Send resource of type {:?}",
                std::any::type_name::<R>()
            );
        }
        self
    }

    /// Applies the provided closure to the non-Send resource of type `R` (read-only), returning an
    /// immutable reference to `self` for chaining purposes.
    fn visit_non_send<R: 'static>(&self, f: impl FnOnce(&R)) -> &Self {
        if let Some(resource) = self.get_non_send::<R>() {
            f(resource);
        } else {
            log::debug!(
                "Non-Send resource of type {:?} not found; unable to visit",
                std::any::type_name::<R>()
            );
        }
        self
    }

    /// Applies the provided closure to the non-Send resource of type `R` (mutable), returning a
    /// mutable reference to `self` for chaining purposes.
    fn edit_non_send<R: 'static>(&mut self, f: impl FnOnce(Mut<R>)) -> &mut Self {
        if let Some(resource) = self.get_non_send_mut::<R>() {
            f(resource);
        } else {
            log::debug!(
                "Non-Send resource of type {:?} not found; unable to edit",
                std::any::type_name::<R>()
            );
        }
        self
    }
}

impl<T> NonSendManager for T
where
    T: ContainsWorld,
{
    fn init_non_send<R: Default + 'static>(&mut self) -> &mut Self {
        self.world_mut().init_non_send_resource::<R>();
        self
    }

    fn insert_non_send<R: 'static>(&mut self, resource: R) -> Option<R> {
        let world = self.world_mut();
        let obj = world.remove_non_send_resource::<R>();
        world.insert_non_send_resource(resource);
        obj
    }

    fn remove_non_send<R: 'static>(&mut self) -> Option<R> {
        self.world_mut().remove_non_send_resource::<R>()
    }

    fn get_non_send<R: 'static>(&self) -> Option<&R> {
        self.world().get_non_send_resource::<R>()
    }

    fn get_non_send_mut<R: 'static>(&mut self) -> Option<Mut<R>> {
        self.world_mut().get_non_send_resource_mut::<R>()
    }
}

// End of File
