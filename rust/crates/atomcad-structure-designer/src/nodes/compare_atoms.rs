//! `compare_atoms` — compare two atomic structures up to a tolerance and,
//! optionally, a rigid motion.
//!
//! Phase 3 of `doc/design_structure_comparison.md` (§4). A thin wrapper over
//! `atomcad_crystolecule::structure_compare::compare`: this file cuts the two
//! clusters (the atoms carrying `tag`, or every atom), calls the core once and
//! turns its `Comparison` into pins. It decides nothing about equality itself,
//! so its verdict is the one `chemisorb`'s `unique` reaches on the same
//! clusters and settings (D1) — which is what makes it the tool for finding out
//! why two candidates were or were not merged.
//!
//! A pure function of its inputs: no eval cache, no stored result. The
//! properties are not pins; the four of them are what a comparison is.
//! `available_tags` is the one mutable field, a write-only snapshot for the
//! property panel's suggestions, as on `proxy`.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::{MoleculeData, NetworkResult};
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::structure_compare::{
    Cluster, CompareError, CompareSettings, Comparison, DEFAULT_TOLERANCE, MAX_TOLERANCE, Motion,
    compare,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// The name of the record on the `report` pin.
pub const COMPARE_REPORT: &str = "CompareReport";

/// Every motion, in the panel's order: from the strictest to the loosest.
pub const MOTIONS: [Motion; 4] = [
    Motion::None,
    Motion::Translation,
    Motion::Rigid,
    Motion::RigidMirror,
];

/// The inverse of [`Motion::name`]; `None` for an unknown spelling.
pub fn parse_motion(text: &str) -> Option<Motion> {
    MOTIONS.into_iter().find(|m| m.name() == text.trim())
}

fn default_tolerance() -> f64 {
    DEFAULT_TOLERANCE
}
fn default_compare_bonds() -> bool {
    true
}

/// Node data for `compare_atoms`. The defaults are `chemisorb`'s `unique`
/// defaults, so a node with only `tag` set compares as `unique` does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareAtomsData {
    /// Compare only the atoms carrying this tag, on both sides. Empty = every
    /// atom.
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub motion: Motion,
    /// Max per-atom deviation after the fit, Å, in `(0, MAX_TOLERANCE]`.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// Bonds between compared atoms must agree in presence and order.
    #[serde(default = "default_compare_bonds")]
    pub compare_bonds: bool,
    /// Both inputs' tag names, captured in `eval` for the editor's
    /// suggestions. Write-only from `eval`; never serialized.
    #[serde(skip)]
    pub available_tags: RefCell<Vec<String>>,
}

impl Default for CompareAtomsData {
    fn default() -> Self {
        Self {
            tag: String::new(),
            motion: Motion::default(),
            tolerance: default_tolerance(),
            compare_bonds: default_compare_bonds(),
            available_tags: RefCell::new(Vec::new()),
        }
    }
}

impl CompareAtomsData {
    pub fn compare_settings(&self) -> CompareSettings {
        CompareSettings {
            motion: self.motion,
            tolerance: self.tolerance,
            compare_bonds: self.compare_bonds,
            ..CompareSettings::default()
        }
    }
}

/// The input's atoms, or the message to output on every pin: an upstream
/// error's own text (forwarded verbatim, never re-wrapped), or a type
/// complaint.
fn atomic_input(name: &str, value: NetworkResult) -> Result<AtomicStructure, String> {
    if let NetworkResult::Error(message) = value {
        return Err(message);
    }
    let kind = value.infer_data_type();
    value
        .extract_atomic()
        .ok_or_else(|| format!("compare_atoms: {name} must be an atomic structure, got {kind:?}"))
}

/// The compared atoms of one side, the error in the node's words.
fn cluster(name: &str, structure: &AtomicStructure, tag: &str) -> Result<Cluster, String> {
    let result = if tag.is_empty() {
        Cluster::whole(structure)
    } else {
        Cluster::from_tag(structure, tag)
    };
    result.map_err(|error| match error {
        CompareError::UnknownTag(tag) => {
            format!("compare_atoms: no atom of {name} carries the tag \"{tag}\"")
        }
        CompareError::EmptyCluster => format!("compare_atoms: {name} has no atoms"),
        other => format!("compare_atoms: {other}"),
    })
}

/// All of `b`, moved onto `a` by the comparison's fit; unmoved without one.
fn aligned(mut b: AtomicStructure, comparison: &Comparison) -> NetworkResult {
    if let Some(fit) = &comparison.transform {
        let ids: Vec<u32> = b.atom_ids().copied().collect();
        for id in ids {
            if let Some(position) = b.get_atom(id).map(|atom| atom.position) {
                b.set_atom_position(id, fit.r * position + fit.t);
            }
        }
    }
    NetworkResult::Molecule(MoleculeData {
        atoms: b,
        geo_tree_root: None,
    })
}

fn report(comparison: &Comparison) -> NetworkResult {
    NetworkResult::record(vec![
        (
            "max_deviation".to_string(),
            NetworkResult::Float(comparison.max_deviation),
        ),
        ("rmsd".to_string(), NetworkResult::Float(comparison.rmsd)),
        (
            "undecided".to_string(),
            NetworkResult::Bool(comparison.undecided),
        ),
        (
            "rejected_by".to_string(),
            NetworkResult::String(
                comparison
                    .rejected_by
                    .as_ref()
                    .map(|r| r.reason.clone())
                    .unwrap_or_default(),
            ),
        ),
        (
            "mismatches".to_string(),
            NetworkResult::Array(
                comparison
                    .mismatches
                    .iter()
                    .map(|m| NetworkResult::String(m.to_string()))
                    .collect(),
            ),
        ),
    ])
}

fn error_on_every_pin(message: String) -> EvalOutput {
    let error = NetworkResult::Error(message);
    EvalOutput::multi(vec![error.clone(), error.clone(), error])
}

impl NodeData for CompareAtomsData {
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
        let a =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        let a = match atomic_input("a", a) {
            Ok(a) => a,
            Err(error) => return error_on_every_pin(error),
        };
        let b =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        let b = match atomic_input("b", b) {
            Ok(b) => b,
            Err(error) => return error_on_every_pin(error),
        };

        // Snapshot both inputs' tags for the editor's suggestion list.
        let mut tags: Vec<String> = a.tag_names().to_vec();
        for name in b.tag_names() {
            if !tags.contains(name) {
                tags.push(name.clone());
            }
        }
        *self.available_tags.borrow_mut() = tags;

        let settings = self.compare_settings();
        if let Err(error) = settings.validate() {
            return error_on_every_pin(format!("compare_atoms: {error}"));
        }
        let tag = self.tag.trim();
        let clusters = cluster("a", &a, tag).and_then(|ca| Ok((ca, cluster("b", &b, tag)?)));
        let (cluster_a, cluster_b) = match clusters {
            Ok(pair) => pair,
            Err(message) => return error_on_every_pin(message),
        };
        let comparison = match compare(&cluster_a, &cluster_b, &settings) {
            Ok(comparison) => comparison,
            Err(error) => {
                return error_on_every_pin(format!("compare_atoms: {error}"));
            }
        };

        EvalOutput::multi(vec![
            NetworkResult::Bool(comparison.equal),
            aligned(b, &comparison),
            report(&comparison),
        ])
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        let tag = self.tag.trim();
        let scope = if tag.is_empty() { "all atoms" } else { tag };
        Some(format!(
            "{scope} · {} · {} Å",
            self.motion.name(),
            self.tolerance
        ))
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("a".to_string(), (true, None));
        m.insert("b".to_string(), (true, None));
        m
    }

    /// `aligned`: a `Bool` draws nothing, and the fit overlaid on `a` is what a
    /// user looks at.
    fn default_displayed_output_pins(&self) -> Option<HashSet<i32>> {
        Some(HashSet::from([1]))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("tag".to_string(), TextValue::String(self.tag.clone())),
            (
                "motion".to_string(),
                TextValue::String(self.motion.name().to_string()),
            ),
            ("tolerance".to_string(), TextValue::Float(self.tolerance)),
            (
                "compare_bonds".to_string(),
                TextValue::Bool(self.compare_bonds),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("tag") {
            self.tag = v
                .as_string()
                .ok_or_else(|| "tag must be a string".to_string())?
                .to_string();
        }
        if let Some(v) = props.get("motion") {
            let text = v
                .as_string()
                .ok_or_else(|| "motion must be a string".to_string())?;
            self.motion = parse_motion(text).ok_or_else(|| {
                format!(
                    "motion must be one of {}, got '{text}'",
                    MOTIONS.map(Motion::name).join(", ")
                )
            })?;
        }
        if let Some(v) = props.get("tolerance") {
            self.tolerance = v
                .as_float()
                .ok_or_else(|| "tolerance must be a number".to_string())?;
        }
        if let Some(v) = props.get("compare_bonds") {
            self.compare_bonds = v
                .as_bool()
                .ok_or_else(|| "compare_bonds must be a boolean".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "compare_atoms".to_string(),
        description: format!(
            "Compares two atomic structures up to a tolerance and, optionally, a rigid \
             motion. **Equal** means: a motion of the allowed kind and a one-to-one matching \
             of atoms, same element, with every matched pair within **tolerance** (Å) after \
             the least-squares fit of that matching — the largest deviation decides, not \
             the average — and, with **compare_bonds**, the matching agreeing on every bond \
             and its order.\n\
             \n\
             **tag** — compare only the atoms carrying this tag, on both sides (empty = \
             every atom). Bonds leaving the tagged atoms are ignored.\n\
             \n\
             **motion** — what may relate the two: `none` (compared where they stand), \
             `translation`, `rigid` (rotation and translation) or `rigid_mirror` (also a \
             reflection).\n\
             \n\
             **tolerance** — Å, at most {MAX_TOLERANCE}.\n\
             \n\
             **equal** is exact: it is only ever true when such a matching was found. \
             false is very likely right but not proven; a pair that exhausted the search \
             says `undecided` in the report. **aligned** is all of `b` moved onto `a` by \
             the best fit found (unmoved when there is none) — display it together with \
             `a` to see the comparison. **report** gives the fit's largest and RMS \
             deviation, the quick check that rejected the pair (`rejected_by`), and the \
             atoms and bonds that differ.\n\
             \n\
             To see why `chemisorb` merged two candidates or kept them apart, switch its \
             `unique` off, take two candidates' structures into `a` and `b`, set `tag` to \
             `cs_local` and `motion` / `tolerance` to `unique`'s values (the defaults \
             match)."
        ),
        summary: Some("Compare two atomic structures up to a motion".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "a".to_string(),
                data_type: DataType::HasAtoms,
            },
            // Any future pin must be APPENDED — pin indices are persisted in
            // wires.
            Parameter {
                id: None,
                name: "b".to_string(),
                data_type: DataType::HasAtoms,
            },
        ],
        output_pins: vec![
            OutputPinDefinition::fixed("equal", DataType::Bool),
            OutputPinDefinition::fixed("aligned", DataType::Molecule),
            OutputPinDefinition::fixed(
                "report",
                DataType::Record(RecordType::Named(COMPARE_REPORT.to_string())),
            ),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(CompareAtomsData::default()),
        node_data_saver: generic_node_data_saver::<CompareAtomsData>,
        node_data_loader: generic_node_data_loader::<CompareAtomsData>,
    }
}
