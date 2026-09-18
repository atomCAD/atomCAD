//! `proxy` — cut a simulation proxy around tagged focus atoms.
//!
//! Phase 3 of `doc/design_proxy_node.md`. A thin adapter over
//! `atomcad_crystolecule::proxy_cut`: this file reads pins, validates, calls
//! the module **once**, and stores the report. It never traverses a bond,
//! places a cap, or decides a distance (§7.1's layering rule).
//!
//! `HasAtoms` in, same type out, in the `tag` / `freeze` / `atom_cut` family:
//! every tunable is both a node-data property and an input pin, and a
//! connected pin overrides the stored value. The node removes *and* adds
//! atoms, so it goes through `map_atomic` rather than the metadata-only
//! `map_atomic_in_region`.
//!
//! There is no `diff` output pin (§5): `snapshot_atoms` / `eval_output_with_diff`
//! exist to feed one, and a second output is deliberately declined. Were one
//! ever wanted it is appended as pin 1, exactly as `atom_cut` has it.
//!
//! The stats go into `context.selected_node_eval_cache` the way `relax`'s
//! message does, **never** onto the node data — subnetwork node state is
//! shared across call sites, so a `RefCell<Option<ProxyStats>>` here would be
//! wrong. `available_tags` is the one mutable field, and it is a write-only
//! snapshot for the property panel's dropdown (same as `tag`).

use crate::data_type::DataType;
use crate::evaluator::atom_op::map_atomic;
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
use atomcad_crystolecule::atomic_constants::{ALLOWED_PASSIVANTS, is_allowed_passivant};
use atomcad_crystolecule::proxy_cut::{ProxyError, ProxyOptions, proxy_cut};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Re-exported so a consumer of the node (the API layer, the property panel)
/// does not have to name the crystolecule crate to spell the report type.
pub use atomcad_crystolecule::proxy_cut::ProxyStats;

fn default_focus() -> String {
    "focus".to_string()
}
fn default_hops() -> i32 {
    6
}
fn default_free() -> i32 {
    3
}
fn default_rm_single() -> bool {
    false
}
fn default_passivate() -> bool {
    true
}
fn default_passiv_elem() -> i16 {
    1
}
fn default_core() -> i32 {
    -1
}
fn default_fill() -> bool {
    true
}

/// Node data for `proxy`. Every persisted field is a text property **and** an
/// input pin; each has a serde default so a `.cnnd` written before a field
/// existed loads with the documented value (a file without `fill` loads
/// `fill: true`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyData {
    /// Tag name marking the source atoms. Every atom carrying it is at
    /// distance 0.
    #[serde(default = "default_focus")]
    pub focus: String,
    /// Keep heavy atoms whose bond distance is at most this.
    #[serde(default = "default_hops")]
    pub hops: i32,
    /// Heavy atoms farther than this get the frozen flag.
    #[serde(default = "default_free")]
    pub free: i32,
    /// Drop heavy atoms the cut left with a single heavy neighbour, to a
    /// fixpoint. Off by default so the `hops` series stays monotonic.
    #[serde(default = "default_rm_single")]
    pub rm_single: bool,
    /// Cap every severed bond with a terminator along the old bond vector.
    #[serde(default = "default_passivate")]
    pub passivate: bool,
    /// Terminator element (atomic number); H/F/Cl/Br/I.
    #[serde(default = "default_passiv_elem")]
    pub passiv_elem: i16,
    /// Tag heavy atoms within this distance as `high` (the ONIOM high layer).
    /// Negative disables.
    #[serde(default = "default_core")]
    pub core: i32,
    /// Keep every dropped heavy atom that bridges two or more kept heavy
    /// atoms, to a fixpoint.
    #[serde(default = "default_fill")]
    pub fill: bool,
    /// Input structure's tag names, captured in `eval` for the editor's
    /// suggestion dropdown. Write-only from `eval`; never serialized.
    #[serde(skip)]
    pub available_tags: RefCell<Vec<String>>,
}

impl Default for ProxyData {
    fn default() -> Self {
        Self {
            focus: default_focus(),
            hops: default_hops(),
            free: default_free(),
            rm_single: default_rm_single(),
            passivate: default_passivate(),
            passiv_elem: default_passiv_elem(),
            core: default_core(),
            fill: default_fill(),
            available_tags: RefCell::new(Vec::new()),
        }
    }
}

/// Root-eval report, stored in `context.selected_node_eval_cache` exactly as
/// `RelaxEvalCache` is.
#[derive(Debug, Clone)]
pub struct ProxyEvalCache {
    pub stats: ProxyStats,
}

/// Reads the input's `tag_names()` table (empty for non-atomic inputs) so the
/// property editor can offer existing names as suggestions — the same
/// write-only snapshot `tag` takes.
fn snapshot_tag_names(input: &NetworkResult) -> Vec<String> {
    match input {
        NetworkResult::Crystal(c) => c.atoms.tag_names().to_vec(),
        NetworkResult::Molecule(m) => m.atoms.tag_names().to_vec(),
        _ => Vec::new(),
    }
}

impl NodeData for ProxyData {
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

        // An errored input is forwarded verbatim — never re-wrapped.
        if let NetworkResult::Error(_) = input_val {
            return EvalOutput::single(input_val);
        }

        // Snapshot the input's tag table for the editor's suggestion list.
        // Must read before `map_atomic` consumes the value below.
        *self.available_tags.borrow_mut() = snapshot_tag_names(&input_val);

        // Pins 1–8: a wired pin overrides the stored property.
        macro_rules! pin {
            ($index:expr, $default:expr, $extractor:path) => {
                match network_evaluator.evaluate_or_default(
                    network_stack,
                    node_id,
                    registry,
                    context,
                    $index,
                    $default,
                    $extractor,
                ) {
                    Ok(value) => value,
                    Err(error) => return EvalOutput::single(error),
                }
            };
        }

        let focus = pin!(1, self.focus.clone(), NetworkResult::extract_string);
        let hops = pin!(2, self.hops, NetworkResult::extract_int);
        let free = pin!(3, self.free, NetworkResult::extract_int);
        let rm_single = pin!(4, self.rm_single, NetworkResult::extract_bool);
        let passivate = pin!(5, self.passivate, NetworkResult::extract_bool);
        let passiv_elem = pin!(6, self.passiv_elem as i32, NetworkResult::extract_int) as i16;
        let core = pin!(7, self.core, NetworkResult::extract_int);
        let fill = pin!(8, self.fill, NetworkResult::extract_bool);

        // Validation, in the node's own words (§4.9 / §7.6 step 4).
        let focus = focus.trim().to_string();
        if focus.is_empty() {
            return EvalOutput::single(NetworkResult::Error(
                "proxy: focus tag name is empty".to_string(),
            ));
        }
        if hops < 0 {
            return EvalOutput::single(NetworkResult::Error(
                "proxy: hops must be >= 0".to_string(),
            ));
        }
        // `free < 0` is treated as 0 rather than rejected; `core < 0` is off.
        let free = free.max(0);
        if !is_allowed_passivant(passiv_elem) {
            return EvalOutput::single(NetworkResult::Error(format!(
                "proxy.passiv_elem: {} is not an allowed passivant; expected one of {:?} \
                 (H/F/Cl/Br/I)",
                passiv_elem, ALLOWED_PASSIVANTS
            )));
        }

        let options = ProxyOptions {
            hops: hops as u32,
            free: free as u32,
            fill,
            rm_single,
            passivate,
            passivant_element: passiv_elem,
            core: if core < 0 { None } else { Some(core as u32) },
        };

        // The cut can fail, but the mutation closure returns the structure
        // rather than a `Result`, so capture the outcome and surface it after
        // the map — the way `tag` captures its tag-limit error.
        let mut cut_error: Option<ProxyError> = None;
        let mut cut_stats: Option<ProxyStats> = None;
        let output = map_atomic(input_val, |mut structure| {
            match proxy_cut(&mut structure, &focus, &options) {
                Ok(stats) => cut_stats = Some(stats),
                Err(error) => cut_error = Some(error),
            }
            structure
        });

        if let Some(error) = cut_error {
            let message = match error {
                ProxyError::NoFocusAtoms => {
                    format!("proxy: no atom carries the tag \"{}\"", focus)
                }
                other => format!("proxy: {}", other),
            };
            return EvalOutput::single(NetworkResult::Error(message));
        }

        // Root evaluations only: the cache is per root evaluation of the
        // selected node, which is what the panel wants.
        if network_stack.len() == 1
            && let Some(stats) = cut_stats
        {
            context.selected_node_eval_cache = Some(Box::new(ProxyEvalCache { stats }));
        }

        EvalOutput::single(output)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        // A wired pin overrides its stored value, so the subtitle would read
        // stale the moment any of the three it shows is connected.
        if connected_input_pins.contains("focus")
            || connected_input_pins.contains("hops")
            || connected_input_pins.contains("free")
        {
            return None;
        }
        Some(format!("{} · {} / {}", self.focus, self.hops, self.free))
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m.insert("focus".to_string(), (false, None));
        m.insert("hops".to_string(), (false, None));
        m.insert("free".to_string(), (false, None));
        m.insert("rm_single".to_string(), (false, None));
        m.insert("passivate".to_string(), (false, None));
        m.insert("passiv_elem".to_string(), (false, None));
        m.insert("core".to_string(), (false, None));
        m.insert("fill".to_string(), (false, None));
        m
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("focus".to_string(), TextValue::String(self.focus.clone())),
            ("hops".to_string(), TextValue::Int(self.hops)),
            ("free".to_string(), TextValue::Int(self.free)),
            ("rm_single".to_string(), TextValue::Bool(self.rm_single)),
            ("passivate".to_string(), TextValue::Bool(self.passivate)),
            (
                "passiv_elem".to_string(),
                TextValue::Int(self.passiv_elem as i32),
            ),
            ("core".to_string(), TextValue::Int(self.core)),
            ("fill".to_string(), TextValue::Bool(self.fill)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("focus") {
            self.focus = v
                .as_string()
                .ok_or_else(|| "focus must be a string".to_string())?
                .to_string();
        }
        if let Some(v) = props.get("hops") {
            self.hops = v
                .as_int()
                .ok_or_else(|| "hops must be an integer".to_string())?;
        }
        if let Some(v) = props.get("free") {
            self.free = v
                .as_int()
                .ok_or_else(|| "free must be an integer".to_string())?;
        }
        if let Some(v) = props.get("rm_single") {
            self.rm_single = v
                .as_bool()
                .ok_or_else(|| "rm_single must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("passivate") {
            self.passivate = v
                .as_bool()
                .ok_or_else(|| "passivate must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("passiv_elem") {
            self.passiv_elem = v
                .as_int()
                .ok_or_else(|| "passiv_elem must be an integer".to_string())?
                as i16;
        }
        if let Some(v) = props.get("core") {
            self.core = v
                .as_int()
                .ok_or_else(|| "core must be an integer".to_string())?;
        }
        if let Some(v) = props.get("fill") {
            self.fill = v
                .as_bool()
                .ok_or_else(|| "fill must be a boolean".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "proxy".to_string(),
        description: "Cuts a simulation proxy — a cluster around the atoms carrying the `focus` \
                      tag, capped where the cut severed bonds and frozen beyond a given shell so \
                      the interior still feels bulk. Distance is **bond hops** over heavy atoms \
                      (an atom with exactly one bond is a rider: never traversed, kept with its \
                      host), not a sphere, so the proxy never contains a fragment the site is not \
                      bonded to and the `hops` series is a discrete convergence series.\n\
                      \n\
                      `hops` is how far the cut reaches; `free` is how far the relaxation may \
                      move (heavy atoms beyond it get the frozen flag, and flags are only ever \
                      set, never cleared). `fill` keeps every dropped atom that bridged two kept \
                      ones, which is what stops the diamond lattice leaving cap pairs 1.42 Å \
                      apart — its atoms all lie beyond `hops`, so with `free < hops` they are \
                      frozen rim and add no degrees of freedom. `rm_single` trims atoms the cut \
                      left singly attached. `passivate` caps **only severed bonds**, so an atom \
                      that was unsaturated in the input — a tool apex, a T-centre carbon — stays \
                      unsaturated. `core` tags the innermost shells `high` for an ONIOM \
                      partition; negative is off."
            .to_string(),
        summary: Some("Cut a simulation proxy around focus atoms".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "focus".to_string(),
                data_type: DataType::String,
            },
            Parameter {
                id: None,
                name: "hops".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "free".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "rm_single".to_string(),
                data_type: DataType::Bool,
            },
            Parameter {
                id: None,
                name: "passivate".to_string(),
                data_type: DataType::Bool,
            },
            Parameter {
                id: None,
                name: "passiv_elem".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "core".to_string(),
                data_type: DataType::Int,
            },
            // Any future pin must be APPENDED — pin indices are persisted in
            // wires.
            Parameter {
                id: None,
                name: "fill".to_string(),
                data_type: DataType::Bool,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ProxyData::default()),
        node_data_saver: generic_node_data_saver::<ProxyData>,
        node_data_loader: generic_node_data_loader::<ProxyData>,
    }
}
