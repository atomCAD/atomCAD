use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::supercell::apply_supercell;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stored state for the `supercell` node: a 3×3 integer matrix whose rows are
/// the new basis vectors expressed as integer combinations of the old basis.
/// Row 0 = `a`, row 1 = `b`, row 2 = `c`. Default is identity (pass-through).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupercellData {
    pub matrix: [[i32; 3]; 3],
}

impl Default for SupercellData {
    fn default() -> Self {
        Self {
            matrix: [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
        }
    }
}

fn det_i64(m: &[[i32; 3]; 3]) -> i64 {
    let m00 = m[0][0] as i64;
    let m01 = m[0][1] as i64;
    let m02 = m[0][2] as i64;
    let m10 = m[1][0] as i64;
    let m11 = m[1][1] as i64;
    let m12 = m[1][2] as i64;
    let m20 = m[2][0] as i64;
    let m21 = m[2][1] as i64;
    let m22 = m[2][2] as i64;
    m00 * (m11 * m22 - m12 * m21) - m01 * (m10 * m22 - m12 * m20) + m02 * (m10 * m21 - m11 * m20)
}

impl NodeData for SupercellData {
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
        // Pin 0: required Structure input.
        let structure = match network_evaluator.evaluate_required(
            network_stack,
            node_id,
            registry,
            context,
            0,
            NetworkResult::extract_structure,
        ) {
            Ok(s) => s,
            Err(err) => return EvalOutput::single(err),
        };

        // Pin 1: optional diagonal IVec3 override. If connected, build
        // diag(v.x, v.y, v.z) and use it in place of the stored matrix.
        let diagonal_arg =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let effective_matrix: [[i32; 3]; 3] = match diagonal_arg {
            NetworkResult::None => self.matrix,
            NetworkResult::Error(_) => return EvalOutput::single(diagonal_arg),
            NetworkResult::IVec3(v) => [[v.x, 0, 0], [0, v.y, 0], [0, 0, v.z]],
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "supercell: diagonal input must be IVec3, got {}",
                    other.to_display_string()
                )));
            }
        };

        match apply_supercell(&structure, &effective_matrix) {
            Ok(new_structure) => EvalOutput::single(NetworkResult::Structure(new_structure)),
            Err(e) => EvalOutput::single(NetworkResult::Error(format!("supercell: {}", e))),
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        // When the diagonal pin is connected the stored matrix is not the
        // effective one; show a neutral indicator instead of a misleading det.
        if connected_input_pins.contains("diagonal") {
            return Some("det = ?".to_string());
        }
        let det = det_i64(&self.matrix);
        match det.cmp(&0) {
            std::cmp::Ordering::Equal => Some("det = 0 (singular)".to_string()),
            std::cmp::Ordering::Less => Some(format!("det = {} (left-handed)", det)),
            std::cmp::Ordering::Greater => Some(format!("det = {}", det)),
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "a".to_string(),
                TextValue::IVec3(IVec3::new(
                    self.matrix[0][0],
                    self.matrix[0][1],
                    self.matrix[0][2],
                )),
            ),
            (
                "b".to_string(),
                TextValue::IVec3(IVec3::new(
                    self.matrix[1][0],
                    self.matrix[1][1],
                    self.matrix[1][2],
                )),
            ),
            (
                "c".to_string(),
                TextValue::IVec3(IVec3::new(
                    self.matrix[2][0],
                    self.matrix[2][1],
                    self.matrix[2][2],
                )),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        for (row_idx, key) in ["a", "b", "c"].iter().enumerate() {
            if let Some(v) = props.get(*key) {
                let iv = v
                    .as_ivec3()
                    .ok_or_else(|| format!("{} must be an IVec3", key))?;
                self.matrix[row_idx] = [iv.x, iv.y, iv.z];
            }
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("structure".to_string(), (true, None));
        m.insert(
            "diagonal".to_string(),
            (
                false,
                Some(
                    "Optional IVec3 override. When connected, the stored matrix is replaced by \
                     diag(v.x, v.y, v.z) for axis-aligned supercells."
                        .to_string(),
                ),
            ),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "supercell".to_string(),
        description:
            "Rewrites a `Structure` with a larger unit cell defined by a 3×3 integer matrix. \
            Each row of the matrix gives a new basis vector as an integer combination of the \
            old basis vectors (row 0 = a, row 1 = b, row 2 = c). The physical crystal field \
            is unchanged; only the representation changes. When the optional `diagonal` pin \
            is connected, the stored matrix is overridden by diag(v.x, v.y, v.z)."
                .to_string(),
        summary: None,
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![
            Parameter {
                id: None,
                name: "structure".to_string(),
                data_type: DataType::Structure,
            },
            Parameter {
                id: None,
                name: "diagonal".to_string(),
                data_type: DataType::IVec3,
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::Structure),
        public: true,
        node_data_creator: || Box::new(SupercellData::default()),
        node_data_saver: generic_node_data_saver::<SupercellData>,
        node_data_loader: generic_node_data_loader::<SupercellData>,
    }
}
