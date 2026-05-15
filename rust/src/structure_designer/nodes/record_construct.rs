use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{DataType, RecordType};
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
pub struct RecordConstructData {
    /// Name of a record type def in the project's registry. Wrapped as
    /// `RecordType::Named(self.schema.clone())` at use time. An empty string
    /// means "no schema chosen yet".
    #[serde(default)]
    pub schema: String,

    /// Per-field stored literal values, consulted in `eval` when the
    /// corresponding input pin is unwired. Keyed by field name. Entries whose
    /// key isn't a current field of the chosen def are inert (orphan-tolerant):
    /// `eval` ignores them and the property-panel getter does not surface them.
    #[serde(default)]
    pub literal_values: HashMap<String, TextValue>,
}

impl NodeData for RecordConstructData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    /// `record_construct`'s parameters and output pin depend on the registry,
    /// not on data alone. The registry-aware path in
    /// `NodeTypeRegistry::populate_custom_node_type_cache_with_types`
    /// installs the cached `NodeType` for record nodes.
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
        let Some(def) = registry.lookup_record_type_def(&self.schema) else {
            // Empty or dangling schema. The output type fails subtyping
            // against any consumer — produce None so downstream destructure
            // / consumer nodes don't see a partial value.
            return EvalOutput::single(NetworkResult::None);
        };

        // Resolve `node` for the wire-state check below. Should always succeed
        // — we are mid-eval on this node — but bail defensively.
        let Some(node) = network_stack
            .last()
            .and_then(|frame| frame.node_network.nodes.get(&node_id))
        else {
            return EvalOutput::single(NetworkResult::None);
        };

        // Walk the def's authored fields in order; the parameter pin layout
        // matches that order (set by the registry-aware cache populator).
        let mut fields: Vec<(String, NetworkResult)> = Vec::with_capacity(def.fields.len());
        for (param_index, (field_name, field_type)) in def.fields.iter().enumerate() {
            // Wired > literal > pass-None-through. A wired pin always wins;
            // an unwired pin with a stored literal uses that literal (if it
            // still coerces to the field type); otherwise we fall through to
            // `evaluate_arg`, which on an unwired pin yields None and
            // short-circuits the whole record below.
            let wired = node
                .arguments
                .get(param_index)
                .map(|arg| !arg.is_empty())
                .unwrap_or(false);
            let value = if wired {
                network_evaluator.evaluate_arg(
                    network_stack,
                    node_id,
                    registry,
                    context,
                    param_index,
                )
            } else if let Some(literal) = self
                .literal_values
                .get(field_name)
                .and_then(|tv| tv.to_network_result(field_type))
            {
                literal
            } else {
                network_evaluator.evaluate_arg(
                    network_stack,
                    node_id,
                    registry,
                    context,
                    param_index,
                )
            };
            match &value {
                NetworkResult::None => return EvalOutput::single(NetworkResult::None),
                NetworkResult::Error(_) => return EvalOutput::single(value),
                _ => {}
            }
            fields.push((field_name.clone(), value));
        }

        // `record(..)` re-sorts into canonical order.
        EvalOutput::single(NetworkResult::record(fields))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if self.schema.is_empty() {
            None
        } else {
            Some(self.schema.clone())
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("schema".to_string(), TextValue::String(self.schema.clone()))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("schema") {
            self.schema = v
                .as_string()
                .ok_or_else(|| "schema must be a String".to_string())?
                .to_string();
        }
        Ok(())
    }
}

/// Build the `NodeType` for a `record_construct` node bound to the given
/// schema name. Returns the bare base type (with no parameters and a
/// dangling-named output) when the schema is empty or missing from the
/// registry — the resulting output type fails subtyping against any
/// consumer, so downstream wires get disconnected by network validation.
pub fn build_node_type_for_schema(
    base_node_type: &NodeType,
    schema: &str,
    registry: &NodeTypeRegistry,
) -> NodeType {
    build_node_type_for_schema_with_defs(
        base_node_type,
        schema,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
    )
}

/// Same as `build_node_type_for_schema`, but takes the record-type-defs maps
/// directly so the cache populator can call it without conflicting with a
/// concurrent `&mut node_networks` borrow on the registry. Looks up the
/// schema name in `record_type_defs` first, then `built_in_record_type_defs`.
pub fn build_node_type_for_schema_with_defs(
    base_node_type: &NodeType,
    schema: &str,
    record_type_defs: &HashMap<String, RecordTypeDef>,
    built_in_record_type_defs: &HashMap<String, RecordTypeDef>,
) -> NodeType {
    let mut custom = base_node_type.clone();
    custom.output_pins =
        OutputPinDefinition::single_fixed(DataType::Record(RecordType::Named(schema.to_string())));
    if let Some(def) = record_type_defs
        .get(schema)
        .or_else(|| built_in_record_type_defs.get(schema))
    {
        // Pin order = authored field order.
        custom.parameters = def
            .fields
            .iter()
            .map(|(name, ty)| Parameter {
                id: None,
                name: name.clone(),
                data_type: ty.clone(),
            })
            .collect();
    } else {
        custom.parameters = Vec::new();
    }
    custom
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "record_construct".to_string(),
        description:
            "Builds a record value from one input per field of the chosen record type def. \
            Pin order matches the def's authored field order; the runtime value is stored in \
            canonical (sorted) order."
                .to_string(),
        summary: Some("Construct a record value".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // Base parameters/pins are placeholders; the registry-aware cache
        // populator replaces them with per-field pins keyed off `schema`.
        parameters: vec![],
        output_pins: OutputPinDefinition::single_fixed(DataType::Record(RecordType::Named(
            String::new(),
        ))),
        public: true,
        node_data_creator: || Box::new(RecordConstructData::default()),
        node_data_saver: generic_node_data_saver::<RecordConstructData>,
        node_data_loader: generic_node_data_loader::<RecordConstructData>,
    }
}
