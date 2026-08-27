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
use crate::node_data::{EvalOutput, NodeData, NodeDataError};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::field::{
    Colormap, IsosurfaceColoring, IsosurfaceData, LevelBasis, LevelResolutionError, auto_level,
};
use atomcad_util::serialization_utils::dvec3_serializer;
use glam::f64::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Default isolevel. This is the **molecular-orbital amplitude** convention,
/// which is an order of magnitude too large for an electron density (`0.002`).
/// Accepted rather than fixed: a field's meaning is not recoverable from its
/// numbers, so no single default serves both. What tells a user which way to
/// move is the upstream `field` **output** pin's hover readout, which carries
/// the value range and the source's own description.
pub const DEFAULT_LEVEL: f64 = 0.02;
/// Default enclosed fraction, matching `LOCALIZED_FRACTION` — the share of the
/// field's total integrated `|v|` that a surface at the stored level bounds.
///
/// Dormant unless [`LevelMode::Fraction`] is selected, and in the *disjoint*
/// range from [`DEFAULT_LEVEL`]: `0.72` read as an absolute level, or `0.02` as
/// a fraction, both produce nonsense. That disjointness is exactly why the node
/// stores two numbers rather than reinterpreting one.
pub const DEFAULT_LEVEL_FRACTION: f64 = 0.72;
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
    /// Which coordinate the level is expressed in — see [`LevelMode`].
    ///
    /// **The load-time default is `Absolute`, not the enum's `Auto`**, and the
    /// asymmetry is deliberate. A document written before this property existed
    /// has a `level` its author typed; defaulting it to `Auto` on load would
    /// silently ignore that number and redraw the surface somewhere else.
    /// `Default::default()` — what `node_data_creator` calls for a *newly
    /// created* node — still gives `Auto`, which is the mode a fresh node wants.
    #[serde(default = "default_level_mode_on_load")]
    pub level_mode: LevelMode,
    /// Level **magnitude**, live under [`LevelMode::Absolute`]. Extraction runs
    /// at `+level` and `-level`, so the sign is not a user choice and `<= 0` is
    /// an evaluation error.
    pub level: f64,
    /// Enclosed share of the field's total integrated `|v|`, live under
    /// [`LevelMode::Fraction`]. Exclusive `(0, 1)`.
    ///
    /// **Stored alongside [`level`](Self::level) rather than converted to it**,
    /// because `absolute -> fraction -> absolute` is lossy: every isovalue
    /// between two adjacent sorted samples encloses the same set, so
    /// `fraction_for_iso` is a step function and the round trip snaps. With two
    /// properties a mode toggle needs no conversion, no API round-trip and no
    /// wired field.
    #[serde(default = "default_level_fraction")]
    pub level_fraction: f64,
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

/// Serde-only: see [`IsosurfaceNodeData::level_mode`]. Deliberately **not**
/// `LevelMode::default()` — a load-time `Auto` would override a level the user
/// typed. Also note that moving `#[default]` to another variant later would
/// silently re-read every stored document, which this indirection prevents.
fn default_level_mode_on_load() -> LevelMode {
    LevelMode::Absolute
}

fn default_level_fraction() -> f64 {
    DEFAULT_LEVEL_FRACTION
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
            level_mode: LevelMode::default(),
            level: DEFAULT_LEVEL,
            level_fraction: DEFAULT_LEVEL_FRACTION,
            positive_color: DEFAULT_POSITIVE_COLOR,
            negative_color: DEFAULT_NEGATIVE_COLOR,
            alpha: DEFAULT_ALPHA,
            colormap: Colormap::default(),
            color_min: DEFAULT_COLOR_MIN,
            color_max: DEFAULT_COLOR_MAX,
        }
    }
}

/// Which coordinate the isolevel is expressed in.
///
/// A field spans ~10 orders of magnitude, so a raw isovalue in a text box is
/// unusable on its own; the *enclosed fraction* is the coordinate a user can
/// reason about, and `Auto` is the one that needs no coordinate at all. All
/// three resolve to an absolute magnitude in `eval`, so nothing downstream —
/// the extractor, the lattice policy, the tessellator, the renderer — knows this
/// enum exists.
///
/// See `doc/design_isosurface_level.md` Part 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LevelMode {
    /// The level is chosen from the field on every evaluation; **neither**
    /// stored number is live.
    ///
    /// The default, because `node_data_creator` takes no arguments and has no
    /// field to look at — a field-dependent default cannot be computed at node
    /// creation, so `eval` is the only honest place for it.
    #[default]
    Auto,
    /// [`IsosurfaceNodeData::level`] is an isovalue magnitude in the field's own
    /// units.
    Absolute,
    /// [`IsosurfaceNodeData::level_fraction`] is the enclosed share of the
    /// field's total integrated `|v|`.
    Fraction,
}

/// Text-format spelling of a [`LevelMode`]. Separate from the enum's `Serialize`
/// for the same reason [`colormap_to_text`] is: the `.cnnd` JSON form and the
/// text form must be able to diverge without one silently changing the other.
fn level_mode_to_text(mode: LevelMode) -> &'static str {
    match mode {
        LevelMode::Auto => "auto",
        LevelMode::Absolute => "absolute",
        LevelMode::Fraction => "fraction",
    }
}

fn level_mode_from_text(text: &str) -> Result<LevelMode, String> {
    match text {
        "auto" => Ok(LevelMode::Auto),
        "absolute" => Ok(LevelMode::Absolute),
        "fraction" => Ok(LevelMode::Fraction),
        other => Err(format!(
            "unknown level_mode '{}' (expected one of: auto, absolute, fraction)",
            other
        )),
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

        // Pin 1, `color_field`, is optional, so `evaluate_arg` rather than
        // `evaluate_arg_required`: `NetworkResult::None` is the unwired signal
        // and means "paint per sign", not "missing input".
        //
        // Evaluated in pin order, ahead of `level`, which also fixes the error
        // precedence when both are bad: the earlier pin's error is the one
        // reported. Nothing depends on that beyond it being stable.
        let color_arg =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let coloring = match color_arg {
            NetworkResult::None => IsosurfaceColoring::Phase {
                positive: self.positive_color.as_vec3(),
                negative: self.negative_color.as_vec3(),
            },
            // The domain stays `f64`: it is compared against `sample()` output,
            // and `color_min > color_max` is not rejected here — `sample_colormap`
            // maps a degenerate domain to the ramp's midpoint, which is a
            // legible answer for a value the user is mid-way through typing.
            NetworkResult::ScalarField(color_field) => IsosurfaceColoring::Field {
                field: color_field,
                range: (self.color_min, self.color_max),
                colormap: self.colormap,
            },
            other if other.is_error() => return EvalOutput::single(other),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "isosurface: color_field input is not a ScalarField".to_string(),
                ));
            }
        };

        // One pin, feeding *whichever property is live* — a second pin would
        // always be inert. Pin 2, not pin 1: pin 1 is `color_field`. The pin
        // order is the design's and is fixed — inserting a pin ahead of `level`
        // later would silently re-target every wire in every saved project.
        //
        // Under `Auto` the pin is not evaluated at all: neither stored number is
        // live there, so a wired value has nothing to feed. Validation raises a
        // non-blocking warning saying so, which is the only place that state is
        // reported — the node still produces a good surface.
        //
        // Which stored number the pin falls back to is therefore mode-dependent,
        // which is why the fallback is chosen before the resolution below rather
        // than inside it.
        let stored_live_value = match self.level_mode {
            LevelMode::Auto => None,
            LevelMode::Absolute => Some(self.level),
            LevelMode::Fraction => Some(self.level_fraction),
        };
        let level_input = match stored_live_value {
            None => None,
            Some(stored) => match network_evaluator.evaluate_or_default(
                network_stack,
                node_id,
                registry,
                context,
                2,
                stored,
                NetworkResult::extract_float,
            ) {
                Ok(value) => Some(value),
                Err(error) => return EvalOutput::single(error),
            },
        };

        let (level, level_basis) = match self.level_mode {
            LevelMode::Absolute => {
                let level = level_input.expect("the pin is evaluated in every non-auto mode");
                // NaN is spelled out rather than left to `!(level > 0.0)`: a
                // wired `expr` can produce one, and it must be rejected here
                // instead of turning every comparison in the extractor false and
                // yielding a silently empty mesh.
                if level.is_nan() || level <= 0.0 {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "isosurface: level must be greater than 0 (got {}); \
                         it is a magnitude — both signs are always drawn",
                        level
                    )));
                }
                (level, LevelBasis::Absolute)
            }
            LevelMode::Fraction => {
                let fraction = level_input.expect("the pin is evaluated in every non-auto mode");
                // Re-checked here even though validation already covers the
                // *stored* number, because the `level` pin can feed this one and
                // validation never sees a wired value.
                if let Some(message) = level_fraction_problem(fraction) {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "isosurface: {}",
                        message
                    )));
                }
                let Some(distribution) = field.value_distribution() else {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "isosurface: {}",
                        LevelResolutionError::AnalyticFieldInFractionMode
                    )));
                };
                // `value_distribution` is `Some` while this is `None` for an
                // all-zero field — the combination easy to miss, and the reason
                // neither of these may be an `unwrap`.
                let Some(level) = distribution.iso_for_fraction(fraction) else {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "isosurface: {}",
                        LevelResolutionError::AllZeroField
                    )));
                };
                (level, LevelBasis::Fraction(fraction))
            }
            LevelMode::Auto => match auto_level(field.as_ref()) {
                Ok((level, basis)) => (level, LevelBasis::Auto(basis)),
                Err(error) => {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "isosurface: {}",
                        error
                    )));
                }
            },
        };

        EvalOutput::single(NetworkResult::Isosurface(IsosurfaceData {
            field,
            level,
            level_basis,
            coloring,
            alpha: self.alpha as f32,
        }))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    /// The mode and the **stored** number — never a resolved one, because this
    /// has no evaluation context and so no field to resolve against.
    ///
    /// Under `Auto` it shows `auto` alone: neither stored number is live there,
    /// and printing one `eval` will not use is worse than printing none. That
    /// also makes it the one mode where a wired `level` pin does not blank the
    /// subtitle — the wire is ignored, so there is nothing for it to hide.
    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        match self.level_mode {
            LevelMode::Auto => Some("auto".to_string()),
            _ if connected_input_pins.contains("level") => None,
            // Spelled differently on purpose: flipping the mode with the pin
            // wired silently changes what the value *means*, and `0.02` is a
            // perfectly valid fraction. Visibility is the mitigation.
            LevelMode::Fraction => Some(format!(
                "level: {:.1}% (fraction)",
                self.level_fraction * 100.0
            )),
            LevelMode::Absolute => Some(format!("level: {:.4}", self.level)),
        }
    }

    /// Rules that need only stored data and wires — the ones evaluation would
    /// either never reach (an unwired `field` never evaluates this node) or
    /// could not see at all (a wired pin's value is not stored).
    ///
    /// The absolute `level > 0` check is **not** here: it stays in `eval`, where
    /// it also covers a wired pin, and duplicating it would put two identical
    /// red lines on one node.
    fn get_data_error(&self, connected_input_pins: &HashSet<String>) -> Option<NodeDataError> {
        match self.level_mode {
            LevelMode::Fraction => level_fraction_problem(self.level_fraction)
                .map(|message| NodeDataError::blocking(message.to_string())),
            LevelMode::Auto if connected_input_pins.contains("level") => {
                // **Non-blocking.** The node still produces a good surface, so
                // nothing downstream may be poisoned; what is wrong is only that
                // the user's wire does nothing.
                Some(NodeDataError::warning(
                    "the level pin is ignored in auto mode; switch to absolute or fraction \
                     to use it"
                        .to_string(),
                ))
            }
            _ => None,
        }
    }

    /// Emits `level_mode` **and both level properties, always.**
    ///
    /// The temptation is to emit only the live one, and it is wrong: the two
    /// properties exist precisely so the dormant one survives a mode toggle, and
    /// the text format is a round-trip path (copy/paste, the CLI, an AI edit).
    /// Emitting only the live number would silently discard the other. The cost
    /// is one inert-looking number, and the `level_mode` beside it says which.
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "level_mode".to_string(),
                TextValue::String(level_mode_to_text(self.level_mode).to_string()),
            ),
            ("level".to_string(), TextValue::Float(self.level)),
            (
                "level_fraction".to_string(),
                TextValue::Float(self.level_fraction),
            ),
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

    /// **Naming a level property implies its mode unless `level_mode` is
    /// explicit.** Without that, `isosurface { level: 0.002 }` would store into
    /// the dead slot while a defaulted `Auto` ignored it and chose its own level.
    ///
    /// `Auto` is reachable only by naming it. This is applied to the *existing*
    /// node data and is only called when at least one literal property is
    /// present, so `isosurface { }` applied to a node already in `Fraction`
    /// leaves it in `Fraction`; omission cannot set the mode back to auto.
    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        let named_mode = props.contains_key("level_mode");
        let named_level = props.contains_key("level");
        let named_fraction = props.contains_key("level_fraction");

        // An error naming both properties, not a silent precedence rule. The
        // emitted form never trips it, since that always names the mode.
        if !named_mode && named_level && named_fraction {
            return Err(
                "naming both 'level' and 'level_fraction' is ambiguous; add \
                 'level_mode: absolute' or 'level_mode: fraction' to say which one is live"
                    .to_string(),
            );
        }

        if let Some(v) = props.get("level_mode") {
            let TextValue::String(text) = v else {
                return Err("level_mode must be a string".to_string());
            };
            self.level_mode = level_mode_from_text(text)?;
        }
        if let Some(v) = props.get("level") {
            self.level = v
                .as_float()
                .ok_or_else(|| "level must be a float".to_string())?;
        }
        if let Some(v) = props.get("level_fraction") {
            self.level_fraction = v
                .as_float()
                .ok_or_else(|| "level_fraction must be a float".to_string())?;
        }
        if !named_mode {
            if named_level {
                self.level_mode = LevelMode::Absolute;
            } else if named_fraction {
                self.level_mode = LevelMode::Fraction;
            }
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

/// The one statement of the fraction's legal range, shared by the validation
/// pass (which sees the stored number) and `eval` (which sees a wired one).
///
/// NaN is spelled out rather than left to `!(f > 0.0 && f < 1.0)`, matching the
/// absolute check: a wired `expr` produces one easily, and the message should
/// name the value rather than the comparison that happened to fail.
fn level_fraction_problem(fraction: f64) -> Option<String> {
    if fraction.is_nan() {
        return Some(
            "level_fraction must be a number strictly between 0 and 1 (got NaN); \
             it is the share of the field's total integrated |v| that the surface encloses"
                .to_string(),
        );
    }
    if fraction <= 0.0 || fraction >= 1.0 {
        return Some(format!(
            "level_fraction must be strictly between 0 and 1 (got {}); \
             it is the share of the field's total integrated |v| that the surface encloses",
            fraction
        ));
    }
    None
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
The level mode picks how that number is expressed. 'auto' chooses it from the field itself \
(0.002 for a non-negative density-like field, otherwise the level enclosing 72% of the field's \
total integrated magnitude) and is the default for a new node; 'absolute' uses the stored level \
as an isovalue; 'fraction' uses the stored level_fraction, the share of the field's total \
integrated magnitude that the surface encloses. Auto is volatile by design - the level moves \
when the field changes, so a figure that must not change belongs in absolute or fraction mode.
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
