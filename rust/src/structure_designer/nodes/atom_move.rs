use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::collections::HashSet;
use crate::util::serialization_utils::dvec3_serializer;
use crate::structure_designer::text_format::TextValue;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::display::gadget::Gadget;
use glam::f64::DQuat;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::utils::xyz_gadget_utils;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;

/// Evaluation cache for atom_move node.
/// Currently empty but reserved for future gadget needs.
#[derive(Debug, Clone)]
pub struct AtomMoveEvalCache {
    // Empty - reserved for future use
}

/// Data structure for atom_move node.
/// Translates an atomic structure by a vector in world space (Cartesian coordinates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomMoveData {
    #[serde(with = "dvec3_serializer")]
    pub translation: DVec3,  // Translation vector in angstroms
}

impl NodeData for AtomMoveData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let _atom_move_cache = eval_cache.downcast_ref::<AtomMoveEvalCache>()?;

        Some(Box::new(AtomMoveGadget::new(self.translation)))
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
        context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext
    ) -> NetworkResult {
        // 1. Get input atomic structure
        let input_val = network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = input_val {
            return input_val;
        }

        if let NetworkResult::Atomic(atomic_structure) = input_val {
            // 2. Get translation (from pin or property)
            let translation = match network_evaluator.evaluate_or_default(
                network_stack, node_id, registry, context, 1,
                self.translation,
                NetworkResult::extract_vec3
            ) {
                Ok(value) => value,
                Err(error) => return error,
            };

            // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
            if network_stack.len() == 1 {
                let eval_cache = AtomMoveEvalCache {};
                context.selected_node_eval_cache = Some(Box::new(eval_cache));
            }

            // 3. Apply translation directly to the atomic structure
            let mut result_atomic_structure = atomic_structure.clone();
            result_atomic_structure.transform(&DQuat::IDENTITY, &translation);

            return NetworkResult::Atomic(result_atomic_structure);
        }

        NetworkResult::None
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("translation".to_string(), TextValue::Vec3(self.translation)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("translation") {
            self.translation = v.as_vec3().ok_or_else(|| "translation must be a Vec3".to_string())?;
        }
        Ok(())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        if connected_input_pins.contains("translation") {
            return None;
        }
        Some(format!("({:.2}, {:.2}, {:.2})",
            self.translation.x, self.translation.y, self.translation.z))
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m
    }
}

/// Gadget for atom_move node that displays an XYZ axis gizmo.
/// The gadget is always world-aligned (not rotated with the structure).
/// Position is at the translation value (starting from origin).
#[derive(Clone)]
pub struct AtomMoveGadget {
    pub translation: DVec3,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
    pub start_drag_translation: DVec3,
}

impl Tessellatable for AtomMoveGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;
        // Use world-aligned gadget (identity rotation)
        // Gadget is positioned at the translation value
        xyz_gadget_utils::tessellate_xyz_gadget(
            output_mesh,
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.translation,
            false,  // No rotation handles (translation only)
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for AtomMoveGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        xyz_gadget_utils::xyz_gadget_hit_test(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.translation,
            &ray_origin,
            &ray_direction,
            false  // No rotation handles
        )
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        self.dragged_handle_index = Some(handle_index);
        self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.translation,
            handle_index,
            &ray_origin,
            &ray_direction
        );
        self.start_drag_translation = self.translation;
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.translation,
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

impl NodeNetworkGadget for AtomMoveGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(atom_move_data) = data.as_any_mut().downcast_mut::<AtomMoveData>() {
            atom_move_data.translation = self.translation;
        }
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl AtomMoveGadget {
    pub fn new(translation: DVec3) -> Self {
        Self {
            translation,
            dragged_handle_index: None,
            start_drag_offset: 0.0,
            start_drag_translation: translation,
        }
    }

    /// Applies drag offset to the translation.
    /// Returns true if the operation was successful and the drag start should be reset.
    fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
        // Only handle translation axes (0, 1, 2 for X, Y, Z)
        if axis_index < 0 || axis_index > 2 {
            return false;
        }

        // Get the world axis direction (always world-aligned for atom_move)
        let axis_direction = match xyz_gadget_utils::get_local_axis_direction(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            axis_index
        ) {
            Some(dir) => dir,
            None => return false,
        };

        // Apply the movement to translation
        let movement_vector = axis_direction * offset_delta;
        self.translation = self.start_drag_translation + movement_vector;

        true
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "atom_move".to_string(),
        description: "Translates an atomic structure by a vector in world space (Cartesian coordinates).
The translation is specified in angstroms along the X, Y, and Z axes.
This node operates in continuous space, unlike lattice_move which operates in discrete lattice space.".to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::Atomic,
            },
            Parameter {
                id: None,
                name: "translation".to_string(),
                data_type: DataType::Vec3,
            },
        ],
        output_type: DataType::Atomic,
        public: true,
        node_data_creator: || Box::new(AtomMoveData {
            translation: DVec3::ZERO,
        }),
        node_data_saver: generic_node_data_saver::<AtomMoveData>,
        node_data_loader: generic_node_data_loader::<AtomMoveData>,
    }
}
