//! `apply_style` — per-atom visual styling driven by tag/element rules.
//!
//! Phases 2 + 4 of `doc/design_style_rules.md`. A `HasAtoms`-polymorphic,
//! metadata-only pass-through in the `freeze`/`xray`/`tag` family. It takes a
//! `rules: Array[Record(Named("StyleRule"))]` value and writes per-atom color,
//! alpha, and render-style overrides onto the decorator (runtime-only display
//! state, never serialized, dropped by structure-rebuilding nodes — so place
//! `apply_style` late in the chain).
//!
//! The node has **no stored properties** (decision 1: rules are wire-only), so
//! there is no property editor, no node-data API, and no text-format surface.
//! Rules are authored upstream (`record_construct` + `sequence`).
//!
//! Matching is ordered, per-property last-writer-wins: rules apply in array
//! order and each matching rule overrides only the properties it sets. A rule
//! matches an atom iff every *present* selector matches (`element` vs.
//! `atomic_number`, `tag` via the structure's tag table); both selectors
//! absent ⇒ the rule matches every atom.

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::AtomRenderStyle;
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::atom_op::map_atomic;
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
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// `apply_style` carries no stored state — its rules live entirely on the wire
/// (design decision 1). The empty struct keeps the standard node-data lifecycle
/// (creator / saver / loader) with nothing to persist.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApplyStyleData {}

/// A single parsed style rule. Selectors (`element`, `tag`) are pre-validated;
/// properties (`color`, `alpha`) are pre-clamped-at-write via the accessors.
#[derive(Debug, Clone)]
struct StyleRule {
    /// Selector: atomic number (fits `i16`). `None` ⇒ no element constraint.
    element: Option<i16>,
    /// Selector: trimmed, non-empty tag name. `None` ⇒ no tag constraint.
    tag: Option<String>,
    /// Property: 0–1 RGB albedo override. `None` ⇒ leave color alone.
    color: Option<Vec3>,
    /// Property: 0–1 display alpha. `None` ⇒ leave alpha alone. `1.0` restores
    /// full opacity (removes the entry) — see `set_atom_alpha`.
    alpha: Option<f32>,
    /// Property: per-atom render-style override (Phase 4). The outer `Option`
    /// distinguishes "field absent ⇒ leave the atom's render style alone"
    /// (`None`) from "field present" (`Some`); the inner `Option` then chooses
    /// **set** the override (`Some(style)`, from `"ball_and_stick"` /
    /// `"space_filling"`) vs. **clear** it (`None`, from `"default"` — restores
    /// the global preference).
    render_style: Option<Option<AtomRenderStyle>>,
}

impl NodeData for ApplyStyleData {
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
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = input_val {
            return EvalOutput::single(input_val);
        }

        // Optional `rules` pin (param index 1). Disconnected → pass the input
        // through unchanged (a no-op, consistent with an empty array — the
        // network stays wireable while rules are under construction).
        let rules_input =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let rules = match rules_input {
            NetworkResult::None => return EvalOutput::single(input_val),
            NetworkResult::Error(_) => return EvalOutput::single(rules_input),
            NetworkResult::Array(items) => match parse_style_rules(items) {
                Ok(r) => r,
                Err(e) => return EvalOutput::single(NetworkResult::Error(e)),
            },
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "apply_style.rules: expected Array[Record], got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        let output = map_atomic(input_val, move |mut structure| {
            // Snapshot the match inputs once. Writing color/alpha only touches
            // the decorator, so `atomic_number` / `tag_bits` are stable across
            // the passes below.
            let atoms: Vec<(u32, i16, u32)> = structure
                .iter_atoms()
                .map(|(id, a)| (*id, a.atomic_number, a.tag_bits))
                .collect();

            for rule in &rules {
                // Precompute the tag-axis test once: `None` selector always
                // passes; a resolvable name becomes a bit mask; a name absent
                // from this structure's table matches nothing (skip the rule).
                let tag_bit = match &rule.tag {
                    None => TagMatch::Any,
                    Some(name) => match structure.tag_index(name) {
                        Some(idx) => TagMatch::Bit(1u32 << idx),
                        None => continue,
                    },
                };

                for &(id, atomic_number, tag_bits) in &atoms {
                    if let Some(e) = rule.element
                        && atomic_number != e
                    {
                        continue;
                    }
                    if let TagMatch::Bit(bit) = tag_bit
                        && tag_bits & bit == 0
                    {
                        continue;
                    }
                    // Matched: write each present property (accessors clamp).
                    if let Some(color) = rule.color {
                        structure.set_atom_color(id, color);
                    }
                    if let Some(alpha) = rule.alpha {
                        structure.set_atom_alpha(id, alpha);
                    }
                    // `render_style`: `Some(style)` sets the override,
                    // `None` ("default") clears it back to the global mode.
                    if let Some(render_style) = rule.render_style {
                        match render_style {
                            Some(style) => structure.set_atom_render_style(id, style),
                            None => structure.clear_atom_render_style(id),
                        }
                    }
                }
            }
            structure
        });

        EvalOutput::single(output)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        // No stored properties → nothing to summarize (rules live on the wire).
        None
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m.insert("rules".to_string(), (false, None)); // optional
        m
    }
}

/// The tag-axis outcome precomputed once per rule against the styled structure.
enum TagMatch {
    /// No tag selector — the tag axis always passes.
    Any,
    /// The selector resolved to this per-structure bit mask.
    Bit(u32),
}

/// Parse a runtime `Array[Record(StyleRule)]` value into the validated rule
/// list. Defensive about absent fields (matches materialize's region parsing):
/// `extract_record_field(name)` returning `None` *or* `Some(NetworkResult::None)`
/// both mean "unset". Any invalid rule → `Err(String)` naming the rule index
/// and problem; nothing is partially applied.
fn parse_style_rules(items: Vec<NetworkResult>) -> Result<Vec<StyleRule>, String> {
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        // `element` is `Optional[Int]` selecting an atomic number. Must fit
        // `i16` (including the param-element / debug numbers ≥ 1000); a number
        // no atom carries simply matches nothing, which is not an error.
        let element = match item.extract_record_field("element") {
            None | Some(NetworkResult::None) => None,
            Some(NetworkResult::Int(v)) => {
                let v = *v;
                if v < i16::MIN as i32 || v > i16::MAX as i32 {
                    return Err(format!(
                        "apply_style.rules[{}].element: atomic number {} out of range \
                         (must fit a 16-bit integer)",
                        i, v
                    ));
                }
                Some(v as i16)
            }
            Some(other) => {
                return Err(format!(
                    "apply_style.rules[{}].element: expected Int, got {:?}",
                    i,
                    other.infer_data_type()
                ));
            }
        };

        // `tag` is `Optional[String]`. Trimmed; empty-after-trim is certainly a
        // mistake (an empty tag name can never exist) → error. A name absent
        // from the structure's table matches nothing without error.
        let tag = match item.extract_record_field("tag") {
            None | Some(NetworkResult::None) => None,
            Some(NetworkResult::String(s)) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    return Err(format!("apply_style.rules[{}].tag: tag name is empty", i));
                }
                Some(trimmed.to_string())
            }
            Some(other) => {
                return Err(format!(
                    "apply_style.rules[{}].tag: expected String, got {:?}",
                    i,
                    other.infer_data_type()
                ));
            }
        };

        // `color` is `Optional[Vec3]`, 0–1 RGB. Clamping happens at write
        // (`set_atom_color`), so store the raw f32 vector here.
        let color = match item.extract_record_field("color") {
            None | Some(NetworkResult::None) => None,
            Some(NetworkResult::Vec3(v)) => Some(v.as_vec3()),
            Some(other) => {
                return Err(format!(
                    "apply_style.rules[{}].color: expected Vec3, got {:?}",
                    i,
                    other.infer_data_type()
                ));
            }
        };

        // `alpha` is `Optional[Float]`, 0–1. `set_atom_alpha` clamps low and
        // treats ≥ 1.0 as "restore opacity" (removes the entry).
        let alpha = match item.extract_record_field("alpha") {
            None | Some(NetworkResult::None) => None,
            Some(NetworkResult::Float(f)) => Some(*f as f32),
            Some(other) => {
                return Err(format!(
                    "apply_style.rules[{}].alpha: expected Float, got {:?}",
                    i,
                    other.infer_data_type()
                ));
            }
        };

        // `render_style` is `Optional[String]`, a three-value enum:
        // `"ball_and_stick"` / `"space_filling"` set the per-atom override;
        // `"default"` clears it (restores the global preference). Any other
        // string → localized error naming the value. A string enum because the
        // type system has no enum `DataType`.
        let render_style = match item.extract_record_field("render_style") {
            None | Some(NetworkResult::None) => None,
            Some(NetworkResult::String(s)) => match s.trim() {
                "ball_and_stick" => Some(Some(AtomRenderStyle::BallAndStick)),
                "space_filling" => Some(Some(AtomRenderStyle::SpaceFilling)),
                "default" => Some(None),
                other => {
                    return Err(format!(
                        "apply_style.rules[{}].render_style: expected \"ball_and_stick\", \
                         \"space_filling\", or \"default\", got \"{}\"",
                        i, other
                    ));
                }
            },
            Some(other) => {
                return Err(format!(
                    "apply_style.rules[{}].render_style: expected String, got {:?}",
                    i,
                    other.infer_data_type()
                ));
            }
        };

        out.push(StyleRule {
            element,
            tag,
            color,
            alpha,
            render_style,
        });
    }
    Ok(out)
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "apply_style".to_string(),
        description: "Applies per-atom visual styling (color, transparency, render style) driven \
                      by a list of \
                      style rules. Each rule selects atoms by element and/or tag and sets the \
                      properties it specifies; rules apply in order, last writer wins per \
                      property. Build rules with record_construct (schema StyleRule) and collect \
                      them with a sequence node into the `rules` pin. Like xray, styling is a \
                      metadata-only pass-through — place apply_style late in the chain, after any \
                      structure-rebuilding node (materialize, lattice fill), which drops it."
            .to_string(),
        summary: Some("Style atoms by tag/element".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "rules".to_string(),
                data_type: DataType::Array(Box::new(DataType::Record(RecordType::Named(
                    "StyleRule".to_string(),
                )))),
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ApplyStyleData::default()),
        node_data_saver: generic_node_data_saver::<ApplyStyleData>,
        node_data_loader: generic_node_data_loader::<ApplyStyleData>,
    }
}
