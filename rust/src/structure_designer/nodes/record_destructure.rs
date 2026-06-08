use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{DragDirection, EvalOutput, NodeData};
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
pub struct RecordDestructureData {
    /// Name of a record type def in the project's registry. Wrapped as
    /// `RecordType::Named(self.schema.clone())` at use time. An empty string
    /// means "no schema chosen yet".
    #[serde(default)]
    pub schema: String,
}

impl NodeData for RecordDestructureData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    /// `record_destructure`'s input pin type and per-field output pins are
    /// derived from the registry. The registry-aware path in
    /// `NodeTypeRegistry::populate_custom_node_type_cache_with_types`
    /// installs the cached `NodeType`.
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
            // Empty or dangling schema. There are no real per-field output
            // pins (only the placeholder), so emit a single None.
            return EvalOutput::single(NetworkResult::None);
        };

        // Read the input record. Pin 0 of the parameters is the record input
        // (set by the registry-aware cache populator).
        let record_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        match &record_val {
            NetworkResult::None => {
                // No input — every output pin emits None.
                let nones = vec![NetworkResult::None; def.fields.len().max(1)];
                return EvalOutput::multi(nones);
            }
            NetworkResult::Error(_) => {
                let errs = vec![record_val.clone(); def.fields.len().max(1)];
                return EvalOutput::multi(errs);
            }
            _ => {}
        }

        // Pass-through: the runtime record may carry extra fields beyond the
        // schema. Look up each declared field by name (binary search on the
        // canonical field list); fields declared in the schema but missing
        // from the runtime value emit None on the corresponding pin.
        if def.fields.is_empty() {
            // Match the placeholder output pin (one None) used when the
            // schema declares zero fields.
            return EvalOutput::multi(vec![NetworkResult::None]);
        }
        let mut outputs: Vec<NetworkResult> = Vec::with_capacity(def.fields.len());
        for (field_name, _) in &def.fields {
            let value = record_val
                .extract_record_field(field_name)
                .cloned()
                .unwrap_or(NetworkResult::None);
            outputs.push(value);
        }
        EvalOutput::multi(outputs)
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

    /// `record_destructure`'s `record` input pin accepts a record, so a record
    /// dragged off an *output* pin (`FromOutput`) can feed it. Adapt by
    /// adopting the dragged record's schema name. `FromInput` is not
    /// adaptable: the node's output pins are the individual fields, not a
    /// record. See `doc/design_drag_aware_add_node.md` and issue #312.
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        if direction != DragDirection::FromOutput {
            return None;
        }
        // Only named record types carry a registry name we can store as the
        // schema; anonymous records have no def to reference.
        let DataType::Record(RecordType::Named(name)) = source_type else {
            return None;
        };
        // Don't pre-set a dangling schema — require the def to resolve.
        registry.lookup_record_type_def(name)?;
        Some(Box::new(RecordDestructureData {
            schema: name.clone(),
        }))
    }
}

/// Build the `NodeType` for a `record_destructure` node bound to the given
/// schema name. The single input pin is `record: Record(Named(schema))`.
/// The output pins are one per field of the def, in authored order. When
/// the schema is empty/missing or has zero fields, the input pin remains
/// `Record(Named(schema))` (dangling and self-evidently broken when empty)
/// and a single placeholder output pin is preserved so the
/// `output_pins.len() >= 1` invariant holds.
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
    custom.parameters = vec![Parameter {
        id: None,
        name: "record".to_string(),
        data_type: DataType::Record(RecordType::Named(schema.to_string())),
    }];
    let resolved = record_type_defs
        .get(schema)
        .or_else(|| built_in_record_type_defs.get(schema));
    let pins: Vec<OutputPinDefinition> = match resolved {
        Some(def) if !def.fields.is_empty() => def
            .fields
            .iter()
            .map(|(name, ty)| OutputPinDefinition::fixed(name, ty.clone()))
            .collect(),
        _ => vec![OutputPinDefinition::fixed("result", DataType::None)],
    };
    custom.output_pins = pins;
    custom
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "record_destructure".to_string(),
        description:
            "Splits a record value into one output per field of the chosen record type def. \
            Output pin order matches the def's authored field order. Extra fields on the \
            input record (beyond what the schema declares) are ignored (pass-through)."
                .to_string(),
        summary: Some("Read fields of a record".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // Base parameters/pins are placeholders; the registry-aware cache
        // populator replaces them with per-field pins keyed off `schema`.
        parameters: vec![Parameter {
            id: None,
            name: "record".to_string(),
            data_type: DataType::Record(RecordType::Named(String::new())),
        }],
        output_pins: vec![OutputPinDefinition::fixed("result", DataType::None)],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(RecordDestructureData::default()),
        node_data_saver: generic_node_data_saver::<RecordDestructureData>,
        node_data_loader: generic_node_data_loader::<RecordDestructureData>,
    }
}
