// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This is the main application crate for atomCAD.  It contains the main
//! windowing event loop, implementations of user interface elements and
//! associated application logic, and the platform-specific code for
//! initializing the application and handling events.  It also contains a fair
//! amount of other functionality that has not yet been moved into separate
//! crates.
//!
//! atomCAD is implemented as a single-window application, with a 3D view
//! showing the molecular parts and aseemblies being edited, and an overlay of
//! various tool widgets optimized for multi-touch interfaces.  The 3D view is
//! implemented using the [wgpu] crate, and the overlay is implemented using
//! [rui].  Native APIs are used whenever possible for other required user
//! interface elements.
//!
//! As of this writing, the application is still in the early stages of
//! development, and is not yet usable for any practical purpose.  The
//! following features are currently implemented:
//!
//! * A basic 3D view, with a camera that can be controlled using the mouse
//!   and keyboard.
//!
//! * A basic menu bar, with a File menu that can be used to open PDB files.
//!
//! As is common with binary applications, the main entry point is in the
//! `main.rs` file, and the rest of the application is implemented in this
//! crate, so that it is accessible to integration tests.
//!
//! [wgpu]: https://crates.io/crates/wgpu
//! [rui]: https://crates.io/crates/rui

/// The API for controlling the camera in the 3D view, and having it respond
/// to user events.
pub mod camera;
/// A platform-independent abstraction over the windowing system's interface
/// for menus and menubars.  Used to setup the application menubar on startup.
pub mod menubar;
/// A module for loading and parsing PDB files.
///
/// TODO: Should probably be abstracted into its own crate.
pub mod pdb;

// This module is not public.  It is a common abstraction over the various
// platform-specific APIs.  For example, `platform::menubar` exposes an API
// for taking a platform-independent `menubar::Menu` and instantiating it in
// the windowing system and attaching it to either the window or application
// object, as required.
//
// The APIs exposed by this module are meant to be called from the rest of the
// `atomCAD` crate.
pub(crate) mod platform;
// This module contains the platform-specific native API code used by
// `platform`.  It is not intended to be used directly by any other code.  In
// the future it may be moved to be a private submodule of `platform`.
pub(crate) mod platform_impl;

/// The user-visible name of the application, used for window titles and such.
pub const APP_NAME: &str = "atomCAD";

// End of File
