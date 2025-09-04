use glam::f64::{DVec2, DVec3};

/*
 * Any 3D geometry that can be implicitly evaluated.
 */
pub trait ImplicitGeometry3D {
  fn get_gradient(
    &self,
    sample_point: &DVec3
  ) -> (DVec3, f64);

  fn implicit_eval_3d(&self, sample_point: &DVec3) -> f64;
}

/*
 * Any 2D geometry that can be implicitly evaluated.
 */
pub trait ImplicitGeometry2D {
  fn get_gradient_2d(
    &self,
    sample_point: &DVec2,
  ) -> (DVec2, f64);

  fn implicit_eval_2d(&self, sample_point: &DVec2) -> f64;
}