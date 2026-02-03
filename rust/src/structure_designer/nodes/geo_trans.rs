use crate::structure_designer::evaluator::network_evaluator::{
  NetworkEvaluationContext, NetworkEvaluator
};
use crate::structure_designer::evaluator::network_result::{
  runtime_type_error_in_input, GeometrySummary, NetworkResult
};
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::util::serialization_utils::ivec3_serializer;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use glam::DQuat;
use std::f64::consts::PI;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::display::gadget::Gadget;
use crate::structure_designer::utils::xyz_gadget_utils;
use crate::renderer::mesh::Mesh;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone)]
pub struct GeoTransEvalCache {
  pub input_frame_transform: Transform,
  pub unit_cell: UnitCellStruct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoTransData {
  #[serde(with = "ivec3_serializer")]
  pub translation: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub rotation: IVec3, // intrinsic euler angles where 1 increment means 90 degrees.
  pub transform_only_frame: bool, // If true, only the reference frame is transformed, the geometry remains in place.
}

impl NodeData for GeoTransData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let geo_trans_cache = eval_cache.downcast_ref::<GeoTransEvalCache>()?;
        
        let gadget = GeoTransGadget::new(
            self.translation,
            self.rotation,
            geo_trans_cache.input_frame_transform.clone(),
            &geo_trans_cache.unit_cell,
        );
        Some(Box::new(gadget))
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }

    fn eval<'a>(
      &self,
      network_evaluator: &NetworkEvaluator,
      network_stack: &Vec<NetworkStackElement<'a>>,
      node_id: u64,
      registry: &NodeTypeRegistry,
      _decorate: bool,
      context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
      let shape_val = network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
    
      if let NetworkResult::Error(_) = shape_val {
        shape_val
      } else if let NetworkResult::Geometry(shape) = shape_val {
    
        let translation = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 1, 
          self.translation, 
          NetworkResult::extract_ivec3
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };
      
        let rotation = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 2, 
          self.rotation, 
          NetworkResult::extract_ivec3
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };
    
        let real_translation = shape.unit_cell.ivec3_lattice_to_real(&translation);

        let cubic = shape.unit_cell.is_approximately_cubic();

        if (!cubic) && rotation != IVec3::ZERO {
          return NetworkResult::Error("Nonzero rotation is only allowed for cubic unit cells for now.".to_string())
        }

        let rotation_euler = rotation.as_dvec3() * PI * 0.5;
        let rotation_quat = DQuat::from_euler(
          glam::EulerRot::XYZ,
          rotation_euler.x, 
          rotation_euler.y, 
          rotation_euler.z);
    
        let frame_transform = shape.frame_transform.apply_lrot_gtrans_new(&Transform::new(real_translation, rotation_quat));
    
        // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
        // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
        if network_stack.len() == 1 {
          let eval_cache = GeoTransEvalCache {
            input_frame_transform: shape.frame_transform.clone(),
            unit_cell: shape.unit_cell.clone(),
          };
          context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }
    
        // We need to be a bit tricky here.
        // The input geometry (shape) is already transformed by the input transform.
        // So theoretically we need to do the inverse of the input transform (shape transform) so the geometry is first transformed back
        // to its local position.
        // And then we apply the whole frame transform.
        let rot = frame_transform.rotation * shape.frame_transform.rotation.inverse();
        let tr = Transform::new(
          rot.mul_vec3(-shape.frame_transform.translation) + frame_transform.translation, 
          rot
        );

        NetworkResult::Geometry(GeometrySummary {
          unit_cell: shape.unit_cell,
          frame_transform,
          geo_tree_root: GeoNode::transform(tr, Box::new(shape.geo_tree_root)),
        })
      } else {
        runtime_type_error_in_input(0)
      }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("translation".to_string(), TextValue::IVec3(self.translation)),
            ("rotation".to_string(), TextValue::IVec3(self.rotation)),
            ("transform_only_frame".to_string(), TextValue::Bool(self.transform_only_frame)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("translation") {
            self.translation = v.as_ivec3().ok_or_else(|| "translation must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("rotation") {
            self.rotation = v.as_ivec3().ok_or_else(|| "rotation must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("transform_only_frame") {
            self.transform_only_frame = v.as_bool().ok_or_else(|| "transform_only_frame must be a boolean".to_string())?;
        }
        Ok(())
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let show_rotation = !connected_input_pins.contains("rotation");
        let show_translation = !connected_input_pins.contains("translation");
        
        match (show_rotation, show_translation) {
            (true, true) => Some(format!("r: ({},{},{}) t: ({},{},{})", 
                self.rotation.x, self.rotation.y, self.rotation.z,
                self.translation.x, self.translation.y, self.translation.z)),
            (true, false) => Some(format!("r: ({},{},{})", 
                self.rotation.x, self.rotation.y, self.rotation.z)),
            (false, true) => Some(format!("t: ({},{},{})",
                self.translation.x, self.translation.y, self.translation.z)),
            (false, false) => None,
        }
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("shape".to_string(), (true, None)); // required
        m
    }
}


#[derive(Clone)]
pub struct GeoTransGadget {
    pub translation: IVec3,
    pub rotation: IVec3, // intrinsic euler angles where 1 increment means 90 degrees
    pub input_frame_transform: Transform,
    pub frame_transform: Transform,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for GeoTransGadget {
  fn tessellate(&self, output: &mut TessellationOutput) {
    let output_mesh: &mut Mesh = &mut output.mesh;
    xyz_gadget_utils::tessellate_xyz_gadget(
      output_mesh,
      &self.unit_cell,
      self.frame_transform.rotation,
      &self.frame_transform.translation,
      false // Don't include rotation handles for now
    );
  }

  fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
      Box::new(self.clone())
  }
}

impl Gadget for GeoTransGadget {
  fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
      xyz_gadget_utils::xyz_gadget_hit_test(
          &self.unit_cell,
          self.frame_transform.rotation,
          &self.frame_transform.translation,
          &ray_origin,
          &ray_direction,
          false // Don't include rotation handles for now
      )
  }

  fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    self.dragged_handle_index = Some(handle_index);
    self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
        &self.unit_cell,
        self.frame_transform.rotation,
        &self.frame_transform.translation,
        handle_index,
        &ray_origin,
        &ray_direction
    );
  }

  fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
        &self.unit_cell,
        self.frame_transform.rotation,
        &self.frame_transform.translation,
        handle_index,
        &ray_origin,
        &ray_direction
    );
    let offset_delta = current_offset - self.start_drag_offset;
    if self.apply_drag_offset(handle_index, offset_delta) {
      self.start_drag(handle_index, ray_origin, ray_direction);
    }
  }

  fn end_drag(&mut self) {
    self.dragged_handle_index = None;
  }
}

impl NodeNetworkGadget for GeoTransGadget {
  fn sync_data(&self, data: &mut dyn NodeData) {
      if let Some(geo_trans_data) = data.as_any_mut().downcast_mut::<GeoTransData>() {
        let real_delta_translation = self.frame_transform.translation - self.input_frame_transform.translation;
        geo_trans_data.translation = self.unit_cell.real_to_ivec3_lattice(&real_delta_translation);
        geo_trans_data.rotation = self.rotation;
      }
  }

  fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
      Box::new(self.clone())
  }
}

impl GeoTransGadget {
  pub fn new(translation: IVec3, rotation: IVec3, input_frame_transform: Transform, unit_cell: &UnitCellStruct) -> Self {
      let mut ret = Self {
          translation,
          rotation,
          input_frame_transform: Transform::new(input_frame_transform.translation, input_frame_transform.rotation),
          frame_transform: Transform::new(DVec3::ZERO, DQuat::IDENTITY),
          dragged_handle_index: None,
          start_drag_offset: 0.0,
          unit_cell: unit_cell.clone(),
      };
      ret.refresh_frame_transform();
      ret
  }

  // Returns whether the application of the drag offset was successful and the drag start should be reset
  fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
    let axis_basis_vector = self.unit_cell.get_basis_vector(axis_index);
    let rounded_delta = (offset_delta / axis_basis_vector.length()).round();

    // Early return if no movement
    if rounded_delta == 0.0 {
      return false;
    }
    
    // Get the local axis direction based on the current rotation
    let local_axis_dir = match xyz_gadget_utils::get_local_axis_direction(&self.unit_cell, self.frame_transform.rotation, axis_index) {
      Some(dir) => dir,
      None => return false, // Invalid axis index
    };

    // Calculate the movement vector
    let movement_distance = rounded_delta * axis_basis_vector.length();
    let movement_vector = local_axis_dir * movement_distance;
    
    // Apply the movement to the frame transform
    self.frame_transform.translation += movement_vector;

    true
  }

  fn refresh_frame_transform(&mut self) {
    let real_translation = self.unit_cell.ivec3_lattice_to_real(&self.translation);
    let rotation_euler = self.rotation.as_dvec3() * PI * 0.5;
    let rotation_quat = DQuat::from_euler(
      glam::EulerRot::XYZ,
      rotation_euler.x, 
      rotation_euler.y, 
      rotation_euler.z);

    self.frame_transform = self.input_frame_transform.apply_lrot_gtrans_new(&Transform::new(real_translation, rotation_quat));
  }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "geo_trans".to_string(),
      description: "".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              id: None,
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
            id: None,
            name: "translation".to_string(),
            data_type: DataType::IVec3,
          },
          Parameter {
            id: None,
            name: "rotation".to_string(),
            data_type: DataType::IVec3,
          },
      ],
      output_type: DataType::Geometry,
      public: false,
      node_data_creator: || Box::new(GeoTransData {
        translation: IVec3::new(0, 0, 0),
        rotation: IVec3::new(0, 0, 0),
        transform_only_frame: false,
      }),
      node_data_saver: generic_node_data_saver::<GeoTransData>,
      node_data_loader: generic_node_data_loader::<GeoTransData>,
    }
}
