// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::App;
use std::any::Any;

/// A collection of application logic and configuration steps that can be added to an [`App`] to
/// customize its startup and runtime behavior.  Instances of types that implement the [`Plugin`]
/// trait may be registered with an [`App`] instance.  When a plugin is registered, the plugin's
/// [`Plugin::initialize`] method is called, allowing the plugin to configure the [`App`] instance
/// as needed.
///
/// By default, a given type of plugin can only be added once to an [`App`], and this is enforced at
/// runtime.  This is a safety check as plugins may contain non-idempotent operations.  For plugins
/// which explicitly expect to be added multiple times, such plugins will need to override the
/// [`Plugin::is_unique`] method to return `false`.  This will allow the plugin to be added multiple
/// times to the same [`App`] instance (presumably with different parameters, e.g. a
/// `ConfigurationFilePlugin` that specifies a file to check on startup for runtime options, and
/// more than one file location is to be checked).
pub trait Plugin: Any {
    /// Immediately called when adding a plugin to an [`App`] instance.
    fn register(&self, app: &mut App);

    /// Configure the [`App`] instance to which this [`Plugin`] was added.  Each plugin will is
    /// initialized in sequence, from the main thread of the application, in the order in which they
    /// were added to the [`App`].  For the avoidance of doubt and to enable asynchronous
    /// initalization steps, [`Plugin`] initialization dependencies should be handled by performing
    /// tasks dependent on the setup of other plugins in the [`Plugin::finalize`] implementation.
    ///
    /// To ensure speedy application startup, [`Plugin`]s with long initialization steps should
    /// (when possible) defer those steps to an asynchronous task.  The
    /// [`Future`](std::future::Future) contracts associated with any async tasks should be stored
    /// somewhere so that they can be waited upon later (see [`Plugin::block_until_initialized`]).
    fn initialize(&self, app: &mut App) -> anyhow::Result<()> {
        let _ = app;
        Ok(())
    }

    /// After [`Plugin::initialize`] has been called for all registered [`Plugin`]s, the [`App`]
    /// will, from the main thread, call [`Plugin::block_until_initialized`] on each plugin in turn,
    /// prior to [`Plugin::finalize`] or [`Plugin::cleanup`] being called for *any* plugin.  Does
    /// not return until the plugin has fully finalized all initialization & configuration steps.
    ///
    /// This API is typically used by [`Plugin`]s which started asynchronous initialization steps in
    /// their [`Plugin::initialize`] step.  This function will block until all such initialization
    /// steps are complete, or return an error (any error) which will cause the [`App`] to abort
    /// initialization and exit.
    fn block_until_initialized(&self, app: &App) -> anyhow::Result<()> {
        let _ = app;
        Ok(())
    }

    /// Perform any further initialization steps that need to be completed before the application
    /// starts running, but typically those which are dependent on the initialization steps of other
    /// [`Plugin`]s (otherwise the startup logic would be handled by [`Plugin::initialize`]).
    ///
    /// This method is called after [`Plugin::initialize`] and [`Plugin::block_until_initialized`]
    /// have been called on *all* plugins.  This method is called in sequence, from the main thread
    /// of the application, in the order in which the plugins were added to the [`App`].  Therefore
    /// if any long-running tasks are to be performed, then once again they should be deferred to an
    /// asynchronous task, and the [`Future`](std::future::Future) contracts associated with those
    /// tasks should be stored somewhere so that they can be waited upon later (see
    /// [`Plugin::block_until_finalized`]).
    fn finalize(&self, app: &mut App) -> anyhow::Result<()> {
        let _ = app;
        Ok(())
    }

    /// After [`Plugin::finalize`] has been called for all registered [`Plugin`]s, the [`App`] will,
    /// from the main thread, call [`Plugin::block_until_finalized`] on each plugin in turn, prior
    /// to calling [`Plugin::cleanup`].  Does not return until the plugin has fully finalized all
    /// initialization & configuration steps.
    fn block_until_finalized(&self, app: &App) -> anyhow::Result<()> {
        let _ = app;
        Ok(())
    }

    /// Runs after all [`Plugin`]s have been built and finalized, but before any schedules are
    /// executed.  This can be useful if you have some resource that other plugins need during their
    /// [`Plugin::finalize`] step, but after the build you want to remove it, save it to a different
    /// resource, or send it to another thread.
    ///
    /// Since all initialization steps are assumed to be complete at this point, this method is
    /// incapable of returning an error to abort execution.  If it is possible for an irrecoverable
    /// error to occur during the execution of this method, you should serious consider if you've
    /// implemented the [`Plugin`] trait correctly.  Otherwise you will need to rely on `panic!` to
    /// abort execution.
    fn cleanup(&self, app: &mut App) {
        let _ = app;
    }

    /// Whether a given [`App`] instance may include more than one instance of this plugin type, as
    /// identified by [`std::any::TypeId`].  The default implementation of this method returns
    /// `true`, as a safety check against plugins that perform non-idempotent steps at runtime. This
    /// behavior only needs to be overridden for plugin implementations with internal state where it
    /// explicitly makes sense to add multiple instances to the same [`App`].  For example:
    ///
    /// ```
    /// # use atomcad_app::{App, Plugin};
    /// pub struct ConfigurationFilePlugin(std::path::PathBuf);
    ///
    /// impl Plugin for ConfigurationFilePlugin {
    ///     fn register(&self, app: &mut App) {
    ///         // Read the configuration file and apply the settings to the app
    ///     }
    ///     fn is_unique(&self) -> bool {
    ///         false
    ///     }
    /// }
    ///
    /// App::new("Duplicate Plugins".into())
    ///     .add_plugin(ConfigurationFilePlugin("~/.config/duplicate-plugins.toml".into()))
    ///     .add_plugin(ConfigurationFilePlugin("/etc/duplicate-plugins/config.toml".into())).run();
    /// ```
    fn is_unique(&self) -> bool {
        true
    }

    /// Return the [`TypeId`](std::any::TypeId) of the plugin type.  The default implementation is
    /// exactly what you would expect: the [`TypeId`](std::any::TypeId) of the plugin type itself,
    /// and most users should have no reason to change this.  One reason you might is if you wanted
    /// to have two separate plugin types that cannot both be added to the same [`App`] instance.
    /// For example:
    ///
    /// ```should_panic
    /// # use atomcad_app::{App, Plugin};
    /// pub struct ConflictingPluginTag;
    ///
    /// pub struct PluginA;
    /// impl Plugin for PluginA {
    ///     fn register(&self, app: &mut App) {}
    ///     fn id(&self) -> std::any::TypeId {
    ///         std::any::TypeId::of::<ConflictingPluginTag>()
    ///     }
    /// }
    ///
    /// pub struct PluginB;
    /// impl Plugin for PluginB {
    ///     fn register(&self, app: &mut App) {}
    ///     fn id(&self) -> std::any::TypeId {
    ///         std::any::TypeId::of::<ConflictingPluginTag>()
    ///     }
    /// }
    ///
    /// App::new("Conflicting Plugins".into())
    ///     .add_plugin(PluginA)
    ///     .add_plugin(PluginB);
    /// ```
    fn id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
}

/// As it is commonly the case that a plugin only implements a single [`Plugin::register`] method
/// that configures the application runner, a convenience implementation is provided that allows any
/// closure to be used in place of a full plugin type.
impl<T: Fn(&mut App) + Any> Plugin for T {
    fn register(&self, app: &mut App) {
        self(app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::Update;
    use std::panic::AssertUnwindSafe;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::thread;
    use std::time::Duration;

    /// Demonstrate the simplest form of plugin implementation.
    #[test]
    fn test_basic_plugin() {
        // Simple counter to track plugin lifecycle
        let counter = Arc::new(AtomicUsize::new(0));

        // A basic plugin just needs to implement the register method
        struct BasicPlugin(Arc<AtomicUsize>);

        impl Plugin for BasicPlugin {
            fn register(&self, app: &mut App) {
                // The register method is called immediately when the plugin is added
                self.0.fetch_add(1, Ordering::SeqCst);

                // Plugins typically configure the app by adding systems, resources, etc.
                app.add_systems(Update, || {
                    // A simple system that does nothing
                });
            }
        }

        // Create an app and add our plugin
        let mut app = App::new("BasicPluginDemo".into());
        app.add_plugin(BasicPlugin(counter.clone()));

        // The register method was called during add_plugin
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    /// Validate the complete plugin lifecycle within a practical usage example.
    #[test]
    fn test_plugin_lifecycle() {
        use ecs::prelude::*;

        // Define resources that our plugins will work with
        #[derive(Resource, Default)]
        struct DatabaseConnection {
            is_connected: bool,
            data: Vec<String>,
        }

        #[derive(Resource, Default)]
        struct ConfigSettings {
            initialized: bool,
            settings: Vec<(String, String)>,
        }

        // Plugin that simulates database connectivity
        struct DatabasePlugin {
            // Channel to wait for async work completion
            connection_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
            connection_rx: Arc<Mutex<Option<mpsc::Receiver<()>>>>,
            // Flag to track if connection has been completed
            connection_ready: Arc<Mutex<bool>>,
        }

        impl DatabasePlugin {
            fn new() -> Self {
                let (tx, rx) = mpsc::channel();
                Self {
                    connection_tx: Arc::new(Mutex::new(Some(tx))),
                    connection_rx: Arc::new(Mutex::new(Some(rx))),
                    connection_ready: Arc::new(Mutex::new(false)),
                }
            }

            // This simulates starting an async connection process
            fn start_connection(&self) {
                // Take the sender to use in our background "async" work
                if let Some(tx) = self.connection_tx.lock().unwrap().take() {
                    // Create a clone of the ready flag for the thread to update
                    let ready = self.connection_ready.clone();

                    // Spawn a thread to simulate async work
                    thread::spawn(move || {
                        // Simulate work with a small delay
                        thread::sleep(Duration::from_millis(50));

                        // Signal completion
                        let _ = tx.send(());

                        // Update the ready flag
                        *ready.lock().unwrap() = true;
                    });
                }
            }

            // This simulates waiting for an async operation to complete
            fn wait_for_connection(&self) -> bool {
                if let Some(rx) = self.connection_rx.lock().unwrap().take() {
                    // Block until connection is ready - equivalent to await in async code
                    match rx.recv() {
                        Ok(()) => true,
                        Err(_) => false,
                    }
                } else {
                    false
                }
            }

            // Checks if the connection is ready
            fn is_connected(&self) -> bool {
                *self.connection_ready.lock().unwrap()
            }
        }

        impl Plugin for DatabasePlugin {
            fn register(&self, app: &mut App) {
                // Register is for immediate setup tasks like registering resources
                app.insert_resource(DatabaseConnection::default());
            }

            fn initialize(&self, _app: &mut App) -> anyhow::Result<()> {
                // Initialize is for starting async operations

                // Start the async connection process
                self.start_connection();

                Ok(())
            }

            fn block_until_initialized(&self, _app: &App) -> anyhow::Result<()> {
                // The purpose of this method is to wait for any async operations
                // started during initialize() to complete before proceeding.

                // Wait for async operations to complete
                let _ = self.wait_for_connection();

                // Note: We'll update the resource in finalize to avoid borrowing issues
                Ok(())
            }

            fn finalize(&self, app: &mut App) -> anyhow::Result<()> {
                // Finalize is for operations that depend on other plugins' initialization

                // First get the config data we need
                let settings = if let Some(config) = app.get_resource::<ConfigSettings>() {
                    config.settings.clone()
                } else {
                    Vec::new()
                };

                // Then update the database with the config data
                if let Some(mut db) = app.get_resource_mut::<DatabaseConnection>() {
                    // Now that connection is complete, update the connected flag
                    db.is_connected = self.is_connected();

                    // Add data from config
                    db.data = settings
                        .iter()
                        .map(|(key, value)| format!("{key}:{value}"))
                        .collect();
                }

                Ok(())
            }

            fn cleanup(&self, app: &mut App) {
                // Final verification before the app starts running
                if let Some(db) = app.get_resource::<DatabaseConnection>() {
                    assert!(db.is_connected, "Database should be connected");
                }
            }
        }

        // Plugin that manages configuration
        struct ConfigPlugin;

        impl Plugin for ConfigPlugin {
            fn register(&self, app: &mut App) {
                app.insert_resource(ConfigSettings::default());
            }

            fn initialize(&self, app: &mut App) -> anyhow::Result<()> {
                // Simulate loading configuration
                if let Some(mut config) = app.get_resource_mut::<ConfigSettings>() {
                    config.settings = vec![
                        ("database.max_connections".into(), "100".into()),
                        ("database.timeout".into(), "30".into()),
                        ("app.name".into(), "Lifecycle Demo".into()),
                    ];
                    config.initialized = true;
                }

                Ok(())
            }

            fn finalize(&self, app: &mut App) -> anyhow::Result<()> {
                // Apply configuration to the application
                let app_name = if let Some(config) = app.get_resource::<ConfigSettings>() {
                    config
                        .settings
                        .iter()
                        .find(|(key, _)| key == "app.name")
                        .map(|(_, value)| value.clone())
                } else {
                    None
                };

                // Use the config value if found
                if let Some(name) = app_name {
                    app.set_name(name);
                }

                Ok(())
            }
        }

        // Create an app and add our plugins
        let mut app = App::new("LifecycleDemo".into());

        app.add_plugin(ConfigPlugin);
        app.add_plugin(DatabasePlugin::new());

        // Add a system that will use the plugins' resources
        app.add_systems(
            Update,
            |db: Res<DatabaseConnection>, config: Res<ConfigSettings>| {
                // This system will run after all plugin initialization is complete
                assert!(db.is_connected);
                assert!(config.initialized);
                assert!(!db.data.is_empty());
            },
        );

        // Running the app triggers the complete lifecycle
        app.run();
    }

    /// Create a plugin that can be added multiple times.
    #[test]
    fn test_non_unique_plugins() {
        // A simple configuration plugin that can be added multiple times
        struct ConfigPlugin {
            name: String,
            value: i32,
        }

        impl Plugin for ConfigPlugin {
            fn register(&self, _app: &mut App) {
                // In a real plugin, we'd store this config in the app's resources
                println!("Registering config: {} = {}", self.name, self.value);
            }

            // By returning false, we allow multiple instances of this plugin
            fn is_unique(&self) -> bool {
                false
            }
        }

        // A regular plugin that is unique
        struct UniquePlugin;

        impl Plugin for UniquePlugin {
            fn register(&self, _app: &mut App) {}
        }

        // The default implementation of is_unique() returns true
        assert!(UniquePlugin.is_unique());

        let mut app = App::new("NonUniqueDemo".into());

        // We can add the same non-unique plugin multiple times with different parameters
        app.add_plugin(ConfigPlugin {
            name: "timeout".into(),
            value: 30,
        });
        app.add_plugin(ConfigPlugin {
            name: "max_retries".into(),
            value: 5,
        });
        app.add_plugin(ConfigPlugin {
            name: "buffer_size".into(),
            value: 1024,
        });

        // For unique plugins, only one instance can be added
        app.add_plugin(UniquePlugin);

        // Adding a second instance would panic:
        // SAFETY: app is not used after this point.
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.add_plugin(UniquePlugin);
        }));
        assert!(result.is_err());
    }

    /// Use custom plugin IDs to create plugin groups.
    #[test]
    fn test_custom_plugin_ids() {
        // A marker type for our plugin group
        struct RenderingPluginGroup;

        // Different plugins that all belong to the same logical group
        struct GraphicsPlugin;
        struct ShadingPlugin;

        // Both plugins share the same ID, so only one can be added
        impl Plugin for GraphicsPlugin {
            fn register(&self, _app: &mut App) {}

            fn id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<RenderingPluginGroup>()
            }
        }

        impl Plugin for ShadingPlugin {
            fn register(&self, _app: &mut App) {}

            fn id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<RenderingPluginGroup>()
            }
        }

        let mut app = App::new("CustomIDDemo".into());

        // We can add one of these plugins
        app.add_plugin(GraphicsPlugin);

        // But adding another would panic because they share the same ID.
        // SAFETY: app is not used after this point.
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.add_plugin(ShadingPlugin);
        }));
        assert!(result.is_err());
    }

    /// Demonstrates how to use closure-based plugins for simple cases
    #[test]
    fn test_closure_plugins() {
        // Track that our closure was called
        let called = Arc::new(AtomicUsize::new(0));
        let called_clone = called.clone();

        let mut app = App::new("ClosureDemo".into());

        // For simple plugins, you can just use a closure
        app.add_plugin(move |app: &mut App| {
            called_clone.fetch_add(1, Ordering::SeqCst);
            app.add_systems(Update, || {
                // A system added by our closure plugin
            });
        });

        assert_eq!(called.load(Ordering::SeqCst), 1);

        // Closure plugins are great for small configuration tasks
        app.add_plugin(|app: &mut App| {
            app.set_name("RenamedByPlugin".into());
        });

        assert_eq!(app.name(), "RenamedByPlugin");
    }
}

// End of File
