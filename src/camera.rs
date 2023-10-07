// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

use bevy::{
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    prelude::*,
};
// Most of the heavy lifting is accomplished by the smooth_bevy_cameras crate.
use smooth_bevy_cameras::{
    LookAngles, LookTransform, LookTransformBundle, LookTransformPlugin, Smoother,
};

/// The `CameraPlugin` manages all cameras in the scene.  It loads a separate
/// plugin to handle each camera type.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        // Only one camera system is implemented so far, that of the CAD view.
        app.add_plugins(CadViewPlugin);
    }
}

/// In order to identify the camera used for the CAD view, we mark it with the
/// `CadViewCamera` entity tag.  This prevents us from accidentally removing
/// some other camera during cleanup, for example.
#[derive(Component)]
pub struct CadViewCamera;

/// The `CadViewPlugin` manages the `Camera3d` object for the CAD viewport.
/// It handles initialization of the camera, user input, and smoothing of the
/// camera movement.
struct CadViewPlugin;

impl Plugin for CadViewPlugin {
    fn build(&self, app: &mut App) {
        app
            // The `LookTransformPlugin` from smooth_bevy_cameras handles all
            // the details of smoothing out camera movement.  All we need to
            // do is specify the correct `LookTransform` based on user input,
            // and it will handle the details of smoothing out motion across
            // multiple frames.
            .add_plugins(LookTransformPlugin)
            //
            .add_event::<CadViewControlEvent>()
            // The input handler checks for user input and converts raw input
            // state (e.g. mouse motion, wheel state, keys pressed) into one
            // or more `CadViewControlEvent` enums, representing a pan, orbit,
            // or zoom by a specified amount.
            .add_systems(Update, cad_view_input_handler)
            // The controller takes the `CadViewControlEvent` enums generated
            // by the input handler, and applies them to the `LookTransform`
            // of the currently active camera.  The smoothing of camera motion
            // is handled by `LookTransformPlugin` of `smooth_bevy_cameras`.
            .add_systems(Update, cad_view_controller);
    }
}

#[derive(Bundle)]
pub struct CadViewBundle {
    controller: CadViewController,
    settings: CadViewControllerSettings,
    look_transform: LookTransformBundle,
    transform: Transform,
    tag: CadViewCamera,
}

impl CadViewBundle {
    pub fn new(
        settings: CadViewControllerSettings,
        position: Vec3,
        target: Vec3,
        up: Vec3,
    ) -> Self {
        let transform = Transform::from_translation(position).looking_at(target, up);
        let smoothing_weight = settings.smoothing_weight;

        Self {
            controller: CadViewController { enabled: true },
            settings,
            look_transform: LookTransformBundle {
                transform: LookTransform::new(position, target, up),
                smoother: Smoother::new(smoothing_weight),
            },
            transform,
            tag: CadViewCamera,
        }
    }
}

// The following code is based on the built-in `OrbitCameraController` from
// smooth_bevy_cameras, with minimal tweaks (so far).  The built-in controller
// wasn't perfect for our needs, and unfortunately not modular enough that we
// could just add in our own tweaks.  So we've effectively forked it to
// provide the changes we need.

#[derive(Component)]
pub struct CadViewControllerSettings {
    /// If pressed, the `alt_button` flips the behavior of the `orbit_button`
    /// and `pan_button`.  This is particularly useful on systems like macOS
    /// where there is only one mouse button.  Although named `alt_button`,
    /// the default settings use the Control key.
    pub alt_button: KeyCode,
    /// The mouse button used to orbit the camera, rotating the field of view
    /// around the target.  If `alt_button` is pressed, this button is used to
    /// pan the camera instead.
    pub orbit_button: MouseButton,
    /// The mouse button used to pan the camera, translating the target in the
    /// current view plane.  If `alt_button` is pressed, this button is used to
    /// orbit the camera instead.
    pub pan_button: MouseButton,
    /// The sensitivity of the mouse / touchpad when orbiting the camera.
    pub rotate_sensitivity: Vec2,
    /// The sensitivity of the mouse / touchpad when panning the camera.
    pub translate_sensitivity: Vec2,
    /// The sensitivity of the mouse wheel / two finger touch when zooming the
    /// camera.
    pub zoom_sensitivity: f32,
    /// Some mice wheels report motion as the number of lines to scroll (in a
    /// more traditional text editor or web browser applicaiton), while others
    /// offer more granular reporting as the number of pixels to scroll.  This
    /// field provides the conversion between the two.
    pub pixels_per_line: f32,
    /// The minimum zoom distance.  This is the distance from the camera to
    /// the target.  The camera can get locked if you get too close, so this
    /// provides a safety mechanism.
    pub min_zoom: f32,
    // The maximum zoom distance.  This is the distance from the camera to the
    // target.  If you get too far away, the entire scene will be beyond the
    // far clip plane and not visible.
    pub max_zoom: f32,
    /// An internal exponential smoothing parameter between 0 and 1 used to
    /// smooth out the camera movement.
    pub smoothing_weight: f32,
}

impl Default for CadViewControllerSettings {
    fn default() -> Self {
        CadViewControllerSettings {
            alt_button: KeyCode::ControlLeft,
            orbit_button: MouseButton::Left,
            pan_button: MouseButton::Right,
            rotate_sensitivity: Vec2::splat(1.),
            translate_sensitivity: Vec2::splat(0.15),
            zoom_sensitivity: 0.005,
            pixels_per_line: 32.0,
            min_zoom: 0.1,
            max_zoom: 1000.0,
            smoothing_weight: 0.8,
        }
    }
}

#[derive(Component)]
pub struct CadViewController {
    /// Whether the controller is processing user input.  There could be many
    /// `CadViewController`s in the ECS at any given time, but only one should
    /// be active (updating based on input).
    pub enabled: bool,
}

impl Default for CadViewController {
    fn default() -> Self {
        CadViewController { enabled: true }
    }
}

#[derive(Event)]
enum CadViewControlEvent {
    Orbit(Vec2),
    Pan(Vec2),
    Zoom(f32),
}

fn cad_view_input_handler(
    mut events: EventWriter<CadViewControlEvent>,
    mut ev_motion: EventReader<MouseMotion>,
    mut ev_scroll: EventReader<MouseWheel>,
    mouse_buttons: Res<Input<MouseButton>>,
    keyboard: Res<Input<KeyCode>>,
    controllers: Query<(&CadViewController, &CadViewControllerSettings)>,
) {
    // Can only control one camera at a time.
    // Make sure that all other cameras are disabled.
    let settings = if let Some((_, settings)) = controllers.iter().find(|(c, _)| c.enabled) {
        settings
    } else {
        return;
    };

    // Extract out the configuration options we care about.
    let CadViewControllerSettings {
        alt_button,
        orbit_button,
        pan_button,
        rotate_sensitivity,
        translate_sensitivity,
        zoom_sensitivity,
        pixels_per_line,
        ..
    } = *settings;

    // How much has the mouse moved?
    let cursor_delta: Vec2 = ev_motion.iter().map(|ev| ev.delta).sum();

    if cursor_delta.x.abs() > 0. || cursor_delta.y.abs() > 0. {
        let alt_pressed = keyboard.pressed(alt_button);
        let pan_pressed = mouse_buttons.pressed(pan_button);
        let orbit_pressed = mouse_buttons.pressed(orbit_button);

        // If the orbit button is pressed, orbit the camera.
        if (!alt_pressed && orbit_pressed) || (alt_pressed && pan_pressed) {
            events.send(CadViewControlEvent::Orbit(
                cursor_delta * rotate_sensitivity,
            ));
        }
        // If the pan button is pressed, translate the camera target.
        if (!alt_pressed && pan_pressed) || (alt_pressed && orbit_pressed) {
            events.send(CadViewControlEvent::Pan(
                cursor_delta * translate_sensitivity,
            ));
        }
    }

    // How much has the scroll wheel rotated?
    let scroll: f32 = ev_scroll
        .iter()
        .map(|ev| match ev.unit {
            // Some mice report lines scrolled, some report pixels.
            MouseScrollUnit::Line => ev.y * pixels_per_line,
            MouseScrollUnit::Pixel => ev.y,
        })
        .map(|x| 1. - x * zoom_sensitivity)
        .product();

    // If the scroll wheel moved, zoom the camera.
    if scroll.abs() > 0. {
        events.send(CadViewControlEvent::Zoom(scroll));
    }
}

fn cad_view_controller(
    time: Res<Time>,
    mut events: EventReader<CadViewControlEvent>,
    mut cameras: Query<(
        &CadViewController,
        &CadViewControllerSettings,
        &mut LookTransform,
        &Transform,
    )>,
) {
    // Can only control one camera at a time.
    // Make sure that all other cameras are disabled.
    let (settings, mut look_transform, transform) =
        if let Some((_, settings, look_transform, transform)) =
            cameras.iter_mut().find(|q| q.0.enabled)
        {
            (settings, look_transform, transform)
        } else {
            return;
        };

    let CadViewControllerSettings {
        min_zoom, max_zoom, ..
    } = *settings;

    let mut look_angles = LookAngles::from_vector(-look_transform.look_direction().unwrap());
    let mut zoom = 1.0;
    let radius = look_transform.radius();

    let dt = time.delta_seconds();
    for event in events.iter() {
        match event {
            CadViewControlEvent::Orbit(delta) => {
                look_angles.add_yaw(dt * -delta.x);
                look_angles.add_pitch(dt * delta.y);
            }
            CadViewControlEvent::Pan(delta) => {
                let right = -transform.local_x();
                let up = transform.local_y();
                look_transform.target += (dt * delta.x * right + dt * delta.y * up) * radius;
            }
            CadViewControlEvent::Zoom(delta) => {
                zoom *= delta;
            }
        }
    }

    // LookAngles should prevent this from ever happening.
    look_angles.assert_not_looking_up();

    look_transform.eye = look_transform.target
        + (radius * zoom).clamp(min_zoom, max_zoom) * look_angles.unit_vector();
}

// End of File
