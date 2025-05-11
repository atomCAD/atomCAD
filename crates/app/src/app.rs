// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{
    platform::PanicHandlerPlugin,
    plugin::Plugin,
    schedule::{First, MainSchedulePlugin},
};
use core::num::NonZero;
use ecs::{
    message::{
        MessageCursor, MessageUpdateSystems, message_update_condition, message_update_system,
    },
    prelude::*,
    schedule::{InternedScheduleLabel, ScheduleLabel},
    system::ScheduleSystem,
};
use std::{
    collections::HashSet,
    process::{ExitCode, Termination},
};

/// The status code to use when exiting the application.  It is the value returned by the
/// application runner, and passed back to the callee of [`App::run()`].
#[derive(Message, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppExit {
    /// The application exited successfully.  This results in a status code of 0 on POSIX systems.
    Success,
    /// The application exited with an error.  The status code is captured in the [`NonZero<u8>`]
    /// value.  The status code must be non-zero because POSIX systems reserve a status code of 0 to
    /// indicate success, and although [`std::process::exit()`] will accept an [`i32`] value, it is
    /// only guaranteed that the lowest 8 bits are passed to the calling shell.  A hypothetical
    /// `Error(0x100)` would thus be falsely interpreted as `Error(0)`.
    Error(NonZero<u8>),
}

impl AppExit {
    pub const fn is_ok(&self) -> bool {
        matches!(self, AppExit::Success)
    }

    pub const fn is_err(&self) -> bool {
        matches!(self, AppExit::Error(_))
    }
}

impl Termination for AppExit {
    fn report(self) -> ExitCode {
        match self {
            AppExit::Success => ExitCode::SUCCESS,
            AppExit::Error(code) => {
                log::error!("ExitCode: {}", code.get());
                ExitCode::from(code.get())
            }
        }
    }
}

type RunnerFn = Box<dyn FnOnce(&mut App) -> AppExit>;

/// Does the necessary glue work to combine [`World`] and [`Schedule`]s to create an ECS-based
/// application.  The [`App`] is a global repository of program state and the application runner,
/// the main loop of the program.  Typically no more than one application runner is instantiated in
/// the lifetime of a process.  On some platforms and configurations, such as graphical apps on iOS
/// or web, this is a hard requirement as the main loop of the application runner must run on the
/// main thread, and will never return.
///
/// # Examples
///
/// Here is a simple “Hello, World!” app:
///
/// ```
/// # use atomcad_app::prelude::*;
/// fn main() {
///     App::new("Hello World".into())
///         .add_systems(Update, print_hello_world)
///         .run();
/// }
///
/// fn print_hello_world() {
///     println!("Hello, World!");
/// }
/// ```
pub struct App {
    /// The application name, as passed to [`new`](Self::new) or [`set_name`](Self::set_name), and
    /// used in user interface or logging/diagnostic text.
    name: String,
    /// The application runner, a closure run by [`run`](Self::run) that processes the main loop of
    /// the application.  This is set by [`set_runner`](Self::set_runner), and defaults to
    /// [`run_once`].
    runner: RunnerFn,
    /// The schedule that will be run by [`update`](Self::update), which is called once on each
    /// iteration through the main loop (or a total of one time if [`run_once`] is your application
    /// runner).  This is set by [`set_update_schedule`](Self::set_update_schedule), and defaults to
    /// [`Main`] if the application is includes the [`MainSchedulePlugin`], which is part of the
    /// default plugin set. If [`None`], calling [`update`](Self::update) will not run any
    /// schedules.
    update_schedule: Option<InternedScheduleLabel>,
    /// A set of plugin IDs that have already been registered with the application.  This set is
    /// checked on each call to [`add_plugin`](Self::add_plugin) to ensure that a plugin is not
    /// added to the same application instance more than once, unless [`Plugin::is_unique`] returns
    /// `false`.
    unique_plugins: HashSet<std::any::TypeId>,
    /// A list of plugins that have been registered with the application, stored in the order in
    /// which they were registered.
    plugins: Vec<Box<dyn Plugin>>,
    /// The global repository of program state, managed by the [`ecs`] ECS library.
    world: World,
}

/// The default application runner, which features no event loop.  This is useful for simple
/// headless applications that do not require a main loop, such as command-line utilities, and is
/// the default behavior of a newly initialized [`App`].
pub fn run_once(app: &mut App) -> AppExit {
    app.update();
    app.should_exit().unwrap_or(AppExit::Success)
}

/// Associated functions for initializing and manipulating [`App`] instances.  You should use
/// [`App::new`] to initialize a new [`App`] instance, unless you really know what you are doing.
/// The closure to use to execute the main loop of the application can be configured with
/// [`set_runner`](Self::set_runner), and plugins can be added with
/// [`add_plugin`](Self::add_plugin).  Once an [`App`] is fully configured, enter the main loop with
/// [`run()`](Self::run).
///
/// ```
/// # use atomcad_app::prelude::*;
/// App::new("My App".into())
///     .set_runner(|app: &mut App| {
///         # let _ = app;
///         // Your application logic here.
///         AppExit::Success
///     })
///     .run();
/// ```
impl App {
    /// Creates a new application runner with the given name, with no default configuration.
    /// Depending on your platform, some platform-specific initialization may be required.  For a
    /// list of the default plugins excluded, see [`App::new`].
    pub fn empty(name: String) -> Self {
        // Initialize the world with the [`Schedules`] resource, as otherwise [`App::add_schedule`]
        // and such will panic.
        let mut world = World::new();
        world.init_resource::<Schedules>();
        Self {
            name,
            runner: Box::new(run_once),
            update_schedule: None,
            unique_plugins: HashSet::new(),
            plugins: Vec::new(),
            world,
        }
    }

    /// Creates a new application runner with the given name, initialized with a sensible but
    /// minimal list of default plugins for platform support and running the main loop schedule.  To
    /// initialize a new application runner with absolutely no default configuration behavior, use
    /// [`App::empty`].
    ///
    /// The name is used to identify the application in log messages and other diagnostic output, as
    /// well as user interface elements in window managers (default window title, application menu
    /// name on macOS, etc.).  The runner is initialized to [`run_once`], but can be changed with
    /// [`set_runner`](Self::set_runner).  The name must be specified, but can later be changed with
    /// [`set_name`](Self::set_name).
    ///
    /// # Current Default Plugins:
    ///
    /// * [`PanicHandlerPlugin`]: Registers a panic hook that logs errors to the Javascript console
    ///   on web.  On other platforms, this plugin does nothing.
    ///
    /// * [`MainSchedulePlugin`]: Sets up the main schedule for the application, which runs every
    ///   iteration of the main loop and is responsible for running the startup and update phases of
    ///   the application:
    ///     - [`PreStartup`](super::schedule::PreStartup), [`Startup`](super::schedule::Startup),
    ///       and [`PostStartup`](super::schedule::PostStartup) are run once on the first iteration
    ///       of the main loop.
    ///     - [`First`](super::schedule::First), [`PreUpdate`](super::schedule::PreUpdate),
    ///       [`Update`](super::schedule::Update), [`PostUpdate`](super::schedule::PostUpdate), and
    ///       [`Last`](super::schedule::Last) are also run on the first iteration of the main loop,
    ///       and again on every iteration thereafter.
    pub fn new(name: String) -> Self {
        let mut app = Self::empty(name);
        app.add_plugin(PanicHandlerPlugin);
        app.add_plugin(MainSchedulePlugin);
        app.add_systems(
            First,
            message_update_system
                .in_set(MessageUpdateSystems)
                .run_if(message_update_condition),
        );
        app.add_message::<AppExit>();
        app
    }

    /// Returns the application name. This is used to identify the application in log messages
    /// and other diagnostic output, as well as user interface elements in window managers.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Change the name of the application runner, during configuration or at runtime.  To read the
    /// current application name, do so directly via the [`name`](Self::name) field.
    pub fn set_name(&mut self, name: String) -> &mut Self {
        self.name = name;
        self
    }

    /// Change the schedule that will be run by [`update`](Self::update), which is called once on
    /// each iteration through the main loop.
    pub fn set_update_schedule(&mut self, label: impl ScheduleLabel) -> &mut Self {
        let label = label.intern();
        self.update_schedule = Some(label);
        self
    }

    /// Change the runner / event loop function.  Runners are expected to be called only once, and
    /// are permitted to never return (although it is generally preferable to do so on platforms
    /// where that is a possibility).
    pub fn set_runner(&mut self, runner: impl FnOnce(&mut App) -> AppExit + 'static) -> &mut Self {
        self.runner = Box::new(runner);
        self
    }

    /// Register a plugin with the application.  Plugins are used to configure the application and
    /// provide additional functionality to maintain global state or service the application event
    /// loop.  A given type of plugin can only be added to the same application instance once,
    /// unless [`Plugin::is_unique`] returns `false`.
    ///
    /// # Panics
    ///
    /// * As must envisioned use cases involve separate plugin types for each configuration or
    ///   feature, adding two plugins of the same type to the same application instance is generally
    ///   disallowed, and will generally result in a panic.  See [`Plugin::is_unique`] for details.
    pub fn add_plugin(&mut self, plugin: impl Plugin) -> &mut Self {
        // Panic if the plugin is unique and has already been added to the application.
        let id = plugin.id();
        if plugin.is_unique() {
            if self.unique_plugins.contains(&id) {
                panic!("Attempted to add a non-unique plugin to the same App instance twice");
            }
            self.unique_plugins.insert(id);
        }

        // Call the plugin's initialization method, which configures the application.
        plugin.register(self);

        // Add the plugin to the application's list of plugins.
        self.plugins.push(Box::new(plugin));

        self
    }

    /// Adds one or more systems to the given `schedule` in this app's [`Schedules`].
    pub fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        let schedule = schedule.intern();
        let mut schedules = self.world.resource_mut::<Schedules>();

        if let Some(schedule) = schedules.get_mut(schedule) {
            schedule.add_systems(systems);
        } else {
            let mut new_schedule = Schedule::new(schedule);
            new_schedule.add_systems(systems);
            schedules.insert(new_schedule);
        }

        self
    }

    /// Runs the update schedule, which is called once on each iteration through the main loop (or a
    /// total of one time if [`run_once`] is your application runner).  This method is called
    /// automatically by the default runner, but can be called manually if you are using a custom
    /// runner.
    pub fn update(&mut self) {
        if let Some(label) = self.update_schedule
            && let Err(e) = self.world.try_run_schedule(label)
        {
            log::error!("Error running update schedule {:?}: {:?}", label, e);
        }
    }

    /// Run the application's event processing loop by calling its [runner](Self::set_runner).  On
    /// some platforms This *must* be called from the main thread of the application.
    ///
    /// *Note*: Despite its `&mut App` signature, this method fully consumes the [`App`] object, as
    ///         if it had the type signature `fn run(self)`.  Upon return (if it returns), the
    ///         [`App`] object will be equal `App::empty("".into())`, with plugins and world state
    ///         having been dropped.
    ///
    /// # Caveats
    ///
    /// * Calls to [`run()`](Self::run) will never return on iOS and Web, unless the headless
    ///   [`run_once`] runner is used, as running the user input event loop on these platforms
    ///   requires giving up control of the main thread.
    ///
    /// * Headless apps that use [`run_once`] or implement their own runner can generally expect
    ///   this method to return control to the caller upon completion.  Those that do not require
    ///   interfacing with the operating system / window manager's event loop may or may not return,
    ///   and even if they do return, on some platforms it is not possible to re-initialize the
    ///   windowing event loop.
    ///
    /// # Panics
    ///
    /// * Panics if not called from the main thread on platforms where this is a requirement.
    pub fn run(&mut self) -> AppExit {
        let plugins = std::mem::take(&mut self.plugins);
        for plugin in plugins.iter() {
            if let Err(e) = plugin.initialize(self) {
                panic!(".initialize failed for plugin {:?}: {}", plugin.id(), e);
            }
        }
        for plugin in plugins.iter() {
            if let Err(e) = plugin.block_until_initialized(self) {
                panic!(
                    ".block_until_initialized failed for plugin {:?}: {}",
                    plugin.id(),
                    e
                );
            }
        }
        for plugin in plugins.iter() {
            if let Err(e) = plugin.finalize(self) {
                panic!(".finalize failed for plugin {:?}: {}", plugin.id(), e);
            }
        }
        for plugin in plugins.iter() {
            if let Err(e) = plugin.block_until_finalized(self) {
                panic!(
                    ".block_until_finalized failed for plugin {:?}: {}",
                    plugin.id(),
                    e
                );
            }
        }
        for plugin in plugins.iter() {
            plugin.cleanup(self)
        }
        drop(plugins);
        let _ = std::mem::take(&mut self.unique_plugins);

        // This is a bit of a hack to get around the borrow checker.  Calling the runner directly
        // from self.runner will consume the runner while also consuming the [`App`] instance, as
        // the runner is of type `FnOnce(App)`.  But the app contains the runner!  The borrow
        // checker rightly complains that the application is already moved when the runner is
        // called, as the compiler needs to consume app twice: once to extract and use the runner,
        // and again as an argument to the runner.  This obviously won't work.
        //
        // Rather than attempt some `unsafe{}`` black magic, we swap out the runner, whatever it
        // was, for another boxed runner value.  We could then call the extracted runner, enabling
        // the App instance and its dummy runner replacement to be consumed.
        let runner = std::mem::replace(&mut self.runner, Box::new(run_once));

        // Returns an AppExit value from the runner.
        (runner)(self)
    }

    pub fn should_exit(&self) -> Option<AppExit> {
        // We manually construct an message reader to see if there are any queued AppExit messages.
        // Returns None if there is no AppExit message queue in the world.
        let mut reader = MessageCursor::default();
        let messages = self.get_resource::<Messages<AppExit>>()?;
        let mut messages = reader.read(messages);

        // If there are no messages in the queue, then shutdown has not been requested.
        if messages.len() == 0 {
            return None;
        }

        // Otherwise *at least one* termination message has been generated.  It's possible that there
        // is more than one, and if any one of them is an error, we return it.  Otherwise they must
        // be AppExit::Success, and we return that.
        Some(
            messages
                .find(|exit| exit.is_err())
                .cloned()
                .unwrap_or(AppExit::Success),
        )
    }
}

impl ContainsWorld for App {
    /// Returns a reference to the application's ECS [`World`], which contains the global repository of
    /// program state.  The world is managed by the [`ecs`] ECS library.
    fn world(&self) -> &World {
        &self.world
    }

    /// Returns a mutable reference to the application's ECS [`World`], which contains the global
    /// repository of program state.  The world is managed by the [`ecs`] ECS library.
    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::{Last, PreStartup, Startup, Update};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    /// Create and configure a basic App.
    #[test]
    fn test_app_creation_and_configuration() {
        // Create a new app with a name.
        let mut app = App::new("MyTestApp".into());

        // App configuration methods can be chained for a fluent API.
        app.set_name("RenamedApp".into())
            .set_update_schedule(Update)
            .set_runner(|_| AppExit::Success);

        // App names are used for UI and logging purposes.
        assert_eq!(app.name(), "RenamedApp");
    }

    /// Use run_once for a single execution of systems.
    #[test]
    fn test_run_once() {
        // Create a counter to track how many times our systems run.
        let update_counter = Arc::new(AtomicUsize::new(0));

        // Create a new app
        let mut app = App::new("RunOnceTest".into());

        // Add a system to the Update schedule that increments the counter
        let counter = update_counter.clone();
        app.add_systems(Update, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        // Use run_once function explicitly
        let exit_code = run_once(&mut app);
        assert_eq!(exit_code, AppExit::Success);
        assert_eq!(update_counter.load(Ordering::SeqCst), 1);

        // Run the system again
        let exit_code = run_once(&mut app);
        assert_eq!(exit_code, AppExit::Success);
        assert_eq!(update_counter.load(Ordering::SeqCst), 2);

        // run_once is the default runner, so we can just call run()
        let exit_code = app.run();
        assert_eq!(exit_code, AppExit::Success);
        assert_eq!(update_counter.load(Ordering::SeqCst), 3);
    }

    /// Add and run systems in proper schedules.
    #[test]
    fn test_system_scheduling() {
        // Every time a system runs it appends a unique debugging string to this vector.
        let exec_order = Arc::new(Mutex::new(Vec::new()));

        // Create a new app.
        let mut app = App::new("ScheduleDemo".into());

        // Add systems to different schedules to demonstrate execution order.
        // Note that we're adding them out of order to show that schedule order
        // is determined by the framework, not by the order of registration.
        app.add_systems(Last, {
            let exec_order = exec_order.clone();
            move || exec_order.lock().unwrap().push("last")
        });

        app.add_systems(Update, {
            let exec_order = exec_order.clone();
            move || exec_order.lock().unwrap().push("update")
        });

        app.add_systems(Startup, {
            let exec_order = exec_order.clone();
            move || exec_order.lock().unwrap().push("startup")
        });

        app.add_systems(PreStartup, {
            let exec_order = exec_order.clone();
            move || exec_order.lock().unwrap().push("prestartup")
        });

        // Running the app will execute all systems in the proper order
        app.run();

        // The execution order demonstrates the framework's schedule ordering
        assert_eq!(
            *exec_order.lock().unwrap(),
            vec!["prestartup", "startup", "update", "last"]
        );
    }

    /// Shows how to use the application plugin system
    #[test]
    fn test_plugin_system() {
        // Define a simple plugin for testing
        struct CounterPlugin(Arc<AtomicUsize>);

        impl Plugin for CounterPlugin {
            fn register(&self, app: &mut App) {
                // Plugins can modify the app during registration
                self.0.fetch_add(1, Ordering::SeqCst);
                app.add_systems(Update, move || {
                    // Systems added by plugins work like any other system
                });
            }
        }

        struct NonUniquePlugin;
        impl Plugin for NonUniquePlugin {
            fn register(&self, _app: &mut App) {}
            fn is_unique(&self) -> bool {
                false
            }
        }

        // Using a plugin to configure the app
        let counter = Arc::new(AtomicUsize::new(0));
        let mut app = App::new("PluginDemo".into());

        // Adding a plugin calls its register method
        app.add_plugin(CounterPlugin(counter.clone()));
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Non-unique plugins can be added multiple times
        app.add_plugin(NonUniquePlugin);
        app.add_plugin(NonUniquePlugin);

        // The app can be further configured after adding plugins
        app.set_update_schedule(Update);
    }

    /// Use custom runners and control loop behavior.
    #[test]
    fn test_custom_runners() {
        // Tracking whether our custom runner was called
        static RUNNER_CALLED: AtomicBool = AtomicBool::new(false);

        // Define a custom application runner
        let mut app = App::new("RunnerDemo".into());
        app.set_runner(|app| {
            RUNNER_CALLED.store(true, Ordering::SeqCst);

            // Custom runners can control exactly how systems are executed
            app.update(); // Run one cycle of systems

            // Custom runners decide when and how to terminate
            AppExit::Success
        });

        // run() transfers control to the runner
        app.run();
        assert!(RUNNER_CALLED.load(Ordering::SeqCst));

        // Example of a headless app with no update schedule
        let mut headless_app = App::empty("HeadlessDemo".into());
        static SYSTEM_RAN: AtomicBool = AtomicBool::new(false);

        // Without a schedule, systems won't execute during update()
        headless_app.add_systems(Update, || {
            SYSTEM_RAN.store(true, Ordering::SeqCst);
        });

        headless_app.update();
        assert!(!SYSTEM_RAN.load(Ordering::SeqCst));
    }

    /// Demonstrates how application shutdown works
    #[test]
    fn test_app_exit() {
        // Scenario 1: App exits successfully
        let mut app = App::new("ExitDemo".into());

        // Systems can request app termination by sending an AppExit event
        app.add_systems(Update, |mut exit: MessageWriter<AppExit>| {
            exit.write(AppExit::Success);
        });

        app.set_runner(|app| {
            // Loop forever until the app is instructed to exit.
            // (Will exit on the very first run through.)
            loop {
                app.update();
                if let Some(exit) = app.should_exit() {
                    return exit;
                }
            }
        });

        // When run() returns, it provides the exit status
        let result = app.run();
        assert_eq!(result, AppExit::Success);

        // Scenario 2: App exits with an error code
        let mut app = App::new("ErrorExitDemo".into());
        let error_code = NonZero::new(42).unwrap();

        app.add_systems(Update, move |mut exit: MessageWriter<AppExit>| {
            // You can provide specific error codes for different error conditions
            exit.write(AppExit::Error(error_code));
        });

        // The error code is preserved and returned to the caller
        let result = app.run();
        assert_eq!(result, AppExit::Error(error_code));

        // How exit codes map to process exit codes:
        let success_exit = AppExit::Success;
        let error_exit = AppExit::Error(NonZero::new(75).unwrap());

        // AppExit implements Termination for integration with main()
        assert_eq!(success_exit.report(), ExitCode::SUCCESS);
        assert_eq!(error_exit.report(), ExitCode::from(75));
    }
}

// End of File
