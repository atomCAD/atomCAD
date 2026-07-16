//! `xray` — per-region semi-transparent atom display node.
//!
//! Phase 2 of `doc/design_xray_node.md`. A `HasAtoms`-polymorphic,
//! metadata-only pass-through (like `freeze`/`unfreeze`) that records a
//! display alpha on every atom inside an optional `region: Blueprint` volume
//! (all atoms when disconnected). `alpha == 1.0` removes the recording
//! (restores opacity), so chained `xray` nodes compose last-writer-wins.
//! The alpha is consumed by the impostor renderer (design Phases 3–5); in
//! `TriangleMesh` mode atoms render opaque (documented limitation).

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::lattice_fill::DEFAULT_REGION_MARGIN;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::atom_op::map_atomic_in_region;
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
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

fn default_alpha() -> f64 {
    0.5
}

fn default_fade_depth() -> f64 {
    0.0
}

/// Alpha of an atom at `depth` Å below the blueprint surface.
///
/// `fade_depth <= 0` disables the ramp: every atom gets `surface_alpha` (the
/// pre-ramp behavior). Otherwise alpha eases from `surface_alpha` at the
/// surface **down to 0 (fully transparent)** at `fade_depth`, via smoothstep.
///
/// The direction is the whole point: deep atoms contribute *less*, so they
/// stop accumulating into the volumetric fog that makes a thick ghosted block
/// impossible to see into. This is a soft version of the display's depth
/// culling — which is exactly the sharp-Heaviside member of the same family.
/// The ramp *reaches* 0 at a stated depth rather than decaying asymptotically,
/// so that atoms are already fully invisible by the time the cull threshold
/// drops them, leaving no visible discontinuity (see
/// `doc/design_xray_node.md` "Depth falloff").
pub fn depth_faded_alpha(surface_alpha: f64, fade_depth: f64, depth: f32) -> f32 {
    // Non-finite is folded into "off" alongside <= 0: a NaN depth (wireable
    // from an `expr`) would otherwise propagate through the ramp into the
    // stored alpha, and an infinitely deep fade leaves every atom at the
    // surface alpha anyway — which is what "off" already means.
    if !fade_depth.is_finite() || fade_depth <= 0.0 {
        return surface_alpha as f32;
    }
    let t = (depth as f64 / fade_depth).clamp(0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    (surface_alpha * (1.0 - s)) as f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XrayData {
    /// Display alpha applied to in-region atoms. Wired `alpha` pin overrides
    /// this stored value; clamped to `[0.0, 1.0]` at eval. `1.0` restores
    /// full opacity (removes the per-atom recording).
    ///
    /// With `fade_depth > 0` this is the alpha *at the surface* — the most
    /// opaque, shallow end of the depth ramp.
    #[serde(default = "default_alpha")]
    pub alpha: f64,
    /// Depth (Å) at which atoms have faded to fully transparent. `0` (the
    /// serde default, so pre-ramp `.cnnd` files keep their exact behavior)
    /// disables the ramp and applies `alpha` uniformly. Wired `fade_depth` pin
    /// overrides this.
    #[serde(default = "default_fade_depth")]
    pub fade_depth: f64,
}

impl NodeData for XrayData {
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

        // Alpha: wired pin 1 overrides the stored property; clamp to [0, 1].
        let alpha = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.alpha,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value.clamp(0.0, 1.0),
            Err(error) => return EvalOutput::single(error),
        };

        // Optional `region` pin (param index 2). Disconnected → record the
        // alpha on every atom. Connected → only in-region atoms.
        let region_input =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 2);
        let region_geo = match region_input {
            NetworkResult::None => None,
            NetworkResult::Error(_) => return EvalOutput::single(region_input),
            NetworkResult::Blueprint(bp) => Some(bp.geo_tree_root),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "xray.region: expected Blueprint, got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        // Depth ramp: wired pin 3 overrides the stored property. Non-positive
        // (the default) = no ramp, `alpha` applied uniformly.
        let fade_depth = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            self.fade_depth,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let output = map_atomic_in_region(
            input_val,
            region_geo.as_ref(),
            DEFAULT_REGION_MARGIN,
            |mut structure, in_region| {
                // Read each atom's depth in the same pass that collects ids —
                // `set_atom_alpha` needs `&mut structure`, so the immutable
                // iteration has to finish first.
                let entries: Vec<(u32, f32)> = structure
                    .iter_atoms()
                    .filter(|(atom_id, _)| in_region(**atom_id))
                    .map(|(atom_id, atom)| (*atom_id, atom.in_crystal_depth))
                    .collect();
                for (id, depth) in entries {
                    structure.set_atom_alpha(id, depth_faded_alpha(alpha, fade_depth, depth));
                }
                structure
            },
        );
        EvalOutput::single(output)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        // Each half of the subtitle is dropped when its pin is wired (the wired
        // value wins at eval, so showing the stored one would be a lie).
        let alpha_part = if connected_input_pins.contains("alpha") {
            None
        } else {
            Some(format!("α = {:.2}", self.alpha))
        };
        let ramp_part = if connected_input_pins.contains("fade_depth") {
            None
        } else if self.fade_depth > 0.0 {
            Some(format!("→ 0 @ {:.3} Å", self.fade_depth))
        } else {
            None
        };
        match (alpha_part, ramp_part) {
            (Some(a), Some(r)) => Some(format!("{a} {r}")),
            (Some(a), None) => Some(a),
            (None, Some(r)) => Some(r),
            (None, None) => None,
        }
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m.insert("alpha".to_string(), (false, None)); // optional
        m.insert("region".to_string(), (false, None)); // optional
        m.insert("fade_depth".to_string(), (false, None)); // optional
        m
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("alpha".to_string(), TextValue::Float(self.alpha)),
            ("fade_depth".to_string(), TextValue::Float(self.fade_depth)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("alpha") {
            self.alpha = v.as_float().ok_or("alpha must be a float")?;
        }
        if let Some(v) = props.get("fade_depth") {
            self.fade_depth = v.as_float().ok_or("fade_depth must be a float")?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "xray".to_string(),
        description: "Makes atoms semi-transparent in the viewport so internal features \
                      show through. With a region connected, only atoms inside the region \
                      volume are affected; otherwise all atoms are. The alpha (0 = invisible, \
                      1 = opaque) comes from the wired `alpha` pin or the stored property; \
                      `1.0` restores full opacity, so chained xray nodes compose \
                      last-writer-wins. `fade_depth` (Å) turns the uniform alpha into a \
                      depth ramp: atoms fade from `alpha` at the crystal surface to fully \
                      transparent that far below it, leaving a thin visible shell instead of \
                      a deep fog that blocks the view into the interior; `0` disables the \
                      ramp. It is a soft version of the display's depth culling — raise the \
                      cull depth past `fade_depth` so atoms vanish by fading rather than at \
                      a hard threshold. The ramp only moves atoms that carry a crystal depth \
                      (lattice-filled ones). Transparency renders in impostor atomic \
                      rendering mode only; in triangle-mesh mode atoms stay opaque."
            .to_string(),
        summary: Some("Ghost atoms semi-transparent".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "alpha".to_string(),
                data_type: DataType::Float,
            },
            Parameter {
                id: None,
                name: "region".to_string(),
                data_type: DataType::Blueprint,
            },
            // Appended AFTER `region` on purpose. `Node.arguments` is a
            // positional `Vec<Argument>` with no pin names in the `.cnnd`, and
            // `repair_network_arguments` only grows/truncates at the tail — so
            // inserting this pin at index 2 would silently reinterpret every
            // existing `region` wire as a `fade_depth` wire. Appending keeps
            // indices 0..=2 stable and needs no migration, at the cost of
            // bending the "region is the last pin" convention.
            Parameter {
                id: None,
                name: "fade_depth".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(XrayData {
                alpha: 0.5,
                fade_depth: 0.0,
            })
        },
        node_data_saver: generic_node_data_saver::<XrayData>,
        node_data_loader: generic_node_data_loader::<XrayData>,
    }
}
