use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::util::serialization_utils::ivec3_serializer;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::DQuat;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_result::runtime_type_error_in_input;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;

fn default_extrude_direction() -> IVec3 {
  IVec3::new(0, 0, 1)
}

fn default_infinite() -> bool {
  false
}

fn default_subdivision() -> i32 {
  1
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtrudeData {
  pub height: i32,
  #[serde(with = "ivec3_serializer")]
  #[serde(default = "default_extrude_direction")]
  pub extrude_direction: IVec3,
  #[serde(default = "default_infinite")]
  pub infinite: bool,
  #[serde(default = "default_subdivision")]
  pub subdivision: i32,
}

#[derive(Debug, Clone)]
pub struct ExtrudeEvalCache {
  pub drawing_plane_miller_direction: IVec3,
}

impl NodeData for ExtrudeData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
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
      //let _timer = Timer::new("eval_extrude");
      let shape_val = network_evaluator.evaluate_arg_required(
        network_stack,
        node_id,
        registry,
        context,
        0,
      );
    
      // NOTE: Input pin 1 (extrude pin) is deprecated but kept for backward compatibility with existing networks.
      // We ignore it and use the unit cell from the shape instead.

      if let NetworkResult::Error(_) = shape_val {
        return shape_val;
      }

      let height = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 2, 
        self.height, 
        NetworkResult::extract_int
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let extrude_direction = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 3,
        self.extrude_direction,
        NetworkResult::extract_ivec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let infinite = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 4,
        self.infinite,
        NetworkResult::extract_bool
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let subdivision = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 5,
        self.subdivision,
        NetworkResult::extract_int
      ) {
        Ok(value) => value.max(1),
        Err(error) => return error,
      };

      if !infinite && height <= 0 {
        return NetworkResult::Error("Extrusion height must be positive".to_string());
      }

      if let NetworkResult::Geometry2D(shape) = shape_val {
        // Extract unit cell from the drawing plane
        let unit_cell = shape.drawing_plane.unit_cell.clone();

        if network_stack.len() == 1 {
          let eval_cache = ExtrudeEvalCache {
            drawing_plane_miller_direction: shape.drawing_plane.miller_index,
          };
          context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }
        
        // Validate extrusion direction for this plane (in world space)
        let (world_direction, d_spacing) = match shape.drawing_plane.validate_extrude_direction(&extrude_direction) {
            Ok(result) => result,
            Err(error_msg) => return NetworkResult::Error(error_msg),
        };

        let height_real = if infinite {
          0.0
        } else {
          (height as f64 / subdivision as f64) * d_spacing
        };
        
        // Compute plane_to_world transform from DrawingPlane
        let plane_to_world_transform = shape.drawing_plane.to_world_transform();
        
        // Transform world extrusion direction to plane-local coordinates
        let world_to_plane_rotation = plane_to_world_transform.rotation.inverse();
        let local_direction = world_to_plane_rotation * world_direction;
        
        let frame_translation_2d = shape.frame_transform.translation;
    
        let frame_transform = Transform::new(
          DVec3::new(frame_translation_2d.x, frame_translation_2d.y, 0.0),
          DQuat::from_rotation_z(shape.frame_transform.rotation),
        );
    
        let s = shape.geo_tree_root;
        return NetworkResult::Geometry(GeometrySummary { 
          unit_cell,
          frame_transform,
          geo_tree_root: GeoNode::extrude(height_real, local_direction, Box::new(s), plane_to_world_transform, infinite)
        });
      } else {
        return runtime_type_error_in_input(0);
      }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(format!("h: {} dir: [{},{},{}]",
            self.height,
            self.extrude_direction.x,
            self.extrude_direction.y,
            self.extrude_direction.z
        ))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("height".to_string(), TextValue::Int(self.height)),
            // Property names must match parameter names for describe command
            ("dir".to_string(), TextValue::IVec3(self.extrude_direction)),
            ("inf".to_string(), TextValue::Bool(self.infinite)),
            ("subdivision".to_string(), TextValue::Int(self.subdivision)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("height") {
            self.height = v.as_int().ok_or_else(|| "height must be an integer".to_string())?;
        }
        // Accept both old names (backward compat) and new names (matching parameters)
        if let Some(v) = props.get("dir").or_else(|| props.get("extrude_direction")) {
            self.extrude_direction = v.as_ivec3().ok_or_else(|| "dir must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("inf").or_else(|| props.get("infinite")) {
            self.infinite = v.as_bool().ok_or_else(|| "inf must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("subdivision") {
            self.subdivision = v.as_int().ok_or_else(|| "subdivision must be an integer".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("shape".to_string(), (true, None)); // required
        m.insert("unit_cell".to_string(), (false, Some("cubic diamond".to_string())));
        m
    }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "extrude".to_string(),
      description: "Extrudes a 2D geometry to a 3D geometry.".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              id: None,
              name: "shape".to_string(),
              data_type: DataType::Geometry2D,
          },
          Parameter {
            id: None,
            name: "unit_cell".to_string(),
            data_type: DataType::UnitCell,
          },
          Parameter {
            id: None,
            name: "height".to_string(),
            data_type: DataType::Int,
          },
          Parameter {
            id: None,
            name: "dir".to_string(),
            data_type: DataType::IVec3,
          },
          Parameter {
            id: None,
            name: "inf".to_string(),
            data_type: DataType::Bool,
          },
          Parameter {
            id: None,
            name: "subdivision".to_string(),
            data_type: DataType::Int,
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(ExtrudeData {
        height: 1,
        extrude_direction: IVec3::new(0, 0, 1),
        infinite: false,
        subdivision: 1,
      }),
      node_data_saver: generic_node_data_saver::<ExtrudeData>,
      node_data_loader: generic_node_data_loader::<ExtrudeData>,
  }
}













