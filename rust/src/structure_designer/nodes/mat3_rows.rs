use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::common_constants::CONNECTED_PIN_SYMBOL;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{NetworkResult, rows_to_dmat3};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use glam::f64::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stored state for the `mat3_rows` node: a 3x3 float matrix, row-major.
/// `matrix[i]` is the i-th row; text properties `a`, `b`, `c` expose rows 0, 1, 2.
/// Default is identity so an unwired node is the identity constant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mat3RowsData {
    pub matrix: [[f64; 3]; 3],
}

impl Default for Mat3RowsData {
    fn default() -> Self {
        Self {
            matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

impl NodeData for Mat3RowsData {
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
        let row_default =
            |i: usize| DVec3::new(self.matrix[i][0], self.matrix[i][1], self.matrix[i][2]);

        let a = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            row_default(0),
            NetworkResult::extract_vec3,
        ) {
            Ok(v) => v,
            Err(e) => return EvalOutput::single(e),
        };

        let b = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            row_default(1),
            NetworkResult::extract_vec3,
        ) {
            Ok(v) => v,
            Err(e) => return EvalOutput::single(e),
        };

        let c = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            row_default(2),
            NetworkResult::extract_vec3,
        ) {
            Ok(v) => v,
            Err(e) => return EvalOutput::single(e),
        };

        let rows = [[a.x, a.y, a.z], [b.x, b.y, b.z], [c.x, c.y, c.z]];
        EvalOutput::single(NetworkResult::Mat3(rows_to_dmat3(&rows)))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let label = |row: usize, key: &str| -> String {
            if connected_input_pins.contains(key) {
                CONNECTED_PIN_SYMBOL.to_string()
            } else {
                format!(
                    "({:.2},{:.2},{:.2})",
                    self.matrix[row][0], self.matrix[row][1], self.matrix[row][2]
                )
            }
        };
        Some(format!(
            "[{}, {}, {}]",
            label(0, "a"),
            label(1, "b"),
            label(2, "c")
        ))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "a".to_string(),
                TextValue::Vec3(DVec3::new(
                    self.matrix[0][0],
                    self.matrix[0][1],
                    self.matrix[0][2],
                )),
            ),
            (
                "b".to_string(),
                TextValue::Vec3(DVec3::new(
                    self.matrix[1][0],
                    self.matrix[1][1],
                    self.matrix[1][2],
                )),
            ),
            (
                "c".to_string(),
                TextValue::Vec3(DVec3::new(
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
                let dv = v
                    .as_vec3()
                    .ok_or_else(|| format!("{} must be a Vec3", key))?;
                self.matrix[row_idx] = [dv.x, dv.y, dv.z];
            }
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "mat3_rows".to_string(),
        description: "Constructs a Mat3 (3x3 float matrix) from three row vectors. \
            Row 0 = a, row 1 = b, row 2 = c. Unwired rows default to the stored matrix \
            (identity by default)."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
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
        output_pins: OutputPinDefinition::single(DataType::Mat3),
        public: true,
        node_data_creator: || Box::new(Mat3RowsData::default()),
        node_data_saver: generic_node_data_saver::<Mat3RowsData>,
        node_data_loader: generic_node_data_loader::<Mat3RowsData>,
    }
}
