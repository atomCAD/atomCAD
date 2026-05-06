use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::iterator_walker::Walker;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::{NodeTypeRegistry, RecordTypeDef};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductData {
    /// Name of a record type def in the project's registry. Wrapped as
    /// `RecordType::Named(self.target.clone())` at use time. An empty string
    /// means "no target chosen yet".
    #[serde(default)]
    pub target: String,
}

impl NodeData for ProductData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    /// `product`'s parameters and output pin depend on the registry, not on
    /// data alone. The registry-aware path in
    /// `NodeTypeRegistry::populate_custom_node_type_cache_with_types` installs
    /// the cached `NodeType` for the node.
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
        let Some(def) = registry.lookup_record_type_def(&self.target) else {
            return EvalOutput::single(NetworkResult::None);
        };

        // Empty-target def: cartesian product of zero axes is a single
        // record value with no fields. Emit a one-element iterator.
        if def.fields.is_empty() {
            return EvalOutput::single(NetworkResult::Iterator(Walker::from_array(vec![
                NetworkResult::record(vec![]),
            ])));
        }

        // Read each Iter[T_i] input in authored order (param index = field
        // index). The pin type is `Iter[FieldType]` after Phase 6, so the
        // wire-time conversion produces an `Iterator`. Belt-and-braces, also
        // accept `Array` (in case a wire path delivers it) and wrap.
        let field_names: Vec<String> = def.fields.iter().map(|(n, _)| n.clone()).collect();
        let mut axes: Vec<Walker> = Vec::with_capacity(def.fields.len());
        for (param_index, (field_name, _)) in def.fields.iter().enumerate() {
            let value = network_evaluator.evaluate_arg(
                network_stack,
                node_id,
                registry,
                context,
                param_index,
            );
            match value {
                NetworkResult::None => return EvalOutput::single(NetworkResult::None),
                NetworkResult::Error(_) => return EvalOutput::single(value),
                NetworkResult::Iterator(w) => axes.push(w),
                NetworkResult::Array(items) => axes.push(Walker::from_array(items)),
                other => {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "product: input '{}' did not resolve to an iterator (got {})",
                        field_name,
                        other.to_display_string()
                    )));
                }
            }
        }

        EvalOutput::single(NetworkResult::Iterator(Walker::product(axes, field_names)))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if self.target.is_empty() {
            None
        } else {
            Some(self.target.clone())
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("target".to_string(), TextValue::String(self.target.clone()))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("target") {
            self.target = v
                .as_string()
                .ok_or_else(|| "target must be a String".to_string())?
                .to_string();
        }
        Ok(())
    }
}

/// Build the `NodeType` for a `product` node bound to the given target def
/// name. Each field of the target becomes an `Iter[FieldType]` input pin in
/// authored order; the single output pin is `Iter[Record(Named(target))]`.
/// When `target` is empty or missing from the registry the parameter list is
/// empty and the output is `Iter[Record(Named(target))]` (dangling — fails
/// subtyping against any consumer).
pub fn build_node_type_for_target(
    base_node_type: &NodeType,
    target: &str,
    registry: &NodeTypeRegistry,
) -> NodeType {
    build_node_type_for_target_with_defs(
        base_node_type,
        target,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
    )
}

/// Same as `build_node_type_for_target`, but takes the record-type-defs maps
/// directly so the cache populator can call it without conflicting with a
/// concurrent `&mut node_networks` borrow on the registry. Looks up the
/// target name in `record_type_defs` first, then `built_in_record_type_defs`.
pub fn build_node_type_for_target_with_defs(
    base_node_type: &NodeType,
    target: &str,
    record_type_defs: &HashMap<String, RecordTypeDef>,
    built_in_record_type_defs: &HashMap<String, RecordTypeDef>,
) -> NodeType {
    let mut custom = base_node_type.clone();
    custom.output_pins = OutputPinDefinition::single_fixed(DataType::Iterator(Box::new(
        DataType::Record(RecordType::Named(target.to_string())),
    )));
    if let Some(def) = record_type_defs
        .get(target)
        .or_else(|| built_in_record_type_defs.get(target))
    {
        custom.parameters = def
            .fields
            .iter()
            .map(|(name, ty)| Parameter {
                id: None,
                name: name.clone(),
                data_type: DataType::Iterator(Box::new(ty.clone())),
            })
            .collect();
    } else {
        custom.parameters = Vec::new();
    }
    custom
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "product".to_string(),
        description:
            "Cartesian product of N input arrays, one per field of the chosen record type def. \
            Each input pin is `Array[FieldType_i]`; the output is \
            `Array[Record(target)]`. Iteration order: rightmost field varies fastest. \
            If any input array is empty, the output is empty."
                .to_string(),
        summary: Some("Cartesian product into records".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // Base parameters/pins are placeholders; the registry-aware cache
        // populator replaces them with per-field Array pins keyed off `target`.
        parameters: vec![],
        output_pins: OutputPinDefinition::single_fixed(DataType::Array(Box::new(
            DataType::Record(RecordType::Named(String::new())),
        ))),
        public: true,
        node_data_creator: || Box::new(ProductData::default()),
        node_data_saver: generic_node_data_saver::<ProductData>,
        node_data_loader: generic_node_data_loader::<ProductData>,
    }
}
