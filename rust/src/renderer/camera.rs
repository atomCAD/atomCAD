use glam::f64::DMat3;
use glam::f64::DMat4;
use glam::f64::DQuat;
use glam::f64::DVec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraCanonicalView {
    Custom,
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
}

pub struct Camera {
    pub eye: DVec3,
    pub target: DVec3,
    pub up: DVec3,
    pub aspect: f64,
    pub fovy: f64, // in radians
    pub znear: f64,
    pub zfar: f64,
    pub orthographic: bool,
    pub ortho_half_height: f64,
    pub pivot_point: DVec3,
    /// Resolved world-space unit vector that acts as the turntable's
    /// screen-vertical ("navigation up"). Default `+Z`. See issue #349 /
    /// `doc/design_view_up_axis.md` (D1).
    pub nav_up: DVec3,
    /// Cosmetic provenance label for `nav_up` (e.g. `"Z"`, `"(1 1 1)"`,
    /// `"[1 1 0]"`). Lives on `Camera` because `sync_camera_to_active_network`
    /// rebuilds `CameraSettings` from `Camera` on every camera move — a label
    /// stored only on `CameraSettings` would be wiped by the first drag.
    pub nav_up_label: String,
}

impl Camera {
    pub fn build_view_matrix(&self) -> DMat4 {
        DMat4::look_at_rh(self.eye, self.target, self.up)
    }

    pub fn build_projection_matrix(&self) -> DMat4 {
        if self.orthographic {
            // Calculate the orthographic projection matrix
            let right = self.ortho_half_height * self.aspect;
            DMat4::orthographic_rh(
                -right,
                right,
                -self.ortho_half_height,
                self.ortho_half_height,
                self.znear,
                self.zfar,
            )
        } else {
            DMat4::perspective_rh_gl(self.fovy, self.aspect, self.znear, self.zfar)
        }
    }

    pub fn build_view_projection_matrix(&self) -> DMat4 {
        let view = self.build_view_matrix();
        let proj = self.build_projection_matrix();
        // println!("Projection matrix: {:?}", proj);
        proj * view
    }

    pub fn calc_headlight_direction(&self) -> DVec3 {
        let forward = (self.target - self.eye).normalize();
        let right = forward.cross(self.up).normalize();

        // Create a quaternion for a slight downward rotation (20 degrees)
        let angle_in_radians = 20.0_f64.to_radians();
        let rotation = DQuat::from_axis_angle(right, -angle_in_radians);

        rotation * forward
    }

    /// Builds the rotated navigation basis (D4): the rotation taking the world
    /// basis to the frame whose `Z'` is `nav_up` and whose side axes stay as
    /// world-aligned as possible.
    ///
    /// ```text
    /// Z' = nav_up
    /// Y' = normalize(Y − Z'·(Y·Z'))    // world +Y projected ⊥ nav_up
    ///      (fallback when nav_up ∥ ±Y: Y' = normalize(Z − Z'·(Z·Z')))
    /// X' = Y' × Z'
    /// ```
    ///
    /// Reduces to the identity for `nav_up = +Z`, so canonical views under the
    /// default axis are byte-identical to the pre-feature behavior.
    pub fn nav_frame(&self) -> DQuat {
        let z_axis = self.nav_up.normalize();
        // World +Y projected onto the plane perpendicular to nav_up.
        let y_proj = DVec3::Y - z_axis * DVec3::Y.dot(z_axis);
        let y_axis = if y_proj.length() < 1e-6 {
            // nav_up ∥ ±Y — fall back to projecting world +Z instead.
            (DVec3::Z - z_axis * DVec3::Z.dot(z_axis)).normalize()
        } else {
            y_proj.normalize()
        };
        let x_axis = y_axis.cross(z_axis);
        DQuat::from_mat3(&DMat3::from_cols(x_axis, y_axis, z_axis))
    }

    /// Re-aligns `up` so `nav_up` reads as screen-vertical, by a pure roll about
    /// the current forward vector (D3). Eye, target, and forward are unchanged.
    /// No-op in the degenerate case where forward ∥ ±nav_up (any roll is equally
    /// valid), mirroring the existing pole guard in the Flutter turntable.
    pub fn realign_up_to_nav_axis(&mut self) {
        let forward = (self.target - self.eye).normalize();
        // nav_up projected onto the plane perpendicular to forward.
        let up_proj = self.nav_up - forward * self.nav_up.dot(forward);
        if up_proj.length() < 1e-6 {
            // forward ∥ ±nav_up: keep the current up unchanged.
            return;
        }
        self.up = up_proj.normalize();
    }

    /// Restores the default navigation axis (`+Z` / `"Z"`) and re-aligns `up`
    /// per D3. Used by the D8 `None`-restore rule and the `reset_view_up` API.
    pub fn reset_nav_up(&mut self) {
        self.nav_up = DVec3::Z;
        self.nav_up_label = "Z".to_string();
        self.realign_up_to_nav_axis();
    }

    pub fn get_canonical_view(&self) -> CameraCanonicalView {
        // Calculate view direction (from eye to target)
        let view_dir = (self.target - self.eye).normalize();

        // Check for alignment with cardinal axes
        // We use a small epsilon for floating point comparison
        const EPSILON: f64 = 0.001;

        // Canonical views follow the navigation frame (D4): compare the view
        // direction against the cardinal directions rotated into that frame, so
        // the dropdown indicator stays consistent under a tilted nav_up.
        let frame = self.nav_frame();
        let rotated = |v: DVec3| frame * v;

        // These direction checks must match the directions set in set_canonical_view
        // Z-up coordinate system: X=right, Y=forward, Z=up
        if (view_dir - rotated(DVec3::new(-1.0, 0.0, 0.0))).length_squared() < EPSILON {
            return CameraCanonicalView::Right;
        } else if (view_dir - rotated(DVec3::new(1.0, 0.0, 0.0))).length_squared() < EPSILON {
            return CameraCanonicalView::Left;
        } else if (view_dir - rotated(DVec3::new(0.0, 0.0, -1.0))).length_squared() < EPSILON {
            return CameraCanonicalView::Top;
        } else if (view_dir - rotated(DVec3::new(0.0, 0.0, 1.0))).length_squared() < EPSILON {
            return CameraCanonicalView::Bottom;
        } else if (view_dir - rotated(DVec3::new(0.0, -1.0, 0.0))).length_squared() < EPSILON {
            return CameraCanonicalView::Back;
        } else if (view_dir - rotated(DVec3::new(0.0, 1.0, 0.0))).length_squared() < EPSILON {
            return CameraCanonicalView::Front;
        }

        // If not aligned with any cardinal direction, return Custom
        CameraCanonicalView::Custom
    }

    pub fn set_canonical_view(&mut self, view: CameraCanonicalView) {
        // If view is Custom, do nothing
        if matches!(view, CameraCanonicalView::Custom) {
            return;
        }

        // Define a constant distance for canonical views
        const CANONICAL_DISTANCE: f64 = 40.0;

        // Set target to origin
        self.target = DVec3::new(0.0, 0.0, 0.0);

        // Define the viewing direction and up vectors for each canonical view
        // Z-up coordinate system: X=right, Y=forward, Z=up
        let (view_dir, up) = match view {
            CameraCanonicalView::Top => (
                DVec3::new(0.0, 0.0, -1.0), // Looking down from +Z
                DVec3::new(0.0, -1.0, 0.0), // Up is -Y (screen up when looking down)
            ),
            CameraCanonicalView::Bottom => (
                DVec3::new(0.0, 0.0, 1.0), // Looking up from -Z
                DVec3::new(0.0, 1.0, 0.0), // Up is +Y (screen up when looking up)
            ),
            CameraCanonicalView::Front => (
                DVec3::new(0.0, 1.0, 0.0), // Looking from -Y (towards +Y)
                DVec3::new(0.0, 0.0, 1.0), // Up is +Z
            ),
            CameraCanonicalView::Back => (
                DVec3::new(0.0, -1.0, 0.0), // Looking from +Y (towards -Y)
                DVec3::new(0.0, 0.0, 1.0),  // Up is +Z
            ),
            CameraCanonicalView::Left => (
                DVec3::new(1.0, 0.0, 0.0), // Looking from -X (towards +X)
                DVec3::new(0.0, 0.0, 1.0), // Up is +Z
            ),
            CameraCanonicalView::Right => (
                DVec3::new(-1.0, 0.0, 0.0), // Looking from +X (towards -X)
                DVec3::new(0.0, 0.0, 1.0),  // Up is +Z
            ),
            CameraCanonicalView::Custom => {
                // This shouldn't happen because of the check at the beginning
                // But we provide a default value for completeness
                (DVec3::new(0.0, 1.0, 0.0), DVec3::new(0.0, 0.0, 1.0))
            }
        };

        // Canonical views follow the navigation frame (D4): rotate both the
        // table's view direction and up vector into the nav frame before use.
        // For nav_up = +Z this is the identity, so behavior is unchanged.
        let frame = self.nav_frame();
        let view_dir = frame * view_dir;
        let up = frame * up;

        // Set eye position at CANONICAL_DISTANCE away from the origin in the view direction
        // We subtract the view_dir because we want to look toward the target from that direction
        self.eye = self.target - view_dir * CANONICAL_DISTANCE;

        // Set the up direction
        self.up = up;

        // If in orthographic mode, adjust ortho_half_height based on fovy
        if self.orthographic {
            // Calculate ortho_half_height that would give the same view frustum at the target distance
            // tan(fovy/2) * distance gives the half-height of the view frustum at that distance
            self.ortho_half_height = (self.fovy / 2.0).tan() * CANONICAL_DISTANCE;
        }
    }
}
