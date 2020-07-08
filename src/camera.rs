use na::{Matrix4, Quaternion, UnitQuaternion, Vector2, Vector3};
use winit::dpi::{PhysicalPosition, PhysicalSize};

pub struct Camera {
    translation: Matrix4<f32>,
    center_translation: Matrix4<f32>,
    rotation: Quaternion<f32>,
    camera: Matrix4<f32>,
    inv_camera: Matrix4<f32>,
    zoom_speed: f32,
    inv_size: PhysicalSize<f32>,
}

impl Camera {
    pub fn new(center: Vector3<f32>, zoom_speed: f32, size: PhysicalSize<u32>) -> Self {
        let mut cam = Self {
            translation: Matrix4::new_translation(&Vector3::new(0.0, 0.0, -1.0)),
            center_translation: Matrix4::new_translation(&center).try_inverse().unwrap(),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            camera: Matrix4::identity(),
            inv_camera: Matrix4::identity(),
            zoom_speed,
            inv_size: PhysicalSize::new(1.0 / size.width as f32, 1.0 / size.height as f32),
        };
        cam.update_camera();
        cam
    }

    pub fn get_camera(&self) -> Matrix4<f32> {
        self.camera
    }

    pub fn get_inv_camera(&self) -> Matrix4<f32> {
        self.inv_camera
    }

    pub fn rotate(&mut self, old_cursor: PhysicalPosition<u32>, new_cursor: PhysicalPosition<u32>) {
        let mouse_current = Vector2::new(
            na::clamp(
                new_cursor.x as f32 * 2.0 * self.inv_size.width - 1.0,
                -1.0,
                1.0,
            ),
            na::clamp(
                1.0 - 2.0 * new_cursor.y as f32 * self.inv_size.height,
                -1.0,
                1.0,
            ),
        );
        let mouse_previous = Vector2::new(
            na::clamp(
                old_cursor.x as f32 * 2.0 * self.inv_size.width - 1.0,
                -1.0,
                1.0,
            ),
            na::clamp(
                1.0 - 2.0 * old_cursor.y as f32 * self.inv_size.height,
                -1.0,
                1.0,
            ),
        );

        let mouse_cur_ball = Self::screen_to_arcball(mouse_current);
        let mouse_prev_ball = Self::screen_to_arcball(mouse_previous);

        self.rotation = mouse_cur_ball * mouse_prev_ball * self.rotation;
        self.update_camera();
    }

    pub fn zoom(&mut self, amount: f32) {
        let motion = Vector3::new(0.0, 0.0, amount);
        self.translation = Matrix4::new_translation(&(motion * self.zoom_speed)) * self.translation;
        self.update_camera();
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.inv_size = PhysicalSize::new(1.0 / size.width as f32, 1.0 / size.height as f32);
    }

    fn update_camera(&mut self) {
        self.camera = self.translation
            * UnitQuaternion::new_unchecked(self.rotation).to_homogeneous()
            * self.center_translation;
        self.inv_camera = self.camera.try_inverse().unwrap();
    }

    fn screen_to_arcball(p: Vector2<f32>) -> Quaternion<f32> {
        let dist = p.dot(&p);

        if dist <= 1.0 {
            Quaternion::new(0.0, p.x, p.y, f32::sqrt(1.0 - dist))
        } else {
            let unit_p = p.normalize();
            Quaternion::new(0.0, unit_p.x, unit_p.y, 0.0)
        }
    }
}
