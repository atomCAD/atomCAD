use glam::f64::{DVec2, DVec3};

pub const BATCH_SIZE: usize = 1024;

/*
 * Any 3D geometry that can be implicitly evaluated.
 */
pub trait ImplicitGeometry3D {
  fn get_gradient(
    &self,
    sample_point: &DVec3
  ) -> (DVec3, f64);

  fn implicit_eval_3d(&self, sample_point: &DVec3) -> f64;

  fn implicit_eval_3d_batch(&self, sample_points: &[DVec3; BATCH_SIZE], results: &mut [f64; BATCH_SIZE]);

  fn is3d(&self) -> bool;
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

  fn implicit_eval_2d_batch(&self, sample_points: &[DVec2; BATCH_SIZE], results: &mut [f64; BATCH_SIZE]);

  fn is2d(&self) -> bool;
}















