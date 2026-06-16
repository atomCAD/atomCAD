use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::common_constants::CONNECTED_PIN_SYMBOL;
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

/// Stored state for the `imat2_cols` node: a 2x2 integer matrix, row-major.
/// Text properties `a`, `b` expose columns 0, 1 of the stored matrix.
/// Default is identity so an unwired node is the identity constant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMat2ColsData {
    pub matrix: [[i32; 2]; 2],
}

impl Default for IMat2ColsData {
    fn default() -> Self {
        Self {
            matrix: [[1, 0], [0, 1]],
        }
    }
}

impl NodeData for IMat2ColsData {
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
        let col_default = |j: usize| IVec2::new(self.matrix[0][j], self.matrix[1][j]);

        let a = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            col_default(0),
            NetworkResult::extract_ivec2,
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
            col_default(1),
            NetworkResult::extract_ivec2,
        ) {
            Ok(v) => v,
            Err(e) => return EvalOutput::single(e),
        };

        EvalOutput::single(NetworkResult::IMat2([[a.x, b.x], [a.y, b.y]]))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let label = |col: usize, key: &str| -> String {
            if connected_input_pins.contains(key) {
                CONNECTED_PIN_SYMBOL.to_string()
            } else {
                format!("({},{})", self.matrix[0][col], self.matrix[1][col])
            }
        };
        Some(format!("[{} | {}]", label(0, "a"), label(1, "b")))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "a".to_string(),
                TextValue::IVec2(IVec2::new(self.matrix[0][0], self.matrix[1][0])),
            ),
            (
                "b".to_string(),
                TextValue::IVec2(IVec2::new(self.matrix[0][1], self.matrix[1][1])),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        for (col_idx, key) in ["a", "b"].iter().enumerate() {
            if let Some(v) = props.get(*key) {
                let iv = v
                    .as_ivec2()
                    .ok_or_else(|| format!("{} must be an IVec2", key))?;
                self.matrix[0][col_idx] = iv.x;
                self.matrix[1][col_idx] = iv.y;
            }
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "imat2_cols".to_string(),
        description: "Constructs an IMat2 (2x2 integer matrix) from two column vectors. \
            Column 0 = a, column 1 = b. Unwired columns default to the stored \
            matrix's columns (identity by default)."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "a".to_string(),
                data_type: DataType::IVec2,
            },
            Parameter {
                id: None,
                name: "b".to_string(),
                data_type: DataType::IVec2,
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::IMat2),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(IMat2ColsData::default()),
        node_data_saver: generic_node_data_saver::<IMat2ColsData>,
        node_data_loader: generic_node_data_loader::<IMat2ColsData>,
    }
}
