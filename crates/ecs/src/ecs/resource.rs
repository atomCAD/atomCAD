// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use super::ContainsWorld;
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
    fn get_resource_mut<R: Resource>(&mut self) -> Option<Mut<'_, R>>;

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
    fn resource_mut<R: Resource>(&mut self) -> Mut<'_, R> {
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

    fn get_resource_mut<R: Resource>(&mut self) -> Option<Mut<'_, R>> {
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
    fn get_non_send_mut<R: 'static>(&mut self) -> Option<Mut<'_, R>>;

    /// Checks if a non-Send resource of type `R` exists.
    fn contains_non_send<R: 'static>(&self) -> bool {
        self.get_non_send::<R>().is_some()
    }

    /// Gets a read-only reference to the non-Send resource of type `R`.
    ///
    /// # Panics
    /// Panics if there is no non-Send resource of type `R`.
    fn non_send<R: 'static>(&self) -> &R {
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
    fn non_send_mut<R: 'static>(&mut self) -> Mut<'_, R> {
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

    fn get_non_send_mut<R: 'static>(&mut self) -> Option<Mut<'_, R>> {
        self.world_mut().get_non_send_resource_mut::<R>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{panic::AssertUnwindSafe, rc::Rc, sync::atomic::AtomicU32};

    /// Simple resource for testing
    #[derive(Debug, Default, PartialEq, Eq, Resource)]
    struct Counter {
        count: u32,
    }

    /// Resource with data for testing manipulations
    #[derive(Debug, Default, PartialEq, Eq, Resource)]
    struct TestResource {
        value: u32,
    }

    /// Non-Send resource for testing NonSendManager
    #[derive(Debug, Default)]
    struct NonSendCounter {
        count: Rc<AtomicU32>,
    }

    /// A world wrapper that implements ContainsWorld
    /// This mimics how a real application might wrap the ECS World
    struct TestApp {
        world: World,
    }

    impl TestApp {
        fn new() -> Self {
            Self {
                world: World::new(),
            }
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

    /// Tests basic resource initialization
    ///
    /// WHY: This test verifies that init_resource correctly creates a resource
    /// only when it doesn't exist. This is important because initialization
    /// should be idempotent - calling it multiple times should have no
    /// additional effect after the first call.
    #[test]
    fn test_init_resource() {
        let mut app = TestApp::new();

        // Resource doesn't exist yet
        assert!(!app.contains_resource::<Counter>());

        // First initialization creates it with default value
        app.init_resource::<Counter>();
        assert!(app.contains_resource::<Counter>());
        assert_eq!(app.resource::<Counter>().count, 0);

        // Modify the resource
        app.edit_resource::<Counter>(|mut counter| {
            counter.count = 42;
        });

        // Second initialization shouldn't change the value
        app.init_resource::<Counter>();
        assert_eq!(app.resource::<Counter>().count, 42);
    }

    /// Tests inserting resources with different approaches
    ///
    /// WHY: This test demonstrates the difference between insert_resource and add_resource.
    /// Understanding the distinction is crucial for managing resources correctly without
    /// unexpected data loss.
    #[test]
    fn test_insert_add_resource() {
        let mut app = TestApp::new();

        // Insert returns None for a new resource
        let previous = app.insert_resource(Counter { count: 5 });
        assert!(previous.is_none());
        assert_eq!(app.resource::<Counter>().count, 5);

        // Insert returns the previous resource when one exists
        let previous = app.insert_resource(Counter { count: 10 });
        assert_eq!(previous, Some(Counter { count: 5 }));
        assert_eq!(app.resource::<Counter>().count, 10);

        // Add_resource also replaces but doesn't return the previous value
        app.add_resource(Counter { count: 15 });
        assert_eq!(app.resource::<Counter>().count, 15);

        // Chaining works with add_resource
        app.add_resource(TestResource { value: 20 })
            .add_resource(Counter { count: 25 });

        assert_eq!(app.resource::<TestResource>().value, 20);
        assert_eq!(app.resource::<Counter>().count, 25);
    }

    /// Tests removing resources
    ///
    /// WHY: Resource removal is important for cleanup and memory management.
    /// This test shows that resources can be completely removed from the world
    /// and ensures the removal operation returns the removed resource.
    #[test]
    fn test_remove_resource() {
        let mut app = TestApp::new();

        // Add a resource first
        app.add_resource(Counter { count: 5 });
        assert!(app.contains_resource::<Counter>());

        // Remove returns the resource and it's no longer in the world
        let removed = app.remove_resource::<Counter>();
        assert_eq!(removed, Some(Counter { count: 5 }));
        assert!(!app.contains_resource::<Counter>());

        // Remove on a non-existent resource returns None
        let not_found = app.remove_resource::<Counter>();
        assert_eq!(not_found, None);
    }

    /// Tests accessing resources with different methods
    ///
    /// WHY: There are multiple ways to access resources, with different error handling
    /// behaviors. Understanding these differences is important to avoid panics in production.
    #[test]
    fn test_resource_access() {
        let mut app = TestApp::new();

        // Add a resource to work with
        app.add_resource(Counter { count: 5 });

        // get_resource returns Option<&T>
        let counter = app.get_resource::<Counter>();
        assert_eq!(counter, Some(&Counter { count: 5 }));

        // resource returns &T directly but panics if not found
        let counter = app.resource::<Counter>();
        assert_eq!(counter.count, 5);

        // Accessing a non-existent resource
        assert!(app.get_resource::<TestResource>().is_none());

        // visit_resource provides safe access without panicking
        let mut visited = false;
        app.visit_resource::<Counter>(|counter| {
            visited = true;
            assert_eq!(counter.count, 5);
        });
        assert!(visited);

        // visit_resource does nothing for missing resources
        let mut visited = false;
        app.visit_resource::<TestResource>(|_| {
            visited = true;
        });
        assert!(!visited);

        // Demonstrate panic behavior
        // SAFETY: we don't use app after this point
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.resource::<TestResource>();
        }));
        assert!(result.is_err());
    }

    /// Tests mutable resource access
    ///
    /// WHY: Mutable resource access is critical for game state updates.
    /// Understanding how resource_mut vs get_resource_mut differ in error
    /// handling helps prevent runtime crashes.
    #[test]
    fn test_mutable_resource_access() {
        let mut app = TestApp::new();

        // Add a resource to work with
        app.add_resource(Counter { count: 5 });

        // get_resource_mut returns Option<Mut<T>> for safe access
        if let Some(mut counter) = app.get_resource_mut::<Counter>() {
            counter.count = 10;
        }
        assert_eq!(app.resource::<Counter>().count, 10);

        // resource_mut returns Mut<T> directly but panics if not found
        {
            let mut counter = app.resource_mut::<Counter>();
            counter.count = 15;
            // drop mutable reference
        }
        assert_eq!(app.resource::<Counter>().count, 15);

        // edit_resource provides safe modification without panicking
        app.edit_resource::<Counter>(|mut counter| {
            counter.count = 20;
        });
        assert_eq!(app.resource::<Counter>().count, 20);

        // edit_resource does nothing for missing resources
        let mut edited = false;
        app.edit_resource::<TestResource>(|_| {
            edited = true;
        });
        assert!(!edited);
    }

    /// Tests resource_scope for accessing resource and world together
    ///
    /// WHY: Sometimes we need to modify both a resource and the world at the same time,
    /// which normally would cause borrowing conflicts. resource_scope provides a safe
    /// way to do this by temporarily removing the resource from the world.
    #[test]
    fn test_resource_scope() {
        let mut app = TestApp::new();

        // Add resources
        app.add_resource(Counter { count: 5 });

        // Use resource_scope to safely modify both world and resource
        let result = app.resource_scope(|world, mut counter: Mut<Counter>| {
            // Can modify the resource
            counter.count = 10;

            // Can also modify the world
            world.insert_resource(TestResource { value: 20 });

            // Can return a value from the scope
            counter.count + 5
        });

        // Check that the resource was modified and returned to the world
        assert_eq!(app.resource::<Counter>().count, 10);

        // Check that world changes took effect
        assert_eq!(app.resource::<TestResource>().value, 20);

        // Check that the return value from the scope is correct
        assert_eq!(result, 15);
    }

    /// Tests non-send resources
    ///
    /// WHY: Non-Send resources are special cases that can't be shared between threads.
    /// Understanding how they work is important for features that must run on the main thread,
    /// like window handles or certain platform-specific APIs.
    #[test]
    fn test_non_send_resources() {
        let mut app = TestApp::new();

        // Non-send resources aren't initially present
        assert!(!app.contains_non_send::<NonSendCounter>());

        // Initialize a non-send resource
        app.init_non_send::<NonSendCounter>();
        assert!(app.contains_non_send::<NonSendCounter>());

        // Can access and modify non-send resources
        let counter = app.non_send_mut::<NonSendCounter>();
        counter.count.store(5, std::sync::atomic::Ordering::Relaxed);

        // Using insert_non_send
        let previous = app.insert_non_send(NonSendCounter::default());
        assert!(previous.is_some());

        // Can modify with edit_non_send
        app.edit_non_send::<NonSendCounter>(|counter| {
            counter
                .count
                .store(10, std::sync::atomic::Ordering::Relaxed);
        });

        assert_eq!(
            app.non_send::<NonSendCounter>()
                .count
                .load(std::sync::atomic::Ordering::Relaxed),
            10
        );

        // Can remove non-send resources
        let removed = app.remove_non_send::<NonSendCounter>();
        assert!(removed.is_some());
        assert!(!app.contains_non_send::<NonSendCounter>());
    }
}

// End of File
