//! `isosurface` — turns a `ScalarField` into a drawable surface specification.
//!
//! See `doc/design_isosurface_node.md`. The node's whole job is to package a
//! field, an isolevel and the paint into an [`IsosurfaceData`]; **it does not
//! extract a mesh.** Marching cubes runs one stage later, in the display
//! conversion (`generate_isosurface_output` →
//! `atomcad_display::isosurface::extract_isosurface`), which is the only place
//! that can see the extraction preferences.
//!
//! The governing rule that split is an instance of: **semantic parameters live
//! in the value, quality parameters live in preferences.** The isolevel is
//! semantic and is node data; the extraction resolution is a quality knob and
//! would be wrong to bake into the `.cnnd`, exactly as a `sphere` node's
//! `radius` is node data while `samples_per_unit_cell` is not.
//!
//! # Pins
//!
//! `level` gets a pin *as well as* a property, following `free_sphere` — the
//! wired value wins. The rule for later additions: **parameters that change the
//! geometry get pins; parameters that change only the paint stay properties.**
//!
//! # What this node deliberately does not report
//!
//! Two conditions that would ideally warn are silent, because the evaluation
//! side has exactly one error channel (`NetworkEvaluationContext::node_errors`)
//! and it carries no severity — everything in it renders red:
//!
//! - a `level` above the field's value range extracts an empty surface, which
//!   looks the same as a broken import. Mitigated by
//!   `NetworkResult::to_display_string` for `ScalarField` — **not**
//!   `to_detailed_string`, which the design doc originally named: the pin
//!   tooltip renders `NodeView::output_pin_strings`, built from the *display*
//!   string, while the detailed one is reachable only from the CLI/AI
//!   `evaluate_node --verbose` path. And it is the upstream `field` **output**
//!   pin that carries it — input pins have no value readout at all.
//! - a color field whose box is smaller than the surface paints the overhang
//!   with whatever `0.0` maps to. Documented in the reference guide.
//!
//! Both become warnings for free if an evaluation-time severity channel is ever
//! added; that is a change to the error subsystem, not to this node.

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
use crate::text_format::TextValue;
use atomcad_crystolecule::field::{Colormap, IsosurfaceColoring, IsosurfaceData};
use atomcad_util::serialization_utils::dvec3_serializer;
use glam::f64::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default isolevel. This is the **molecular-orbital amplitude** convention,
/// which is an order of magnitude too large for an electron density (`0.002`).
/// Accepted rather than fixed: a field's meaning is not recoverable from its
/// numbers, so no single default serves both. What tells a user which way to
/// move is the upstream `field` **output** pin's hover readout, which carries
/// the value range and the source's own description.
pub const DEFAULT_LEVEL: f64 = 0.02;
/// 0-1 RGB, matching the convention `apply_style` uses.
pub const DEFAULT_POSITIVE_COLOR: DVec3 = DVec3::new(0.20, 0.40, 0.90);
pub const DEFAULT_NEGATIVE_COLOR: DVec3 = DVec3::new(0.90, 0.30, 0.25);
/// Not `0.5`, because the two-pass transparent draw composites two layers:
/// `1 - (1 - 0.4)^2 = 0.64`, where `0.5` would give `0.75`.
pub const DEFAULT_ALPHA: f64 = 0.4;
pub const DEFAULT_COLOR_MIN: f64 = -0.05;
pub const DEFAULT_COLOR_MAX: f64 = 0.05;

/// Every property is stored at `TextValue::Float` precision (`f64`/`DVec3`) and
/// **narrows selectively at `eval`**: the ones the *renderer* consumes become
/// `f32`/`Vec3`, while the ones compared against *field values* stay `f64`.
/// `ScalarField::sample` returns `f64`, so narrowing `level` or the colormap
/// domain would introduce a rounding difference between a threshold and the
/// data it is compared against.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsosurfaceNodeData {
    /// Level **magnitude**. Extraction runs at `+level` and `-level`, so the
    /// sign is not a user choice and `<= 0` is an evaluation error.
    pub level: f64,
    #[serde(with = "dvec3_serializer")]
    pub positive_color: DVec3,
    #[serde(with = "dvec3_serializer")]
    pub negative_color: DVec3,
    /// `>= 1.0` takes the opaque fast path in the scene tessellator.
    pub alpha: f64,
    /// Only consulted when the `color_field` pin is wired.
    #[serde(default)]
    pub colormap: Colormap,
    #[serde(default = "default_color_min")]
    pub color_min: f64,
    #[serde(default = "default_color_max")]
    pub color_max: f64,
}

fn default_color_min() -> f64 {
    DEFAULT_COLOR_MIN
}

fn default_color_max() -> f64 {
    DEFAULT_COLOR_MAX
}

impl Default for IsosurfaceNodeData {
    fn default() -> Self {
        Self {
            level: DEFAULT_LEVEL,
            positive_color: DEFAULT_POSITIVE_COLOR,
            negative_color: DEFAULT_NEGATIVE_COLOR,
            alpha: DEFAULT_ALPHA,
            colormap: Colormap::default(),
            color_min: DEFAULT_COLOR_MIN,
            color_max: DEFAULT_COLOR_MAX,
        }
    }
}

/// Text-format spelling of a [`Colormap`]. Kept separate from the enum's own
/// `Serialize` so the `.cnnd` JSON form and the text form can diverge without
/// one silently changing the other.
fn colormap_to_text(colormap: Colormap) -> &'static str {
    match colormap {
        Colormap::BlueWhiteRed => "blue_white_red",
    }
}

fn colormap_from_text(text: &str) -> Result<Colormap, String> {
    match text {
        "blue_white_red" => Ok(Colormap::BlueWhiteRed),
        other => Err(format!(
            "unknown colormap '{}' (expected one of: blue_white_red)",
            other
        )),
    }
}

impl NodeData for IsosurfaceNodeData {
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
                "isosurface: field input is not a ScalarField".to_string(),
            ));
        };

        // Pin 2, not pin 1: pin 1 is `color_field`. The pin order is the
        // design's and is fixed — inserting a pin ahead of `level` later would
        // silently re-target every wire in every saved project.
        let level = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.level,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // NaN is spelled out rather than left to `!(level > 0.0)`: a wired
        // `expr` can produce one, and it must be rejected here instead of
        // turning every comparison in the extractor false and yielding a
        // silently empty mesh.
        if level.is_nan() || level <= 0.0 {
            return EvalOutput::single(NetworkResult::Error(format!(
                "isosurface: level must be greater than 0 (got {}); \
                 it is a magnitude — both signs are always drawn",
                level
            )));
        }

        EvalOutput::single(NetworkResult::Isosurface(IsosurfaceData {
            field,
            level,
            // Colouring by a second field is not wired up yet; the pin and the
            // stored colormap domain exist so that the `.cnnd` written today is
            // already the right shape.
            coloring: IsosurfaceColoring::Phase {
                positive: self.positive_color.as_vec3(),
                negative: self.negative_color.as_vec3(),
            },
            alpha: self.alpha as f32,
        }))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if connected_input_pins.contains("level") {
            None
        } else {
            Some(format!("level: {:.4}", self.level))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("level".to_string(), TextValue::Float(self.level)),
            (
                "positive_color".to_string(),
                TextValue::Vec3(self.positive_color),
            ),
            (
                "negative_color".to_string(),
                TextValue::Vec3(self.negative_color),
            ),
            ("alpha".to_string(), TextValue::Float(self.alpha)),
            (
                "colormap".to_string(),
                TextValue::String(colormap_to_text(self.colormap).to_string()),
            ),
            ("color_min".to_string(), TextValue::Float(self.color_min)),
            ("color_max".to_string(), TextValue::Float(self.color_max)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("level") {
            self.level = v
                .as_float()
                .ok_or_else(|| "level must be a float".to_string())?;
        }
        if let Some(v) = props.get("positive_color") {
            self.positive_color = v
                .as_vec3()
                .ok_or_else(|| "positive_color must be a Vec3".to_string())?;
        }
        if let Some(v) = props.get("negative_color") {
            self.negative_color = v
                .as_vec3()
                .ok_or_else(|| "negative_color must be a Vec3".to_string())?;
        }
        if let Some(v) = props.get("alpha") {
            self.alpha = v
                .as_float()
                .ok_or_else(|| "alpha must be a float".to_string())?;
        }
        if let Some(v) = props.get("colormap") {
            let TextValue::String(text) = v else {
                return Err("colormap must be a string".to_string());
            };
            self.colormap = colormap_from_text(text)?;
        }
        if let Some(v) = props.get("color_min") {
            self.color_min = v
                .as_float()
                .ok_or_else(|| "color_min must be a float".to_string())?;
        }
        if let Some(v) = props.get("color_max") {
            self.color_max = v
                .as_float()
                .ok_or_else(|| "color_max must be a float".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("field".to_string(), (true, None));
        m.insert("color_field".to_string(), (false, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "isosurface".to_string(),
        // Grouped with `import_cube` rather than under Geometry3D, which would
        // imply a CSG composability this type does not have.
        description: "Draws the surface where a scalar field (from import_cube) equals a given \
level - the standard way to picture a molecular orbital, an electron density or an \
electrostatic potential.
The level is a magnitude: the surface is extracted at +level AND at -level, so a signed field \
such as an orbital shows both lobes, painted in the positive and negative colors. Wiring the \
level input pin overrides the stored value.
A level larger than anything in the field produces an empty surface with no error - hover the \
field output pin of the upstream import_cube node, whose readout carries the field's value range \
along with the description the file was written with.
Extraction resolution is not part of the node: it comes from the isosurface preferences, so \
changing it re-renders without touching the document."
            .to_string(),
        summary: Some("Level-set surface of a scalar field".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "field".to_string(),
                data_type: DataType::ScalarField,
            },
            Parameter {
                id: None,
                name: "color_field".to_string(),
                data_type: DataType::ScalarField,
            },
            Parameter {
                id: None,
                name: "level".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: vec![OutputPinDefinition::fixed("surface", DataType::Isosurface)],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(IsosurfaceNodeData::default()),
        node_data_saver: generic_node_data_saver::<IsosurfaceNodeData>,
        node_data_loader: generic_node_data_loader::<IsosurfaceNodeData>,
    }
}
