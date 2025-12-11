use csgrs::traits::CSG;
use glam::f64::{DVec2, DVec3, DQuat};
use nalgebra::{Point3, Vector3};
use crate::geo_tree::csg_types::CSGMesh;
use crate::geo_tree::csg_types::CSGSketch;
use super::{GeoNode, GeoNodeKind};
use super::csg_cache::CsgConversionCache;
use crate::util::transform::Transform;
use crate::geo_tree::csg_utils::dvec3_to_point3;
use crate::geo_tree::csg_utils::dvec3_to_vector3;
use crate::geo_tree::csg_utils::scale_to_csg;
use csgrs::mesh::polygon::Polygon;
use csgrs::mesh::vertex::Vertex;
use crate::util::timer::Timer;

impl GeoNode {
  /// Convert to CSG mesh without caching
  pub fn to_csg_mesh(&self) -> Option<CSGMesh> {
    //let _timer = Timer::new("GeoNode::to_csg_mesh");
    self.internal_to_csg_mesh(true, None)
  }

  /// Convert to CSG mesh with optional caching
  pub fn to_csg_mesh_cached(&self, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGMesh> {
    //let _timer = Timer::new("GeoNode::to_csg_mesh_cached");
    self.internal_to_csg_mesh(true, cache.as_deref_mut())
  }

  /// Convert to CSG sketch without caching
  pub fn to_csg_sketch(&self) -> Option<CSGSketch> {
    self.to_csg_sketch_cached(None)
  }

  /// Convert to CSG sketch with optional caching
  pub fn to_csg_sketch_cached(&self, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGSketch> {
    self.internal_to_csg_sketch(cache.as_deref_mut())
  }

  fn internal_to_csg_sketch(&self, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGSketch> {
    // Check cache first
    if let Some(cache_ref) = cache.as_deref_mut() {
      if let Some(cached) = cache_ref.get_sketch(self.hash()) {
        return Some((*cached).clone());
      }
    }
    
    // Compute if not cached
    let result = match &self.kind {
      GeoNodeKind::HalfPlane { point1, point2 } => {
        Some(Self::half_plane_to_csg(*point1, *point2))
      }
      GeoNodeKind::Circle { center, radius } => {
        Some(Self::circle_to_csg(*center, *radius))
      }
      GeoNodeKind::Polygon { vertices } => {
        Some(Self::polygon_to_csg(vertices))
      }
      GeoNodeKind::Union2D { shapes } => {
        Self::union_2d_to_csg(shapes, cache.as_deref_mut())
      }
      GeoNodeKind::Intersection2D { shapes } => {
        Self::intersection_2d_to_csg(shapes, cache.as_deref_mut())
      }
      GeoNodeKind::Difference2D { base, sub } => {
        Self::difference_2d_to_csg(base, sub, cache.as_deref_mut())
      }
      _ => None
    }?;
    
    // Store in cache
    if let Some(cache_ref) = cache.as_deref_mut() {
      cache_ref.insert_sketch(*self.hash(), result.clone());
    }
    
    Some(result)
  }

  fn internal_to_csg_mesh(&self, is_root: bool, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGMesh> {
    // Check cache first
    if let Some(cache_ref) = cache.as_deref_mut() {
      if let Some(cached) = cache_ref.get_mesh(self.hash()) {
        return Some((*cached).clone());
      }
    }
    
    // Compute if not cached
    let result = match &self.kind {
      GeoNodeKind::HalfSpace { normal, center} => {
        Some(Self::half_space_to_csg(*normal, *center, is_root))
      }
      GeoNodeKind::Sphere { center, radius } => {
        Some(Self::sphere_to_csg(*center, *radius))
      }
      GeoNodeKind::Extrude { height, direction, shape, plane_to_world_transform } => {
        Self::extrude_to_csg(*height, *direction, shape, plane_to_world_transform, cache.as_deref_mut())
      }
      GeoNodeKind::Transform { transform, shape } => {
        Self::transform_to_csg(transform, shape, cache.as_deref_mut())
      }
      GeoNodeKind::Union3D { shapes } => {
        Self::union_3d_to_csg(shapes, cache.as_deref_mut())
      }
      GeoNodeKind::Intersection3D { shapes } => {
        Self::intersection_3d_to_csg(shapes, cache.as_deref_mut())
      }
      GeoNodeKind::Difference3D { base, sub } => {
        Self::difference_3d_to_csg(base, sub, cache.as_deref_mut())
      }
      _ => None
    }?;
    
    // Store in cache
    if let Some(cache_ref) = cache.as_deref_mut() {
      cache_ref.insert_mesh(*self.hash(), result.clone());
    }
    
    Some(result)
  }

  fn half_space_to_csg(normal: DVec3, center: DVec3, is_root: bool) -> CSGMesh {
    create_half_space_geo(
          &normal,
          &center,
          is_root)
  }

  fn half_plane_to_csg(point1: DVec2, point2: DVec2) -> CSGSketch {
    // Calculate direction vector from point1 to point2
    let dir_vector = point2 - point1;
    let dir = dir_vector.normalize();
    let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
    let center_pos = point1 + dir_vector * 0.5;
  
    let width = 1200.0;
    let height = 1200.0;
  
    let tr = center_pos - dir * width * 0.5 - normal * height;

    CSGSketch::rectangle(scale_to_csg(width), scale_to_csg(height), None)
    .rotate(0.0, 0.0, dir.y.atan2(dir.x).to_degrees())
    .translate(scale_to_csg(tr.x), scale_to_csg(tr.y), 0.0)
  }

  fn circle_to_csg(center: DVec2, radius: f64) -> CSGSketch {
    // Calculate adaptive subdivision based on radius
    // Use square root for more gradual scaling than linear
    //let scale = (radius.sqrt() * 3.0).max(6.0) as i32;
    //let segments = (scale * 4) as usize;
    let segments = 36;
    
    CSGSketch::circle(
      scale_to_csg(radius),
      segments,
      None
    )
    .translate(scale_to_csg(center.x), scale_to_csg(center.y), 0.0)
  }

  fn sphere_to_csg(center: DVec3, radius: f64) -> CSGMesh {
    // Calculate adaptive subdivision based on radius
    // Use square root for more gradual scaling than linear
    //let scale = (radius.sqrt() * 3.0).max(6.0) as i32;
    //let segments = (scale * 4) as usize;
    //let stacks = (scale * 2) as usize;
    let segments = 24;
    let stacks = 12;
    
    CSGMesh::sphere(
      scale_to_csg(radius),
      segments,
      stacks,
      None
    )
      .translate(scale_to_csg(center.x), scale_to_csg(center.y), scale_to_csg(center.z))
  }

  fn polygon_to_csg(vertices: &Vec<DVec2>) -> CSGSketch {
    let mut points: Vec<[f64; 2]> = Vec::new();
  
    for i in 0..vertices.len() {
        points.push([scale_to_csg(vertices[i].x), scale_to_csg(vertices[i].y)]);
    }
  
    CSGSketch::polygon(&points, None)
  }

  fn extrude_to_csg(
      height: f64, 
      direction: DVec3, 
      shape: &Box<GeoNode>, 
      plane_to_world_transform: &Transform,
      mut cache: Option<&mut CsgConversionCache>
  ) -> Option<CSGMesh> {
      // 1. Get 2D sketch (already in plane-local XY coordinates)
      let sketch = shape.to_csg_sketch_cached(cache.as_deref_mut())?;
      
      // 2. Extrude in plane-local space (direction is in plane-local coordinates)
      let scaled_height = scale_to_csg(height);
      let extrusion_vector = dvec3_to_vector3(direction * scaled_height);
      let extruded = sketch.extrude_vector(extrusion_vector);
      
      // 3. Transform the extruded mesh from plane-local to world space
      let transformed = Self::apply_transform_to_csg(&extruded, plane_to_world_transform);

      Some(transformed)
  }

  fn apply_transform_to_csg(mesh: &CSGMesh, transform: &Transform) -> CSGMesh {
    let euler_extrinsic_zyx = transform.rotation.to_euler(glam::EulerRot::ZYX);
    mesh
      .rotate(
        euler_extrinsic_zyx.2.to_degrees(), 
        euler_extrinsic_zyx.1.to_degrees(), 
        euler_extrinsic_zyx.0.to_degrees()
      )
      .translate(
        scale_to_csg(transform.translation.x), 
        scale_to_csg(transform.translation.y), 
        scale_to_csg(transform.translation.z)
      )
  }

  fn transform_to_csg(transform: &Transform, shape: &Box<GeoNode>, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGMesh> {
    let mesh = shape.internal_to_csg_mesh(false, cache.as_deref_mut())?;
    Some(Self::apply_transform_to_csg(&mesh, transform))
  }

  fn union_2d_to_csg(shapes: &Vec<GeoNode>, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGSketch> {
    if shapes.is_empty() {
      return Some(CSGSketch::new());
    }
    
    let mut result = shapes[0].internal_to_csg_sketch(cache.as_deref_mut())?;
    for shape in shapes.iter().skip(1) {
      result = result.union(&shape.internal_to_csg_sketch(cache.as_deref_mut())?);
    }
    Some(result)
  }

  fn intersection_2d_to_csg(shapes: &Vec<GeoNode>, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGSketch> {
    if shapes.is_empty() {
      return Some(CSGSketch::new());
    }
    
    let mut result = shapes[0].internal_to_csg_sketch(cache.as_deref_mut())?;
    for shape in shapes.iter().skip(1) {
      result = result.intersection(&shape.internal_to_csg_sketch(cache.as_deref_mut())?);
    }
    Some(result)
  }

  fn difference_2d_to_csg(base: &Box<GeoNode>, sub: &Box<GeoNode>, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGSketch> {
    let base_csg = base.internal_to_csg_sketch(cache.as_deref_mut())?;
    let sub_csg = sub.internal_to_csg_sketch(cache.as_deref_mut())?;
    Some(base_csg.difference(&sub_csg))
  }

  fn union_3d_to_csg(shapes: &Vec<GeoNode>, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGMesh> {
    if shapes.is_empty() {
      return Some(CSGMesh::new());
    }
    
    let mut result = shapes[0].internal_to_csg_mesh(false, cache.as_deref_mut())?;
    for shape in shapes.iter().skip(1) {
      result = result.union(&shape.internal_to_csg_mesh(false, cache.as_deref_mut())?);
    }
    Some(result)
  }

  fn intersection_3d_to_csg(shapes: &Vec<GeoNode>, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGMesh> {
    if shapes.is_empty() {
      return Some(CSGMesh::new());
    }
    
    let mut result = shapes[0].internal_to_csg_mesh(false, cache.as_deref_mut())?;
    for shape in shapes.iter().skip(1) {
      let shape_mesh = shape.internal_to_csg_mesh(false, cache.as_deref_mut())?;
      result = result.intersection(&shape_mesh);
    }
    Some(result)
  }

  fn difference_3d_to_csg(base: &Box<GeoNode>, sub: &Box<GeoNode>, mut cache: Option<&mut CsgConversionCache>) -> Option<CSGMesh> {
    let base_csg = base.internal_to_csg_mesh(false, cache.as_deref_mut())?;
    let sub_csg = sub.internal_to_csg_mesh(false, cache.as_deref_mut())?;
    Some(base_csg.difference(&sub_csg))
  }
}

pub fn create_half_space_geo(normal: &DVec3, center_pos: &DVec3, is_root: bool) -> CSGMesh {
  let na_normal = dvec3_to_vector3(*normal);
  let rotation = DQuat::from_rotation_arc(DVec3::Z, *normal);

  let width : f64 = if is_root { 100.0 } else { 1200.0 };
  let height : f64 = if is_root { 100.0 } else { 1200.0 };
  let scaled_width = scale_to_csg(width);
  let scaled_height = scale_to_csg(height);

  let start_x = -scaled_width * 0.5;
  let start_y = -scaled_height * 0.5;
  let end_x = scaled_width * 0.5;
  let end_y = scaled_height * 0.5;

  // Front face vertices (at z=0) - counter-clockwise order
  let v1 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, start_y, 0.0)));
  let v2 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, start_y, 0.0)));
  let v3 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, end_y, 0.0)));
  let v4 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, end_y, 0.0)));

  // Create polygons based on the visualization type
  let polygons = 
      vec![
        Polygon::new(
            vec![
                Vertex::new(v1, na_normal),
                Vertex::new(v2, na_normal),
                Vertex::new(v3, na_normal),
                Vertex::new(v4, na_normal),
            ], None
        ),
      ];

  return CSGMesh::from_polygons(&polygons, None)
    .translate(scale_to_csg(center_pos.x), scale_to_csg(center_pos.y), scale_to_csg(center_pos.z));
}

















