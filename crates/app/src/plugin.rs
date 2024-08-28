// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

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

// End of File
