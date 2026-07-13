use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::structure::Structure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::Alignment;
use crate::structure_designer::evaluator::network_result::BlueprintData;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::serialization_utils::ivec3_serializer;
use glam::f64::DVec3;
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuboidData {
    #[serde(with = "ivec3_serializer")]
    pub min_corner: IVec3,
    #[serde(with = "ivec3_serializer")]
    pub extent: IVec3,
    #[serde(default = "default_subdivision")]
    pub subdivision: i32,
}

fn default_subdivision() -> i32 {
    1
}

impl NodeData for CuboidData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let min_corner = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            self.min_corner,
            NetworkResult::extract_ivec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let extent = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.extent,
            NetworkResult::extract_ivec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let structure = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            Structure::diamond(),
            NetworkResult::extract_structure,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let subdivision = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            self.subdivision,
            NetworkResult::extract_int,
        ) {
            Ok(value) => value.max(1), // Ensure minimum value of 1
            Err(error) => return EvalOutput::single(error),
        };

        // Both the corner and the extent are expressed in units of 1/subdivision of a
        // unit cell, so a fractional cuboid can be authored while the pins stay integer.
        let inv_subdivision = 1.0 / subdivision as f64;
        let geo_tree_root = create_parallelepiped_from_lattice(
            &structure.lattice_vecs,
            min_corner.as_dvec3() * inv_subdivision,
            extent.as_dvec3() * inv_subdivision,
        );

        EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
            structure,
            geo_tree_root,
            alignment: Alignment::Aligned,
            alignment_reason: None,
        }))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let show_min_corner = !connected_input_pins.contains("min_corner");
        let show_extent = !connected_input_pins.contains("extent");
        let show_subdivision =
            !connected_input_pins.contains("subdivision") && self.subdivision != 1;

        let mut parts = Vec::new();
        if show_min_corner {
            parts.push(format!(
                "mc: ({},{},{})",
                self.min_corner.x, self.min_corner.y, self.min_corner.z
            ));
        }
        if show_extent {
            parts.push(format!(
                "e: ({},{},{})",
                self.extent.x, self.extent.y, self.extent.z
            ));
        }
        if show_subdivision {
            parts.push(format!("sub: {}", self.subdivision));
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("min_corner".to_string(), TextValue::IVec3(self.min_corner)),
            ("extent".to_string(), TextValue::IVec3(self.extent)),
            ("subdivision".to_string(), TextValue::Int(self.subdivision)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("min_corner") {
            self.min_corner = v
                .as_ivec3()
                .ok_or_else(|| "min_corner must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("extent") {
            self.extent = v
                .as_ivec3()
                .ok_or_else(|| "extent must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("subdivision") {
            self.subdivision = v
                .as_int()
                .ok_or_else(|| "subdivision must be an integer".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert(
            "structure".to_string(),
            (false, Some("diamond".to_string())),
        );
        m
    }
}

/// Creates a parallelepiped in real space from lattice coordinates and unit cell basis vectors.
/// The parallelepiped is represented as an intersection of 6 half-spaces (3 pairs of opposing faces).
fn create_parallelepiped_from_lattice(
    unit_cell: &UnitCellStruct,
    min_corner_lattice: DVec3,
    extent_lattice: DVec3,
) -> GeoNode {
    // Get the unit cell basis vectors
    let basis_a = unit_cell.a;
    let basis_b = unit_cell.b;
    let basis_c = unit_cell.c;

    // Convert lattice coordinates to real space coordinates
    let min_corner_real = min_corner_lattice.x * basis_a
        + min_corner_lattice.y * basis_b
        + min_corner_lattice.z * basis_c;

    let max_corner_lattice = min_corner_lattice + extent_lattice;
    let _max_corner_real = max_corner_lattice.x * basis_a
        + max_corner_lattice.y * basis_b
        + max_corner_lattice.z * basis_c;

    // Create 6 half-spaces defining the parallelepiped faces
    let mut half_spaces = Vec::new();

    // Calculate the center of the parallelepiped for reference
    let _parallelepiped_center = min_corner_real
        + (extent_lattice.x * basis_a + extent_lattice.y * basis_b + extent_lattice.z * basis_c)
            / 2.0;

    // For a parallelepiped, the normal to each face is the cross product of the other two basis vectors
    // Face pair perpendicular to the plane containing basis_b and basis_c (A-direction faces)
    let normal_a = (basis_b.cross(basis_c)).normalize();

    // Calculate face centers instead of corner points
    let min_face_center_a =
        min_corner_real + (extent_lattice.y * basis_b + extent_lattice.z * basis_c) / 2.0;
    let max_face_center_a = min_corner_real
        + extent_lattice.x * basis_a
        + (extent_lattice.y * basis_b + extent_lattice.z * basis_c) / 2.0;

    half_spaces.push(GeoNode::half_space(-normal_a, min_face_center_a));
    half_spaces.push(GeoNode::half_space(normal_a, max_face_center_a));

    // Face pair perpendicular to the plane containing basis_c and basis_a (B-direction faces)
    let normal_b = (basis_c.cross(basis_a)).normalize();

    let min_face_center_b =
        min_corner_real + (extent_lattice.x * basis_a + extent_lattice.z * basis_c) / 2.0;
    let max_face_center_b = min_corner_real
        + extent_lattice.y * basis_b
        + (extent_lattice.x * basis_a + extent_lattice.z * basis_c) / 2.0;

    half_spaces.push(GeoNode::half_space(-normal_b, min_face_center_b));
    half_spaces.push(GeoNode::half_space(normal_b, max_face_center_b));

    // Face pair perpendicular to the plane containing basis_a and basis_b (C-direction faces)
    let normal_c = (basis_a.cross(basis_b)).normalize();

    let min_face_center_c =
        min_corner_real + (extent_lattice.x * basis_a + extent_lattice.y * basis_b) / 2.0;
    let max_face_center_c = min_corner_real
        + extent_lattice.z * basis_c
        + (extent_lattice.x * basis_a + extent_lattice.y * basis_b) / 2.0;

    half_spaces.push(GeoNode::half_space(-normal_c, min_face_center_c));
    half_spaces.push(GeoNode::half_space(normal_c, max_face_center_c));

    // Return the intersection of all half-spaces
    GeoNode::intersection_3d(half_spaces)
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "cuboid".to_string(),
      description: "Outputs a cuboid with integer minimum corner coordinates and integer extent coordinates. If the unit cell is not cubic, the shape will not necessarily be a cuboid: in the most general case it will be a parallelepiped. The subdivision parameter (default 1) refines the lattice grid: both the minimum corner and the extent are measured in units of 1/subdivision of a unit cell, allowing sub-cell resolution while keeping the pins integer-typed.".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
        Parameter {
            id: None,
            name: "min_corner".to_string(),
            data_type: DataType::IVec3,
        },
        Parameter {
          id: None,
          name: "extent".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          id: None,
          name: "structure".to_string(),
          data_type: DataType::Structure,
        },
        Parameter {
          id: None,
          name: "subdivision".to_string(),
          data_type: DataType::Int,
        },
      ],
      output_pins: OutputPinDefinition::single(DataType::Blueprint),
      zone_input_pins: vec![],
      zone_output_pins: vec![],
      public: true,
      node_data_creator: || Box::new(CuboidData {
        min_corner: IVec3::new(0, 0, 0),
        extent: IVec3::new(1, 1, 1),
        subdivision: 1,
      }),
      node_data_saver: generic_node_data_saver::<CuboidData>,
      node_data_loader: generic_node_data_loader::<CuboidData>,
    }
}
