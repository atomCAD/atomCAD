//! The `switch` node: select a value by matching a selector against literal
//! cases (`doc/design_switch_node.md`).
//!
//! `if` gave the network a lazy two-way branch on a `Bool`; `switch` is the
//! n-way generalization keyed by a value:
//!
//! - **Selector** — a `value` input pin of a user-selected *selector type*
//!   (**Int or String**).
//! - **Cases** — a user-edited list of literal case values (edited in the
//!   property panel, not wired). Each case contributes one input pin whose name
//!   is derived from its value (`case_5`, `case_slot_a`), typed by a separate
//!   user-selected *value type* (any concrete type, like `if.value_type`).
//! - **Default** — a fixed trailing `default` pin of the same value type.
//! - **Output** — a single pin of the value type.
//!
//! Like `if`, this is not expressible with `expr` (which cannot carry
//! structural values and eagerly evaluates every wired input), and like
//! `zip_with` the variadic pin list must survive case edits without dropping
//! wires — case identity rides on a hidden stable `id` per case stamped onto
//! `Parameter.id`, never on the derived name.

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{DragDirection, EvalOutput, NodeData};
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

/// A single case literal, always matching the node's `selector_type`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SwitchCaseValue {
    Int(i32),
    String(String),
}

impl SwitchCaseValue {
    /// Human-readable rendering used in error messages.
    pub fn to_display_string(&self) -> String {
        match self {
            SwitchCaseValue::Int(i) => i.to_string(),
            SwitchCaseValue::String(s) => format!("{:?}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchCase {
    /// Hidden stable identity; wires survive case-value edits and removals.
    /// Stamped onto the external `Parameter.id` so by-id argument rebuilds
    /// (`node_network.rs::set_custom_node_type`) follow the case across a
    /// removal or reorder.
    #[serde(default)]
    pub id: Option<u64>,
    pub value: SwitchCaseValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchData {
    /// Type of the `value` selector pin. Restricted to Int | String — validated
    /// by every setter; a hand-authored other type is healed at load
    /// (Phase 3).
    pub selector_type: DataType,
    /// Type of the case pins, the `default` pin, and the output pin.
    pub value_type: DataType,
    /// The case literals, in pin order. Never empty (setters reject an empty
    /// list; the minimum is one case). Values are unique through supported
    /// edit paths.
    pub cases: Vec<SwitchCase>,
    /// Monotonic id source for new cases. Persisted — max(existing)+1 would
    /// recycle the id of a just-removed highest case, the `next_param_id`
    /// wire-stability hazard (`doc/design_parameter_wire_stability.md`).
    #[serde(default)]
    pub next_case_id: u64,
}

impl Default for SwitchData {
    fn default() -> Self {
        Self {
            selector_type: DataType::Int,
            value_type: DataType::Float,
            cases: vec![
                SwitchCase {
                    id: Some(1),
                    value: SwitchCaseValue::Int(0),
                },
                SwitchCase {
                    id: Some(2),
                    value: SwitchCaseValue::Int(1),
                },
            ],
            next_case_id: 3,
        }
    }
}

impl SwitchData {
    /// The derived external/inside pin name for each case, the single source of
    /// truth used by `calculate_custom_node_type` (and hence the text-format
    /// serializer/parser). Wire identity never depends on these names — they
    /// are cosmetic (external wires are matched by id) — so renames are free.
    ///
    /// - **Int**: `case_5`; negative values render the sign as `neg` →
    ///   `case_neg3` (`-` is not an identifier char).
    /// - **String**: `case_` + sanitized value (alphanumeric — unicode, matching
    ///   the lexer — and `_` kept, every other char → `_`, truncated to 24
    ///   chars). A value that sanitizes to nothing yields the bare name `case_`.
    /// - **Dedup**: distinct values that produce the same base name keep the
    ///   first occurrence bare and append `__2`, `__3`, … to later collisions
    ///   in list order. Deterministic given the case list, so serialize → parse
    ///   round-trips agree on names.
    pub fn derived_case_pin_names(&self) -> Vec<String> {
        let mut names = Vec::with_capacity(self.cases.len());
        let mut seen: HashMap<String, u32> = HashMap::new();
        for case in &self.cases {
            let base = match &case.value {
                SwitchCaseValue::Int(i) => {
                    if *i < 0 {
                        format!("case_neg{}", i.unsigned_abs())
                    } else {
                        format!("case_{}", i)
                    }
                }
                SwitchCaseValue::String(s) => {
                    let sanitized: String = s
                        .chars()
                        .map(|c| {
                            if c.is_alphanumeric() || c == '_' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .take(24)
                        .collect();
                    format!("case_{}", sanitized)
                }
            };
            let count = seen.entry(base.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                names.push(base);
            } else {
                names.push(format!("{}__{}", base, count));
            }
        }
        names
    }

    /// The value type of the case pins / `default` / output.
    fn case_pin_type(&self) -> DataType {
        self.value_type.clone()
    }

    /// Whole-list value-keyed id merge (`doc/design_switch_node.md`), shared by
    /// `set_text_properties` and (Phase 2) the `StructureDesigner`-level setter.
    ///
    /// Because case values are a **unique key**, a single whole-list setter is
    /// id-accurate. The pass separation is load-bearing (see the design doc):
    /// resolving *all* value matches before any positional fallback prevents an
    /// early positional fallback from stealing the id a later value match needs.
    ///
    /// 1. Reject duplicates / empty list (node left unchanged on error).
    /// 2. **Value-match pass**: a new value equal to an old case's value keeps
    ///    that old case's id (removal + reorder).
    /// 3. **Positional-fallback pass**: a still-unmatched new value inherits the
    ///    id of the old case at its index if that case exists and its id was not
    ///    consumed by the value-match pass (in-place value edit — the wire
    ///    follows).
    /// 4. **Mint pass**: any new value still without an id takes `next_case_id`.
    pub fn merge_cases(&mut self, new_values: Vec<SwitchCaseValue>) -> Result<(), String> {
        if new_values.is_empty() {
            return Err("switch requires at least one case".to_string());
        }
        for i in 0..new_values.len() {
            for j in (i + 1)..new_values.len() {
                if new_values[i] == new_values[j] {
                    return Err(format!(
                        "duplicate case value: {}",
                        new_values[i].to_display_string()
                    ));
                }
            }
        }

        let old = std::mem::take(&mut self.cases);
        let mut consumed = vec![false; old.len()];
        let mut assigned: Vec<Option<u64>> = vec![None; new_values.len()];

        // Pass 2: value match. Old and new values are each unique, so a given
        // new value matches at most one old case and no two new values contend
        // for the same old id — the match is unambiguous.
        for (ni, nv) in new_values.iter().enumerate() {
            if let Some(oi) = old.iter().position(|c| &c.value == nv)
                && let Some(id) = old[oi].id
            {
                assigned[ni] = Some(id);
                consumed[oi] = true;
            }
        }

        // Pass 3: positional fallback for unmatched new values. A new value at
        // index `ni` inherits the old case at the *same* index, if that old case
        // exists and its id was not already claimed by the value-match pass. The
        // `consumed`/`old` vectors are old-length, so `ni` can run past them when
        // the list grows — treat an out-of-range index as "no old case here"
        // (mint below) rather than indexing out of bounds.
        for (ni, slot) in assigned.iter_mut().enumerate() {
            if slot.is_some() || consumed.get(ni).copied().unwrap_or(true) {
                continue;
            }
            if let Some(id) = old.get(ni).and_then(|c| c.id) {
                *slot = Some(id);
                consumed[ni] = true;
            }
        }

        // Pass 4: mint fresh ids for whatever remains.
        self.cases = new_values
            .into_iter()
            .zip(assigned)
            .map(|(value, id)| {
                let id = id.unwrap_or_else(|| {
                    let fresh = self.next_case_id;
                    self.next_case_id += 1;
                    fresh
                });
                SwitchCase {
                    id: Some(id),
                    value,
                }
            })
            .collect();
        Ok(())
    }

    /// Convert the **stored** case values in place to a new selector type, ids
    /// untouched (`doc/design_switch_node.md` §Selector-type change). This is
    /// the only place values cross the Int/String boundary; `merge_cases`
    /// itself never compares across types (plain same-type equality), so a
    /// flip-and-edit runs this first, then `merge_cases` on the now-same-type
    /// values.
    ///
    /// - Int → String: stringify (`5` → `"5"`), always succeeds.
    /// - String → Int: parse each; **any failure rejects the whole edit**. Parse
    ///   success alone is not enough — distinct strings can parse to the same
    ///   int (`"5"` / `"05"`), so the converted list is re-checked for
    ///   duplicates and a collision **also rejects the whole edit**, otherwise
    ///   the flip would smuggle in the duplicate state every other edit path
    ///   rejects. Either rejection leaves the node unchanged.
    /// - Same type: no-op.
    pub fn convert_selector_type(&mut self, new_type: &DataType) -> Result<(), String> {
        match (&self.selector_type, new_type) {
            (DataType::Int, DataType::Int) | (DataType::String, DataType::String) => {
                return Ok(());
            }
            (DataType::Int, DataType::String) => {
                for case in self.cases.iter_mut() {
                    if let SwitchCaseValue::Int(i) = &case.value {
                        case.value = SwitchCaseValue::String(i.to_string());
                    }
                }
            }
            (DataType::String, DataType::Int) => {
                // Parse everything into a scratch vec first so a failure
                // partway through leaves the node untouched.
                let mut converted = Vec::with_capacity(self.cases.len());
                for case in &self.cases {
                    if let SwitchCaseValue::String(s) = &case.value {
                        let parsed = s.parse::<i32>().map_err(|_| {
                            format!(
                                "cannot convert case value {} to Int",
                                case.value.to_display_string()
                            )
                        })?;
                        converted.push(parsed);
                    }
                }
                // Distinct strings can parse to the same int — reject the flip
                // rather than smuggle in a duplicate.
                for i in 0..converted.len() {
                    for j in (i + 1)..converted.len() {
                        if converted[i] == converted[j] {
                            return Err(format!(
                                "converting to Int makes case values collide at {}",
                                converted[i]
                            ));
                        }
                    }
                }
                for (case, v) in self.cases.iter_mut().zip(converted) {
                    case.value = SwitchCaseValue::Int(v);
                }
            }
            _ => return Err("selector_type must be Int or String".to_string()),
        }
        self.selector_type = new_type.clone();
        Ok(())
    }

    /// Restore every invariant the setters enforce after a raw deserialize of a
    /// possibly hand-authored `.cnnd` blob (`doc/design_switch_node.md`
    /// §Serialization, healing). Idempotent — a normally-saved switch (valid
    /// selector, matching value variants, present distinct ids, `next_case_id`
    /// past the max) passes through unchanged, so a round-trip of a good file is
    /// a no-op.
    ///
    /// 1. A `selector_type` outside {Int, String} resets to `Int` (the default
    ///    domain).
    /// 2. Each case value is converted into the selector domain via the
    ///    canonical string form (Int↔String); a value that can't convert is
    ///    **dropped**, and a value that duplicates an earlier survivor is
    ///    **dropped** too (distinct strings can parse to the same int).
    /// 3. If no case survives, the list resets to the default two cases in the
    ///    selector domain — never empty.
    /// 4. `next_case_id` is bumped past every surviving id and any case still
    ///    lacking an id (or one minted for a reset case) is assigned a fresh id,
    ///    so wire matching never silently degrades to name/positional.
    pub fn heal(&mut self) {
        // 1. Selector domain.
        if !matches!(self.selector_type, DataType::Int | DataType::String) {
            self.selector_type = DataType::Int;
        }

        // 2. Convert values into the selector domain, dropping the unconvertible
        //    and later duplicates. Ids ride along on the survivors.
        let mut healed: Vec<SwitchCase> = Vec::with_capacity(self.cases.len());
        let mut seen: Vec<SwitchCaseValue> = Vec::new();
        for case in std::mem::take(&mut self.cases) {
            let converted = match (&self.selector_type, &case.value) {
                (DataType::Int, SwitchCaseValue::Int(_))
                | (DataType::String, SwitchCaseValue::String(_)) => Some(case.value.clone()),
                (DataType::Int, SwitchCaseValue::String(s)) => {
                    s.parse::<i32>().ok().map(SwitchCaseValue::Int)
                }
                (DataType::String, SwitchCaseValue::Int(i)) => {
                    Some(SwitchCaseValue::String(i.to_string()))
                }
                // selector_type was healed to Int|String above, so this arm is
                // unreachable in practice.
                _ => None,
            };
            if let Some(v) = converted
                && !seen.contains(&v)
            {
                seen.push(v.clone());
                healed.push(SwitchCase {
                    id: case.id,
                    value: v,
                });
            }
        }
        self.cases = healed;

        // 3. Never empty: reset to the default two cases in the selector domain.
        if self.cases.is_empty() {
            self.cases = match self.selector_type {
                DataType::String => vec![
                    SwitchCase {
                        id: None,
                        value: SwitchCaseValue::String("0".to_string()),
                    },
                    SwitchCase {
                        id: None,
                        value: SwitchCaseValue::String("1".to_string()),
                    },
                ],
                _ => vec![
                    SwitchCase {
                        id: None,
                        value: SwitchCaseValue::Int(0),
                    },
                    SwitchCase {
                        id: None,
                        value: SwitchCaseValue::Int(1),
                    },
                ],
            };
        }

        // 4. Heal id state: bump the counter past every existing id, then mint
        //    fresh ids for any id-less case (from the reset, or a hand-authored
        //    id: None). Computed over the final case list so a dropped id never
        //    leaves the counter behind a survivor.
        let max_id = self.cases.iter().filter_map(|c| c.id).max().unwrap_or(0);
        if self.next_case_id <= max_id {
            self.next_case_id = max_id + 1;
        }
        for case in &mut self.cases {
            if case.id.is_none() {
                case.id = Some(self.next_case_id);
                self.next_case_id += 1;
            }
        }
    }
}

impl NodeData for SwitchData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();

        // Built from scratch (like `zip_with`/`expr`, not by indexing base
        // parameters — the count varies): the `value` selector pin, then one
        // pin per case (carrying the case's hidden stable id), then the fixed
        // `default` pin.
        let case_names = self.derived_case_pin_names();
        let mut parameters = Vec::with_capacity(self.cases.len() + 2);
        parameters.push(Parameter {
            id: None,
            name: "value".to_string(),
            data_type: self.selector_type.clone(),
        });
        for (case, name) in self.cases.iter().zip(case_names) {
            parameters.push(Parameter {
                id: case.id,
                name,
                data_type: self.case_pin_type(),
            });
        }
        parameters.push(Parameter {
            id: None,
            name: "default".to_string(),
            data_type: self.case_pin_type(),
        });
        custom.parameters = parameters;

        custom.output_pins = OutputPinDefinition::single_fixed(self.value_type.clone());
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
        // 1. Evaluate the selector (pin 0). Unwired → inert (None); error →
        // propagate.
        let sel_val = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        match &sel_val {
            NetworkResult::None => return EvalOutput::single(NetworkResult::None),
            NetworkResult::Error(_) => return EvalOutput::single(sel_val),
            _ => {}
        }

        // 2. Find the case whose literal equals the selector value. A `Float`
        // wired to an Int selector was already truncated at the wire, so the
        // returned variant matches `selector_type`; any other variant is a
        // localized error (mirrors `if.cond`).
        let matched_index = match sel_val {
            NetworkResult::Int(v) => self
                .cases
                .iter()
                .position(|c| c.value == SwitchCaseValue::Int(v)),
            NetworkResult::String(ref s) => self
                .cases
                .iter()
                .position(|c| c.value == SwitchCaseValue::String(s.clone())),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "switch.value: expected {}, got {:?}",
                    self.selector_type,
                    other.infer_data_type()
                )));
            }
        };

        // 3. Lazily pull *only* the taken pin: `1 + case_index` on a match, the
        // `default` pin (`1 + cases.len()`) otherwise. An unwired taken pin
        // yields `None`, which flows through unchanged — exactly `if`'s
        // contract. First match wins for the impossible hand-authored duplicate
        // state (documented, not policed).
        let pin_index = match matched_index {
            Some(i) => 1 + i,
            None => 1 + self.cases.len(),
        };
        let branch_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, pin_index);
        EvalOutput::single(branch_val)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        Some(format!(
            "{} → {} ({} cases)",
            self.selector_type,
            self.value_type,
            self.cases.len()
        ))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let cases = self
            .cases
            .iter()
            .map(|c| match &c.value {
                SwitchCaseValue::Int(i) => TextValue::Int(*i),
                SwitchCaseValue::String(s) => TextValue::String(s.clone()),
            })
            .collect();
        vec![
            (
                "selector_type".to_string(),
                TextValue::DataType(self.selector_type.clone()),
            ),
            (
                "value_type".to_string(),
                TextValue::DataType(self.value_type.clone()),
            ),
            ("cases".to_string(), TextValue::Array(cases)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("selector_type") {
            let dt = v
                .as_data_type()
                .ok_or_else(|| "selector_type must be a DataType".to_string())?;
            if !matches!(dt, DataType::Int | DataType::String) {
                return Err("selector_type must be Int or String".to_string());
            }
            // Convert the **stored** case values into the new domain *before* any
            // `cases` merge below (`doc/design_switch_node.md` §Selector-type
            // change). Setting `selector_type` directly instead would leave the
            // old-domain stored values un-matchable against the new-domain text
            // values (String "1" ≠ Int 1 under same-type equality), silently
            // degrading every id to the positional fallback.
            self.convert_selector_type(dt)?;
        }
        if let Some(v) = props.get("value_type") {
            self.value_type = v
                .as_data_type()
                .ok_or_else(|| "value_type must be a DataType".to_string())?
                .clone();
        }
        if let Some(v) = props.get("cases") {
            let arr = v
                .as_array()
                .ok_or_else(|| "cases must be an array of case values".to_string())?;
            let mut values = Vec::with_capacity(arr.len());
            for item in arr {
                values.push(coerce_case_value(item, &self.selector_type)?);
            }
            self.merge_cases(values)?;
        }
        Ok(())
    }

    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        _direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        // Mirror `if`: the case pins / `default` / output are all plain `T`, so
        // in both drag directions the useful adaptation is `value_type = T`.
        // Reject types that can't be a clean concrete value pin (abstract phase
        // supertypes, `Iter[T]`). A dragged Int also matches the static `value`
        // selector pin, so `switch` still surfaces for an integer drag; a
        // String drag adapts the *value* side (users flip `selector_type`
        // manually when the string is meant as the selector).
        if source_type.is_abstract() || matches!(source_type, DataType::Iterator(_)) {
            return None;
        }
        Some(Box::new(SwitchData {
            value_type: source_type.clone(),
            ..SwitchData::default()
        }))
    }
}

/// Coerce one text-format array element to a `SwitchCaseValue` in the selector
/// domain. For an Int selector a whole-number parse is required — a fractional
/// `Float` is rejected rather than silently truncated.
fn coerce_case_value(
    item: &TextValue,
    selector_type: &DataType,
) -> Result<SwitchCaseValue, String> {
    match selector_type {
        DataType::Int => match item {
            TextValue::Int(i) => Ok(SwitchCaseValue::Int(*i)),
            TextValue::Float(f) if f.fract() == 0.0 => Ok(SwitchCaseValue::Int(*f as i32)),
            TextValue::Float(_) => Err("Int case values must be whole numbers".to_string()),
            other => Err(format!(
                "Int case value expected, got {:?}",
                other.inferred_data_type()
            )),
        },
        DataType::String => item
            .as_string()
            .map(|s| SwitchCaseValue::String(s.to_string()))
            .ok_or_else(|| "String case value expected".to_string()),
        _ => Err("selector_type must be Int or String".to_string()),
    }
}

/// Loader that heals persisted case-id / selector-domain state (missing/zero
/// `next_case_id`, id-less cases, value variants disagreeing with
/// `selector_type`, a `selector_type` outside {Int, String}, post-conversion
/// duplicates, and an all-dropped case list) after the generic deserialize.
/// Mirrors `zip_with_node_data_loader`.
fn switch_node_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: SwitchData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    data.heal();
    Ok(Box::new(data))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "switch".to_string(),
        description:
            "Selects a value by matching a selector against a list of literal cases (an n-way \
             generalization of `if`, also known as select / match / multiplex). The `value` \
             selector pin is compared against the user-edited case literals; the matching case's \
             pin — or the fixed `default` pin when nothing matches — is the one and only branch \
             evaluated (the others' inputs are never computed). The selector type is Int or \
             String; the case / default / output value type is selectable and can be any concrete \
             type, including structural types like Crystal, Molecule, or Geometry. All pins are \
             optional: an unwired selector makes the node inert, and an unwired taken branch \
             produces no value."
                .to_string(),
        summary: Some("Select a value by matching a selector against literal cases".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // Mirrors `SwitchData::default()` (Int selector, Float value, cases
        // 0 and 1 with ids 1/2) so the data-driven custom type equals this base
        // layout from registration.
        parameters: vec![
            Parameter {
                id: None,
                name: "value".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: Some(1),
                name: "case_0".to_string(),
                data_type: DataType::Float,
            },
            Parameter {
                id: Some(2),
                name: "case_1".to_string(),
                data_type: DataType::Float,
            },
            Parameter {
                id: None,
                name: "default".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Float),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(SwitchData::default()),
        node_data_saver: generic_node_data_saver::<SwitchData>,
        node_data_loader: switch_node_data_loader,
    }
}
