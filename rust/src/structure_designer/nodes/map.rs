use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{DataType, FunctionType};
use crate::structure_designer::evaluator::iterator_walker::Walker;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::zone_closure::obtain_closure;
use crate::structure_designer::node_data::{DragDirection, EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapData {
    pub input_type: DataType,
    pub output_type: DataType,
}

impl NodeData for MapData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom_node_type = base_node_type.clone();

        // External: `xs` (the stream) and the optional `f` (a function value
        // that, when wired, overrides the inline body). The inline body lives
        // inside the zone.
        custom_node_type.parameters[0].data_type =
            DataType::Iterator(Box::new(self.input_type.clone()));
        custom_node_type.parameters[1].data_type = DataType::Function(FunctionType::new(
            vec![self.input_type.clone()],
            self.output_type.clone(),
        ));
        custom_node_type.output_pins =
            OutputPinDefinition::single(DataType::Iterator(Box::new(self.output_type.clone())));

        // Inside-facing pins: one element source, one result destination.
        custom_node_type.zone_input_pins = vec![OutputPinDefinition::fixed(
            "element",
            self.input_type.clone(),
        )];
        custom_node_type.zone_output_pins = vec![Parameter {
            id: None,
            name: "result".to_string(),
            data_type: self.output_type.clone(),
        }];

        Some(custom_node_type)
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
        // a. Resolve `xs` first — runs against the HOF's containing network
        // scope, before the body is pushed.
        let xs_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        let xs_walker = match xs_val {
            NetworkResult::Iterator(w) => w,
            // Belt-and-braces: the implicit `[T] → Iter[T]` wire conversion
            // normally wraps any incoming array as `Iterator(_)` already.
            NetworkResult::Array(items) => Walker::from_array(items),
            err @ NetworkResult::Error(_) => return EvalOutput::single(err),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "map: xs is not an iterator (got {})",
                    other.to_display_string()
                )));
            }
        };

        // b. Obtain the closure to run: the function wired into `f` if
        // connected, otherwise one built from this node's own inline zone
        // (grab the body, freeze captures once, collect the zone-output wires).
        let closure = match obtain_closure(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            1, // `f` pin index
            "map",
        ) {
            Ok(c) => c,
            Err(e) => return EvalOutput::single(e),
        };

        // c. Construct the walker. The closure travels via the walker;
        // subsequent iterations run it once per element via `run_closure_once`.
        let walker = Walker::map_zone(xs_walker, closure);
        EvalOutput::single(NetworkResult::Iterator(walker))
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

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "input_type".to_string(),
                TextValue::DataType(self.input_type.clone()),
            ),
            (
                "output_type".to_string(),
                TextValue::DataType(self.output_type.clone()),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("input_type") {
            self.input_type = v
                .as_data_type()
                .ok_or_else(|| "input_type must be a DataType".to_string())?
                .clone();
        }
        if let Some(v) = props.get("output_type") {
            self.output_type = v
                .as_data_type()
                .ok_or_else(|| "output_type must be a DataType".to_string())?
                .clone();
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("xs".to_string(), (true, None)); // required
        // `f` is optional: when disconnected, the inline zone body drives map.
        m.insert("f".to_string(), (false, None));
        m
    }

    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        _direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        // Both directions accept the same shape: peel `Iter[T]` / `Array[T]` /
        // `T`. The over-promised case (e.g. `FromInput` with a scalar `T`) is
        // caught by the filter's static-match verification step.
        let elem = source_type.drag_element_type_from_output()?;
        Some(Box::new(MapData {
            input_type: elem.clone(),
            output_type: elem,
        }))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "map".to_string(),
      description: "Lazily applies the inline zone body to every element pulled from `xs`, producing an iterator of the resulting values. The intermediate sequence is never materialised — downstream consumers (`fold`, `collect`, …) drive the stream one element at a time. The body reads the per-element value from the inside-facing `element` source pin and delivers its result to the inside-facing `result` destination pin.".to_string(),
      summary: None,
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
          id: None,
          name: "xs".to_string(),
          data_type: DataType::Iterator(Box::new(DataType::Float)), // will change based on  ParameterData::data_type.
        },
        Parameter {
          id: None,
          name: "f".to_string(),
          // Optional function value. When wired, it overrides the inline zone
          // body. Type tracks input/output via `calculate_custom_node_type`.
          data_type: DataType::Function(FunctionType::new(
            vec![DataType::Float],
            DataType::Float,
          )),
        },
      ],
      output_pins: OutputPinDefinition::single(DataType::Iterator(Box::new(DataType::Float))), // will change based on the output type
      zone_input_pins: vec![OutputPinDefinition::fixed("element", DataType::Float)],
      zone_output_pins: vec![Parameter {
        id: None,
        name: "result".to_string(),
        data_type: DataType::Float,
      }],
      public: true,
      node_data_creator: || Box::new(MapData {
        input_type: DataType::Float,
        output_type: DataType::Float,
      }),
      node_data_saver: generic_node_data_saver::<MapData>,
      node_data_loader: generic_node_data_loader::<MapData>,
    }
}
