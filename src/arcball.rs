use crate::Settings;
use glm::{self, Vec2, Vec3, Vec4, Mat4x4, Quat};

pub struct Arcball {
    center_pos: Vec3,
    camera_pos: Vec3,
    last_mouse: Option<Vec2>,
}

impl Arcball {
    pub fn new(center_pos: Vec3, camera_pos: Vec3) -> Self {
        Self {
            center_pos,
            camera_pos,
            last_mouse: None,
        }
    }

    pub fn get_quat(
        &mut self,
        view_matrix: Mat4x4,
        new_pos: Vec2,
    ) -> Option<Quat> {
        self.last_mouse.replace(new_pos).map(|last_pos| {
            let sphere_radius = self.camera_pos.z - self.center_pos.z;

            let pos_on_sphere = |pos: Vec2| {
                let v = pos * sphere_radius;
                let vec = view_matrix * Vec4::new(v.x, v.y, 0.0, 0.0);

                let on_sphere = Vec3::new(
                    -vec.x,
                    -vec.y,
                    ((sphere_radius * sphere_radius) - (pos.x * pos.x) - (pos.y * pos.y)).sqrt(),
                );

                on_sphere.normalize()
            };

            let start_on_sphere = pos_on_sphere(last_pos);
            let end_on_sphere = pos_on_sphere(new_pos);

            glm::quat_rotation(&start_on_sphere, &end_on_sphere)
        })

        // self.last_mouse.replace(new_pos).map(|last_pos| {
        //     let sphere_radius = self.camera_pos.z - self.center_pos.z;

            

        //     // do raytrace to find point
        //     let pos_on_sphere = |pos: Point2<f32>| {
        //         let vec: Vector4<f32> =
        //             view_matrix * (pos.to_vec() * sphere_radius).extend(0.0).extend(0.0);

        //         let on_sphere = Vector2::new(-vec.x, -vec.y).extend(
        //             ((sphere_radius * sphere_radius) - (pos.x * pos.x) - (pos.y * pos.y)).sqrt(),
        //         );

        //         on_sphere.normalize()
        //     };

        //     let start_on_sphere = pos_on_sphere(last_pos);
        //     let end_on_sphere = pos_on_sphere(new_pos);

        //     (Quaternion::from_arc(start_on_sphere, end_on_sphere, None) * 10.0).normalize()
        // })
    }

    pub fn release(&mut self) {
        self.last_mouse = None;
    }
}
