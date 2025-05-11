// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy_ecs::prelude::*;

/// The `ContainsWorld` trait provides an interface for objects which contain an ECS [`World`] to
/// expose access to the [`World`] via the [`world()`](ContainsWorld::world) and
/// [`world_mut()`](ContainsWorld::world_mut) methods.
///
/// Implementing `ContainsWorld` automatically makes default implementations of
/// [`ResourceManager`](crate::ResourceManager), [`NonSendManager`](crate::NonSendManager), and
/// [`ScheduleManager`](crate::ScheduleManager) available.
pub trait ContainsWorld {
    /// Gets an immutable reference to the ECS [`World`].
    fn world(&self) -> &World;

    /// Gets a mutable reference to the ECS [`World`].
    fn world_mut(&mut self) -> &mut World;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple resource to test world access
    #[derive(Resource, Default, Debug, PartialEq, Eq)]
    struct TestCounter {
        value: u32,
    }

    /// Simple implementations for testing the ContainsWorld trait
    struct TestSimpleApp {
        world: World,
    }

    impl TestSimpleApp {
        fn new() -> Self {
            Self {
                world: World::new(),
            }
        }
    }

    impl ContainsWorld for TestSimpleApp {
        fn world(&self) -> &World {
            &self.world
        }

        fn world_mut(&mut self) -> &mut World {
            &mut self.world
        }
    }

    /// More complex implementation with interior mutability
    struct TestComplexApp {
        // Using RefCell to demonstrate ContainsWorld with interior mutability
        world: std::cell::RefCell<World>,
    }

    impl TestComplexApp {
        fn new() -> Self {
            Self {
                world: std::cell::RefCell::new(World::new()),
            }
        }
    }

    impl ContainsWorld for TestComplexApp {
        fn world(&self) -> &World {
            // This is just for demonstration - in real code we'd need to
            // handle this better (perhaps with Rc<RefCell<World>> and cloning)
            // or using a different approach to interior mutability
            unsafe { &*self.world.as_ptr() }
        }

        fn world_mut(&mut self) -> &mut World {
            self.world.get_mut()
        }
    }

    /// Tests basic immutable world access
    ///
    /// WHY: This verifies that the simplest form of the trait works properly
    /// and allows read-only access to world data.
    #[test]
    fn test_world_immutable_access() {
        let mut app = TestSimpleApp::new();

        // Initialize a resource
        app.world_mut().init_resource::<TestCounter>();

        // Modify the resource
        {
            let mut counter = app.world_mut().resource_mut::<TestCounter>();
            counter.value = 42;
        }

        // Test immutable access
        let counter = app.world().resource::<TestCounter>();
        assert_eq!(counter.value, 42);
    }

    /// Tests mutable world access
    ///
    /// WHY: This test ensures the world_mut() method provides proper mutability,
    /// allowing changes to the contained world and its resources.
    #[test]
    fn test_world_mutable_access() {
        let mut app = TestSimpleApp::new();

        // Add resources and entities through the trait methods
        app.world_mut().init_resource::<TestCounter>();

        // Create an entity with a specific component
        let entity = app.world_mut().spawn_empty().id();

        // Verify entity exists
        assert!(app.world().entities().contains(entity));

        // Modify resource through mutable access
        {
            let mut counter = app.world_mut().resource_mut::<TestCounter>();
            counter.value = 100;
        }

        // Verify changes via immutable access
        assert_eq!(app.world().resource::<TestCounter>().value, 100);
    }

    /// Tests that different implementations can satisfy the trait
    ///
    /// WHY: This test demonstrates that the ContainsWorld trait is flexible
    /// and can be implemented by different types with different internal structures.
    #[test]
    fn test_different_implementations() {
        // Test the simple implementation
        let mut simple_app = TestSimpleApp::new();
        simple_app.world_mut().init_resource::<TestCounter>();
        assert_eq!(simple_app.world().resource::<TestCounter>().value, 0);

        // Test the complex implementation with interior mutability
        let mut complex_app = TestComplexApp::new();
        complex_app.world_mut().init_resource::<TestCounter>();
        assert_eq!(complex_app.world().resource::<TestCounter>().value, 0);

        // Modify both and ensure changes are isolated
        {
            let mut counter = simple_app.world_mut().resource_mut::<TestCounter>();
            counter.value = 10;
        }
        {
            let mut counter = complex_app.world_mut().resource_mut::<TestCounter>();
            counter.value = 20;
        }

        assert_eq!(simple_app.world().resource::<TestCounter>().value, 10);
        assert_eq!(complex_app.world().resource::<TestCounter>().value, 20);
    }
}

// End of File
