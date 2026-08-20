//! `sample_field` — evaluates a `ScalarField` at one point.
//!
//! Small, pure, and the thing that makes the whole `.cube` ingestion half
//! testable inside the running application: wire a `vec3` into it, wire the
//! result into `print`, and read values off the Console. It is also a genuine
//! analysis capability — sampling an orbital at a point, or along a bond via
//! `map` over a point array, works unchanged through the headless CLI.
//! See `doc/design_scalar_fields.md`.
//!
//! **Out-of-bounds returns `0.0`, exactly as `ScalarField::sample` specifies.**
//! Not an error, and not a second convention layered on top of the trait's. A
//! finite cube box is a *window* onto a field that decays to zero, so `0.0` is
//! the physically correct answer just outside it; making it an error would fail
//! a whole `map` the moment one point strayed past the box edge — exactly the
//! region where a decaying field is most interesting. The units bug that a
//! bounds error would have diagnosed is caught one phase earlier and far more
//! legibly by `import_cube`'s `molecule` pin, which renders 1.89x too large.

use crate::data_type::DataType;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use serde::{Deserialize, Serialize};

/// Stateless: both inputs are wired, there is nothing to store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleFieldData {}

impl NodeData for SampleFieldData {
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
        let field_arg =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if field_arg.is_error() {
            return EvalOutput::single(field_arg);
        }
        let NetworkResult::ScalarField(field) = field_arg else {
            return EvalOutput::single(NetworkResult::Error(
                "sample_field: field input is not a ScalarField".to_string(),
            ));
        };

        let point_arg =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        if point_arg.is_error() {
            return EvalOutput::single(point_arg);
        }
        let Some(point) = point_arg.extract_vec3() else {
            return EvalOutput::single(NetworkResult::Error(
                "sample_field: point input is not a Vec3".to_string(),
            ));
        };

        EvalOutput::single(NetworkResult::Float(field.sample(point)))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "sample_field".to_string(),
        description: "Evaluates a scalar field (from import_cube) at one point in space and \
            returns the value as a Float.
The point is in ordinary real-space Angstrom, like every other position in the app. \
The value comes back in whatever atomic unit the source quantity uses, unconverted, so the \
threshold conventions published in the chemistry literature apply unchanged.
Sampling outside the region the file covers returns 0.0 rather than an error - a finite box \
is a window onto a field that decays to zero."
            .to_string(),
        summary: Some("Value of a scalar field at a point".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "field".to_string(),
                data_type: DataType::ScalarField,
            },
            Parameter {
                id: None,
                name: "point".to_string(),
                data_type: DataType::Vec3,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Float),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(SampleFieldData {}),
        node_data_saver: generic_node_data_saver::<SampleFieldData>,
        node_data_loader: generic_node_data_loader::<SampleFieldData>,
    }
}
