use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitCellData {
  pub cell_length_a: f64,
  pub cell_length_b: f64,
  pub cell_length_c: f64,
  pub cell_angle_alpha: f64, // in degrees
  pub cell_angle_beta: f64, // in degrees
  pub cell_angle_gamma: f64, // in degrees
}

impl UnitCellData {
    /// Converts UnitCellData (crystallographic format) to UnitCellStruct (basis vectors)
    /// 
    /// This function converts from the standard crystallographic unit cell parameters
    /// (lengths a, b, c and angles α, β, γ) to three basis vectors in 3D space.
    /// 
    /// The conversion follows the standard crystallographic convention:
    /// - Vector a is aligned with the x-axis
    /// - Vector b lies in the xy-plane
    /// - Vector c is positioned to satisfy the given angles
    pub fn to_unit_cell_struct(&self) -> UnitCellStruct {
        
        // Convert angles from degrees to radians
        let alpha = self.cell_angle_alpha.to_radians();
        let beta = self.cell_angle_beta.to_radians();
        let gamma = self.cell_angle_gamma.to_radians();
        
        // Vector a is aligned with the x-axis
        let a = DVec3::new(self.cell_length_a, 0.0, 0.0);
        
        // Vector b lies in the xy-plane
        let b = DVec3::new(
            self.cell_length_b * gamma.cos(),
            self.cell_length_b * gamma.sin(),
            0.0
        );
        
        // Vector c is positioned to satisfy the given angles
        // Calculate the z-component using the volume formula
        let cos_alpha = alpha.cos();
        let cos_beta = beta.cos();
        let cos_gamma = gamma.cos();
        let sin_gamma = gamma.sin();
        
        let c_x = self.cell_length_c * cos_beta;
        let c_y = self.cell_length_c * (cos_alpha - cos_beta * cos_gamma) / sin_gamma;
        
        // Calculate c_z using the constraint that |c| = cell_length_c
        let c_z_squared = self.cell_length_c * self.cell_length_c - c_x * c_x - c_y * c_y;
        let c_z = if c_z_squared > 0.0 { c_z_squared.sqrt() } else { 0.0 };
        
        let c = DVec3::new(c_x, c_y, c_z);
        
        UnitCellStruct { 
            a, 
            b, 
            c,
            cell_length_a: self.cell_length_a,
            cell_length_b: self.cell_length_b,
            cell_length_c: self.cell_length_c,
            cell_angle_alpha: self.cell_angle_alpha,
            cell_angle_beta: self.cell_angle_beta,
            cell_angle_gamma: self.cell_angle_gamma,
        }
    }
}

impl NodeData for UnitCellData {
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
      context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
      // Convert the unit cell data to UnitCellStruct and return it
      let default_unit_cell_struct = self.to_unit_cell_struct();

      let a = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0, 
        default_unit_cell_struct.a, 
        NetworkResult::extract_vec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let b = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        default_unit_cell_struct.b, 
        NetworkResult::extract_vec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let c = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 2, 
        default_unit_cell_struct.c, 
        NetworkResult::extract_vec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      // Check if any basis vectors were overridden by comparing to defaults
      // Use a small epsilon for floating-point comparison
      const EPSILON: f64 = 1e-10;
      let a_is_default = (a - default_unit_cell_struct.a).length() < EPSILON;
      let b_is_default = (b - default_unit_cell_struct.b).length() < EPSILON;
      let c_is_default = (c - default_unit_cell_struct.c).length() < EPSILON;

      // If none were overridden, return the default to preserve exact crystallographic parameters
      if a_is_default && b_is_default && c_is_default {
        NetworkResult::UnitCell(default_unit_cell_struct)
      } else {
        // At least one was overridden, calculate crystallographic parameters from basis vectors
        NetworkResult::UnitCell(UnitCellStruct::new(a, b, c))
      }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("cell_length_a".to_string(), TextValue::Float(self.cell_length_a)),
            ("cell_length_b".to_string(), TextValue::Float(self.cell_length_b)),
            ("cell_length_c".to_string(), TextValue::Float(self.cell_length_c)),
            ("cell_angle_alpha".to_string(), TextValue::Float(self.cell_angle_alpha)),
            ("cell_angle_beta".to_string(), TextValue::Float(self.cell_angle_beta)),
            ("cell_angle_gamma".to_string(), TextValue::Float(self.cell_angle_gamma)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("cell_length_a") {
            self.cell_length_a = v.as_float().ok_or_else(|| "cell_length_a must be a float".to_string())?;
        }
        if let Some(v) = props.get("cell_length_b") {
            self.cell_length_b = v.as_float().ok_or_else(|| "cell_length_b must be a float".to_string())?;
        }
        if let Some(v) = props.get("cell_length_c") {
            self.cell_length_c = v.as_float().ok_or_else(|| "cell_length_c must be a float".to_string())?;
        }
        if let Some(v) = props.get("cell_angle_alpha") {
            self.cell_angle_alpha = v.as_float().ok_or_else(|| "cell_angle_alpha must be a float".to_string())?;
        }
        if let Some(v) = props.get("cell_angle_beta") {
            self.cell_angle_beta = v.as_float().ok_or_else(|| "cell_angle_beta must be a float".to_string())?;
        }
        if let Some(v) = props.get("cell_angle_gamma") {
            self.cell_angle_gamma = v.as_float().ok_or_else(|| "cell_angle_gamma must be a float".to_string())?;
        }
        Ok(())
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let a_connected = connected_input_pins.contains("a");
        let b_connected = connected_input_pins.contains("b");
        let c_connected = connected_input_pins.contains("c");
        
        if a_connected && b_connected && c_connected {
            None
        } else {
            Some(format!("l: ({:.2},{:.2},{:.2}) a: ({:.1},{:.1},{:.1})", 
                self.cell_length_a, self.cell_length_b, self.cell_length_c,
                self.cell_angle_alpha, self.cell_angle_beta, self.cell_angle_gamma))
        }
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "unit_cell".to_string(),
      description: "Produces a `UnitCell` value representing the three lattice basis vectors defined by the lattice parameters `(a, b, c, α, β, γ)`.".to_string(),
      summary: None,
      category: NodeTypeCategory::OtherBuiltin,
      parameters: vec![
        Parameter {
            id: None,
            name: "a".to_string(),
            data_type: DataType::Vec3,
        },
        Parameter {
          id: None,
          name: "b".to_string(),
          data_type: DataType::Vec3,
        },
        Parameter {
          id: None,
          name: "c".to_string(),
          data_type: DataType::Vec3,
        },
      ],
      output_type: DataType::UnitCell,
      public: true,
      node_data_creator: || Box::new(UnitCellData {
        cell_length_a: DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
        cell_length_b: DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
        cell_length_c: DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
        cell_angle_alpha: 90.0,
        cell_angle_beta: 90.0,
        cell_angle_gamma: 90.0,
      }),
      node_data_saver: generic_node_data_saver::<UnitCellData>,
      node_data_loader: generic_node_data_loader::<UnitCellData>,
  }
}









