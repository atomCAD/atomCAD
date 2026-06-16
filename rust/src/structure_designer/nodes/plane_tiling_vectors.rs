use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
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
use glam::i32::IVec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stored state for the `plane_tiling_vectors` node: a 2x2 integer superlattice
/// matrix whose rows are the two superlattice vectors expressed in the
/// `(u_axis, v_axis)` basis supplied by the `plane` input. Row 0 = `a`,
/// row 1 = `b`. Default is identity, i.e. the conventional `(1×1)` cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaneTilingVectorsData {
    pub matrix: [[i32; 2]; 2],
}

impl Default for PlaneTilingVectorsData {
    fn default() -> Self {
        Self {
            matrix: [[1, 0], [0, 1]],
        }
    }
}

fn det_i64(m: &[[i32; 2]; 2]) -> i64 {
    let m00 = m[0][0] as i64;
    let m01 = m[0][1] as i64;
    let m10 = m[1][0] as i64;
    let m11 = m[1][1] as i64;
    m00 * m11 - m01 * m10
}

impl NodeData for PlaneTilingVectorsData {
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
        // Pin 0: required DrawingPlane input. Supplies the in-plane lattice
        // basis vectors u_axis, v_axis.
        let plane_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        let plane = match plane_val {
            NetworkResult::Error(_) => return EvalOutput::single(plane_val),
            other => match other.extract_drawing_plane() {
                Some(p) => p,
                None => {
                    return EvalOutput::single(NetworkResult::Error(
                        "plane_tiling_vectors: plane input must be a DrawingPlane".to_string(),
                    ));
                }
            },
        };

        // Pin 1: optional IMat2 superlattice override. If connected, use it in
        // place of the stored matrix (supercell pattern).
        let matrix_arg =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let m: [[i32; 2]; 2] = match matrix_arg {
            NetworkResult::None => self.matrix,
            NetworkResult::Error(_) => return EvalOutput::single(matrix_arg),
            NetworkResult::IMat2(m) => m,
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "plane_tiling_vectors: superlattice input must be IMat2, got {}",
                    other.to_display_string()
                )));
            }
        };

        // Each row of the superlattice expresses a tiling vector as an integer
        // combination of u and v. Do not error on det == 0 — the (then
        // dependent) vectors flow to patch_build, whose linear-independence
        // check on tiling_vectors reports it.
        let u = plane.u_axis;
        let v = plane.v_axis;
        let vec0 = u * m[0][0] + v * m[0][1];
        let vec1 = u * m[1][0] + v * m[1][1];

        EvalOutput::single(NetworkResult::Array(vec![
            NetworkResult::IVec3(vec0),
            NetworkResult::IVec3(vec1),
        ]))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        // When the superlattice pin is connected the stored matrix is not the
        // effective one; show a neutral indicator instead of a misleading det.
        if connected_input_pins.contains("superlattice") {
            return Some("det = ?".to_string());
        }
        Some(format!("det = {}", det_i64(&self.matrix)))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "a".to_string(),
                TextValue::IVec2(IVec2::new(self.matrix[0][0], self.matrix[0][1])),
            ),
            (
                "b".to_string(),
                TextValue::IVec2(IVec2::new(self.matrix[1][0], self.matrix[1][1])),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        for (row_idx, key) in ["a", "b"].iter().enumerate() {
            if let Some(v) = props.get(*key) {
                let iv = v
                    .as_ivec2()
                    .ok_or_else(|| format!("{} must be an IVec2", key))?;
                self.matrix[row_idx] = [iv.x, iv.y];
            }
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("plane".to_string(), (true, None));
        m.insert(
            "superlattice".to_string(),
            (
                false,
                Some(
                    "Optional IMat2 override. When connected, the stored 2×2 is replaced by \
                     the wired integer superlattice; its rows are the two tiling vectors in \
                     the (u_axis, v_axis) basis."
                        .to_string(),
                ),
            ),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "plane_tiling_vectors".to_string(),
        description:
            "Turns a Miller-indexed DrawingPlane plus a 2×2 integer superlattice into the \
            Array[IVec3] tiling vectors consumed by patch_build.tiling_vectors. The plane \
            supplies the in-plane lattice basis u_axis, v_axis; each row of the superlattice \
            gives one tiling vector as an integer combination of u and v (row 0 = a, row 1 = b). \
            Diagonal n×m: rows (n,0),(0,m); √3×√3 R30°: rows (2,1),(-1,1); c(2×2): (1,1),(1,-1). \
            When the optional superlattice pin is connected, the wired IMat2 overrides the \
            stored matrix. The plane must be built from the same UnitCellStruct as \
            patch_build.lattice."
                .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "plane".to_string(),
                data_type: DataType::DrawingPlane,
            },
            Parameter {
                id: None,
                name: "superlattice".to_string(),
                data_type: DataType::IMat2,
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::Array(Box::new(DataType::IVec3))),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(PlaneTilingVectorsData::default()),
        node_data_saver: generic_node_data_saver::<PlaneTilingVectorsData>,
        node_data_loader: generic_node_data_loader::<PlaneTilingVectorsData>,
    }
}
