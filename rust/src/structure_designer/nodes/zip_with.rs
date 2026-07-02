//! The `zip_with` node: n-ary element-wise map (issue #382).
//!
//! Combines N input streams element-wise with an N-argument function — the
//! variadic generalization of `map` (`map` == zipWith1, Haskell's `zipWith` ==
//! zipWith2, `zipWith3`, …). The N lanes use fixed, position-derived pin names
//! (`xs1..xsN` external, `element1..elementN` inside the zone); lane identity
//! is carried by a hidden stable id per lane stamped onto `Parameter.id`, never
//! by the name, so external wires follow their lane across removal of an
//! earlier lane. See `doc/design_zip_with.md`.

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::iterator_walker::Walker;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::zone_closure::obtain_closure;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipWithLane {
    /// Hidden stable identity; wires survive lane removal. Stamped onto the
    /// external `Parameter.id` so by-id argument rebuilds
    /// (`node_network.rs::set_custom_node_type`) follow the lane.
    #[serde(default)]
    pub id: Option<u64>,
    pub data_type: DataType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipWithData {
    /// The N input lanes, in pin order. Never empty (the setters reject an
    /// empty lane list; minimum arity is 1 — the degenerate `map`).
    pub lanes: Vec<ZipWithLane>,
    pub output_type: DataType,
    /// Monotonic id source for new lanes. Persisted — deriving it as
    /// max(existing)+1 would recycle the id of a just-removed highest lane,
    /// the same hazard class as the `next_param_id` wire-stability regression
    /// (`doc/design_parameter_wire_stability.md`). `#[serde(default)]`; healed
    /// on load to max(lane ids)+1 when missing/zero. The loader also mints
    /// ids from the healed counter for any lane loaded with `id: None`
    /// (hand-authored files) — an id-less lane silently degrades to
    /// name-based (i.e. positional) wire matching in `set_custom_node_type`,
    /// exactly the fragility the ids exist to prevent.
    #[serde(default)]
    pub next_lane_id: u64,
}

impl Default for ZipWithData {
    fn default() -> Self {
        Self {
            lanes: vec![
                ZipWithLane {
                    id: Some(1),
                    data_type: DataType::Float,
                },
                ZipWithLane {
                    id: Some(2),
                    data_type: DataType::Float,
                },
            ],
            output_type: DataType::Float,
            next_lane_id: 3,
        }
    }
}

impl ZipWithData {
    /// Heal the persisted id state: bump `next_lane_id` past every existing
    /// lane id (covers a missing/zero counter in hand-authored files) and mint
    /// fresh ids for any lane loaded with `id: None`. Idempotent.
    pub fn heal_lane_ids(&mut self) {
        let max_id = self.lanes.iter().filter_map(|l| l.id).max().unwrap_or(0);
        if self.next_lane_id <= max_id {
            self.next_lane_id = max_id + 1;
        }
        for lane in &mut self.lanes {
            if lane.id.is_none() {
                lane.id = Some(self.next_lane_id);
                self.next_lane_id += 1;
            }
        }
    }

    fn lane_types(&self) -> Vec<DataType> {
        self.lanes.iter().map(|l| l.data_type.clone()).collect()
    }
}

impl NodeData for ZipWithData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();

        // External pins: `xs1..xsN` (Iter[T_i]), then the trailing optional
        // `f`. Built from scratch (like `product`/`expr`, not by indexing base
        // parameters — the count varies). Each lane's hidden stable id rides
        // on `Parameter.id` so external wires follow their lane across removal
        // of an earlier lane (the label renumbers, the wire stays).
        custom.parameters = self
            .lanes
            .iter()
            .enumerate()
            .map(|(i, lane)| Parameter {
                id: lane.id,
                name: format!("xs{}", i + 1),
                data_type: DataType::Iterator(Box::new(lane.data_type.clone())),
            })
            .collect();
        // `f` is permanently typed `AnyFunction { leading_params: [T_1..T_N] }`:
        // any function whose parameter list starts with the lane types flows
        // in; excess parameters become the partial-application tail at
        // evaluation, mirroring `map.f` (`doc/design_function_pin_unification.md`).
        custom.parameters.push(Parameter {
            id: None,
            name: "f".to_string(),
            data_type: DataType::AnyFunction {
                leading_params: self.lane_types(),
            },
        });

        custom.output_pins =
            OutputPinDefinition::single(DataType::Iterator(Box::new(self.output_type.clone())));

        // Inside-facing pins: element1..elementN sources, one result
        // destination.
        custom.zone_input_pins = self
            .lanes
            .iter()
            .enumerate()
            .map(|(i, lane)| {
                OutputPinDefinition::fixed(&format!("element{}", i + 1), lane.data_type.clone())
            })
            .collect();
        custom.zone_output_pins = vec![Parameter {
            id: None,
            name: "result".to_string(),
            data_type: self.output_type.clone(),
        }];

        Some(custom)
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
        // a. Resolve every lane first — runs against the HOF's containing
        // network scope, before the body is pushed.
        let mut sources = Vec::with_capacity(self.lanes.len());
        for i in 0..self.lanes.len() {
            let val = network_evaluator.evaluate_arg_required(
                network_stack,
                node_id,
                registry,
                context,
                i,
            );
            let walker = match val {
                NetworkResult::Iterator(w) => w,
                // Belt-and-braces: the implicit `[T] → Iter[T]` wire conversion
                // normally wraps any incoming array as `Iterator(_)` already.
                NetworkResult::Array(items) => Walker::from_array(items),
                err @ NetworkResult::Error(_) => return EvalOutput::single(err),
                other => {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "zip_with: input 'xs{}' is not an iterator (got {})",
                        i + 1,
                        other.to_display_string()
                    )));
                }
            };
            sources.push(walker);
        }

        // b. Obtain the closure to run: the function wired into `f` if
        // connected, otherwise one built from this node's own inline zone.
        // Construction errors return a single Error — never a degenerate
        // walker (errors would multiply per element).
        let closure = match obtain_closure(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            self.lanes.len(), // `f` pin index (after the lane pins)
            "zip_with",
        ) {
            Ok(c) => c,
            Err(e) => return EvalOutput::single(e),
        };

        // c. Construct the walker; iteration runs the closure once per pulled
        // frame via the shared run-frame step.
        let walker = Walker::zip_zone(sources, closure);
        EvalOutput::single(NetworkResult::Iterator(walker))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let lanes = self
            .lanes
            .iter()
            .map(|l| l.data_type.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("({}) → {}", lanes, self.output_type))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "lane_types".to_string(),
                TextValue::Array(
                    self.lanes
                        .iter()
                        .map(|l| TextValue::DataType(l.data_type.clone()))
                        .collect(),
                ),
            ),
            (
                "output_type".to_string(),
                TextValue::DataType(self.output_type.clone()),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("lane_types") {
            let arr = v
                .as_array()
                .ok_or_else(|| "lane_types must be an array of data types".to_string())?;
            let mut types = Vec::with_capacity(arr.len());
            for item in arr {
                types.push(
                    item.as_data_type()
                        .ok_or_else(|| "lane_types entries must be DataTypes".to_string())?
                        .clone(),
                );
            }
            if types.is_empty() {
                return Err("zip_with requires at least one lane".to_string());
            }
            // Positional id merge: the lane at position i keeps the old
            // position-i id (retype preserves identity), growth mints fresh
            // ids from `next_lane_id` (never reusing a consumed id), shrink
            // drops the tail. Body-wire cleanup for dropped tail indices is a
            // Phase 3 deliverable (see `doc/design_zip_with.md`).
            self.lanes = types
                .into_iter()
                .enumerate()
                .map(|(i, data_type)| {
                    let id = match self.lanes.get(i).and_then(|l| l.id) {
                        Some(id) => id,
                        None => {
                            let id = self.next_lane_id;
                            self.next_lane_id += 1;
                            id
                        }
                    };
                    ZipWithLane {
                        id: Some(id),
                        data_type,
                    }
                })
                .collect();
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
        for i in 0..self.lanes.len() {
            m.insert(format!("xs{}", i + 1), (true, None)); // required
        }
        // `f` is optional: when disconnected, the inline zone body drives the
        // zip.
        m.insert("f".to_string(), (false, None));
        m
    }
}

/// Loader that heals persisted lane-id state (missing/zero `next_lane_id`,
/// id-less lanes in hand-authored files) after the generic deserialize.
fn zip_with_node_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: ZipWithData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    data.heal_lane_ids();
    Ok(Box::new(data))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "zip_with".to_string(),
        description: "Combines N input streams element-wise with an N-argument function (an n-ary map, also known as zipWith or multimap): lazily pulls one element from every lane per step and applies the inline zone body — or the function wired into `f` — to the pulled frame, producing an iterator of the results. The stream ends with the shortest input (empty lane → empty output). The body reads the per-lane values from the inside-facing `element1..elementN` source pins and delivers its result to the inside-facing `result` destination pin. Lanes are added, removed, and retyped in the property panel. To combine each element with a constant, reference the constant from inside the body (a capture) instead of wiring it to a lane — a scalar-fed lane broadcasts to a single-element stream and ends the zip after one element.".to_string(),
        summary: Some("n-ary element-wise map (zipWith / multimap)".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // The 2-lane Float default, mirroring `ZipWithData::default()` (ids
        // included) so the data-driven custom type equals this base layout and
        // `has_zone()` is true from registration, before any custom-type cache
        // runs.
        parameters: vec![
            Parameter {
                id: Some(1),
                name: "xs1".to_string(),
                data_type: DataType::Iterator(Box::new(DataType::Float)),
            },
            Parameter {
                id: Some(2),
                name: "xs2".to_string(),
                data_type: DataType::Iterator(Box::new(DataType::Float)),
            },
            Parameter {
                id: None,
                name: "f".to_string(),
                // Optional function value. When wired, it overrides the inline
                // zone body. Declared type tracks the lane types via
                // `calculate_custom_node_type`.
                data_type: DataType::AnyFunction {
                    leading_params: vec![DataType::Float, DataType::Float],
                },
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::Iterator(Box::new(DataType::Float))),
        zone_input_pins: vec![
            OutputPinDefinition::fixed("element1", DataType::Float),
            OutputPinDefinition::fixed("element2", DataType::Float),
        ],
        zone_output_pins: vec![Parameter {
            id: None,
            name: "result".to_string(),
            data_type: DataType::Float,
        }],
        public: true,
        node_data_creator: || Box::new(ZipWithData::default()),
        node_data_saver: generic_node_data_saver::<ZipWithData>,
        node_data_loader: zip_with_node_data_loader,
    }
}
