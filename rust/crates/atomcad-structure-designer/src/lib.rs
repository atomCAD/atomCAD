#![allow(clippy::module_inception)]

//! The node network system: types, evaluator, serialization, text format,
//! layout, undo and the ~130 built-in node implementations.
//!
//! Extracted from `rust/src/structure_designer/` by Phase 6 of
//! `doc/design_rust_crate_split.md`. Two things about the boundary are worth
//! knowing before you edit here:
//!
//! - **`expr` is a submodule of this crate, not a peer** (D8). The expression
//!   language needs `DataType`, `RecordType` and `NetworkResult`, and
//!   `NetworkResult` needs `NodeTypeRegistry`, which needs `node_network`,
//!   which needs the whole `nodes/` tree. There is no thin seam to cut, and
//!   `architecture_overview.md` simply described a component as a peer.
//!   Making it independent means generalising it over a value trait — a
//!   redesign, listed under Deferred in the design doc.
//! - **Nothing here may reference `api/`.** That was the last and largest of
//!   the four back-edges (145 sites); D9/D10 deleted it, and the compiler now
//!   enforces it — the root crate is not in this crate's manifest. When a type
//!   has to be visible to Dart, keep the authoritative definition here and a
//!   same-named twin in `api/` with `From` impls (D9a).

pub mod camera_settings;
pub mod canonicalize;
pub mod canvas_viewport;
pub mod closure_network_conversion;
pub mod common_constants;
pub mod data_type;
pub mod displayed_node_refs;
pub mod eval_errors;
pub mod evaluator;
// D8: a component of the node network, not a peer module. See the crate doc.
pub mod expr;
pub mod identifier;
pub mod implicit_eval;
pub mod invariants;
pub mod layout;
pub mod navigation_history;
pub mod network_usages;
pub mod network_validator;
pub mod node_data;
pub mod node_dependency_analysis;
pub mod node_display_policy_resolver;
pub mod node_inlining;
pub mod node_layout;
pub mod node_network;
pub mod node_network_gadget;
pub mod node_networks_import_manager;
pub mod node_type;
pub mod node_type_registry;
pub mod nodes;
pub mod preferences;
pub mod promote_to_parameter;
pub mod recent_files;
pub mod scene_tessellator;
pub mod scoped_validation_errors;
pub mod selection_factoring;
pub mod serialization;
pub mod structure_designer;
pub mod structure_designer_changes;
pub mod structure_designer_scene;
pub mod text_format;
pub mod undo;
pub mod utils;
