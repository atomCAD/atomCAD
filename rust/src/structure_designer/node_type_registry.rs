use super::node_type::{NodeType, Parameter, PinOutputType};
use super::nodes::add_hydrogen::get_node_type as add_hydrogen_get_node_type;
use super::nodes::apply::get_node_type as apply_get_node_type;
use super::nodes::apply_diff::get_node_type as apply_diff_get_node_type;
use super::nodes::array_append::get_node_type as array_append_get_node_type;
use super::nodes::array_at::get_node_type as array_at_get_node_type;
use super::nodes::array_concat::get_node_type as array_concat_get_node_type;
use super::nodes::array_len::get_node_type as array_len_get_node_type;
use super::nodes::atom_composediff::get_node_type as atom_composediff_get_node_type;
use super::nodes::atom_cut::get_node_type as atom_cut_get_node_type;
use super::nodes::atom_edit::atom_edit::get_node_type as atom_edit_get_node_type;
use super::nodes::atom_edit::atom_edit::get_node_type_motif_edit as motif_edit_get_node_type;
use super::nodes::atom_replace::get_node_type as atom_replace_get_node_type;
use super::nodes::atom_union::get_node_type as atom_union_get_node_type;
use super::nodes::bool::get_node_type as bool_get_node_type;
use super::nodes::circle::get_node_type as circle_get_node_type;
use super::nodes::closure::get_node_type as closure_get_node_type;
use super::nodes::collect::get_node_type as collect_get_node_type;
use super::nodes::comment::get_node_type as comment_get_node_type;
use super::nodes::cuboid::get_node_type as cuboid_get_node_type;
use super::nodes::dematerialize::get_node_type as dematerialize_get_node_type;
use super::nodes::diff::get_node_type as diff_get_node_type;
use super::nodes::diff_2d::get_node_type as diff_2d_get_node_type;
use super::nodes::drawing_plane::get_node_type as drawing_plane_get_node_type;
use super::nodes::edit_atom::edit_atom::get_node_type as edit_atom_get_node_type;
use super::nodes::enter_structure::get_node_type as enter_structure_get_node_type;
use super::nodes::exit_structure::get_node_type as exit_structure_get_node_type;
use super::nodes::export_xyz::get_node_type as export_xyz_get_node_type;
use super::nodes::expr::get_node_type as expr_get_node_type;
use super::nodes::extrude::get_node_type as extrude_get_node_type;
use super::nodes::facet_shell::get_node_type as facet_shell_get_node_type;
use super::nodes::filter::get_node_type as filter_get_node_type;
use super::nodes::float::get_node_type as float_get_node_type;
use super::nodes::fold::get_node_type as fold_get_node_type;
use super::nodes::foreach::get_node_type as foreach_get_node_type;
use super::nodes::free_move::get_node_type as free_move_get_node_type;
use super::nodes::free_rot::get_node_type as free_rot_get_node_type;
use super::nodes::geo_trans::get_node_type as geo_trans_get_node_type;
use super::nodes::get_structure::get_node_type as get_structure_get_node_type;
use super::nodes::half_plane::get_node_type as half_plane_get_node_type;
use super::nodes::half_space::get_node_type as half_space_get_node_type;
use super::nodes::imat3_cols::get_node_type as imat3_cols_get_node_type;
use super::nodes::imat3_diag::get_node_type as imat3_diag_get_node_type;
use super::nodes::imat3_rows::get_node_type as imat3_rows_get_node_type;
use super::nodes::import_cif::get_node_type as import_cif_get_node_type;
use super::nodes::import_xyz::get_node_type as import_xyz_get_node_type;
use super::nodes::infer_bonds::get_node_type as infer_bonds_get_node_type;
use super::nodes::int::get_node_type as int_get_node_type;
use super::nodes::intersect::get_node_type as intersect_get_node_type;
use super::nodes::intersect_2d::get_node_type as intersect_2d_get_node_type;
use super::nodes::ivec2::get_node_type as ivec2_get_node_type;
use super::nodes::ivec3::get_node_type as ivec3_get_node_type;
use super::nodes::lattice_symop::get_node_type as lattice_symop_get_node_type;
use super::nodes::lattice_vecs::get_node_type as lattice_vecs_get_node_type;
use super::nodes::map::get_node_type as map_get_node_type;
use super::nodes::mat3_cols::get_node_type as mat3_cols_get_node_type;
use super::nodes::mat3_diag::get_node_type as mat3_diag_get_node_type;
use super::nodes::mat3_rows::get_node_type as mat3_rows_get_node_type;
use super::nodes::materialize::get_node_type as materialize_get_node_type;
use super::nodes::motif::get_node_type as motif_get_node_type;
use super::nodes::motif_sub::get_node_type as motif_sub_get_node_type;
use super::nodes::parameter::get_node_type as parameter_get_node_type;
use super::nodes::polygon::get_node_type as polygon_get_node_type;
use super::nodes::print::get_node_type as print_get_node_type;
use super::nodes::product::get_node_type as product_get_node_type;
use super::nodes::range::get_node_type as range_get_node_type;
use super::nodes::record_construct::get_node_type as record_construct_get_node_type;
use super::nodes::record_destructure::get_node_type as record_destructure_get_node_type;
use super::nodes::rect::get_node_type as rect_get_node_type;
use super::nodes::reg_poly::get_node_type as reg_poly_get_node_type;
use super::nodes::relax::get_node_type as relax_get_node_type;
use super::nodes::remove_hydrogen::get_node_type as remove_hydrogen_get_node_type;
use super::nodes::sequence::get_node_type as sequence_get_node_type;
use super::nodes::sphere::get_node_type as sphere_get_node_type;
use super::nodes::string::get_node_type as string_get_node_type;
use super::nodes::structure::get_node_type as structure_get_node_type;
use super::nodes::structure_move::get_node_type as structure_move_get_node_type;
use super::nodes::structure_rot::get_node_type as structure_rot_get_node_type;
use super::nodes::supercell::get_node_type as supercell_get_node_type;
use super::nodes::union::get_node_type as union_get_node_type;
use super::nodes::union_2d::get_node_type as union_2d_get_node_type;
use super::nodes::value::get_node_type as value_get_node_type;
use super::nodes::vec2::get_node_type as vec2_get_node_type;
use super::nodes::vec3::get_node_type as vec3_get_node_type;
use super::nodes::with_structure::get_node_type as with_structure_get_node_type;
use crate::api::structure_designer::structure_designer_api_types::APINetworkWithValidationErrors;
use crate::api::structure_designer::structure_designer_api_types::APINodeCategoryView;
use crate::api::structure_designer::structure_designer_api_types::APINodeTypeView;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{
    DataType, FunctionType, RecordType, walk_data_type_record_names_mut,
};
use crate::structure_designer::node_network::Argument;
use crate::structure_designer::node_network::IncomingWire;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_network::SourcePin;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Top-level definition of a named record type. Lives in
/// `NodeTypeRegistry::record_type_defs` alongside `node_networks` (single user-type
/// namespace). Field order is **authored** — driven by the schema editor and used
/// for `record_construct` / `record_destructure` / `product` pin layouts. The
/// canonical (sorted) view used for subtyping is derived on demand by
/// `RecordType::resolve_fields`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecordTypeDef {
    pub name: String,
    /// Authored field order. Names are unique within this list (enforced by the
    /// edit-time validator). Field types may reference other record defs by
    /// name; the dependency graph must be acyclic (also enforced).
    pub fields: Vec<(String, DataType)>,
}

/// Reasons an `add_record_type_def` / `update_record_type_def` /
/// `rename_record_type_def` operation can fail. The carried `String` typically
/// names the offending def or field for display in UI errors.
#[derive(Debug, Clone, Error, PartialEq)]
pub enum RecordTypeDefError {
    #[error("name '{0}' is already taken by a record type def, node network, or built-in type")]
    NameCollision(String),
    #[error("'{0}' is a built-in record type")]
    BuiltIn(String),
    #[error("record type def '{0}' does not exist")]
    NotFound(String),
    #[error("duplicate field name '{1}' in record type def '{0}'")]
    DuplicateField(String, String),
    #[error("cycle introduced: {description}")]
    CycleDetected { description: String },
    #[error("name '{0}' is invalid: {1}")]
    InvalidName(String, String),
}

pub struct NodeTypeRegistry {
    pub built_in_node_types: HashMap<String, NodeType>,
    pub node_networks: HashMap<String, NodeNetwork>,
    /// User-declared named record type defs. Shares the user-type namespace
    /// with `node_networks` and with `built_in_record_type_defs` (collisions
    /// are rejected at add/rename time). See `doc/design_record_types.md` and
    /// `doc/design_atom_replace_rules_input.md` Phase A.
    pub record_type_defs: HashMap<String, RecordTypeDef>,
    /// Built-in named record type defs. Registered once at construction time
    /// and never mutated, never serialized. Looked up alongside
    /// `record_type_defs` via `lookup_record_type_def`. Built-in names are
    /// reserved — `add/delete/rename/update_record_type_def` and
    /// `name_is_taken` consult this map. See
    /// `doc/design_atom_replace_rules_input.md` Phase A.
    pub built_in_record_type_defs: HashMap<String, RecordTypeDef>,
    pub design_file_name: Option<String>,
}

impl Default for NodeTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of `NodeTypeRegistry::resolve_output_type_detailed`. Carries the
/// resolved concrete `DataType` along with `via_fallback`, which is `true`
/// only when a `SameAsInput` pin resolved via its `fallback_if_disconnected`
/// because the named input had zero connections. The Flutter UI uses this
/// flag to label fallback-resolved types as "default — no input connected".
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedOutputType {
    pub data_type: DataType,
    pub via_fallback: bool,
}

impl NodeTypeRegistry {
    pub fn new() -> Self {
        let mut ret = Self {
            built_in_node_types: HashMap::new(),
            node_networks: HashMap::new(),
            record_type_defs: HashMap::new(),
            built_in_record_type_defs: HashMap::new(),
            design_file_name: None,
        };

        // Built-in record type defs. Registered before any node type so that
        // node types referencing them (e.g. `atom_replace.rules` →
        // `Array[Record(Named("ElementMapping"))]`) resolve consistently.
        // See `doc/design_atom_replace_rules_input.md` Phase A.
        ret.built_in_record_type_defs.insert(
            "ElementMapping".to_string(),
            RecordTypeDef {
                name: "ElementMapping".to_string(),
                fields: vec![
                    ("from".to_string(), DataType::Int),
                    ("to".to_string(), DataType::Int),
                ],
            },
        );

        // Annotation nodes
        ret.add_node_type(comment_get_node_type());

        ret.add_node_type(parameter_get_node_type());

        ret.add_node_type(expr_get_node_type());
        ret.add_node_type(value_get_node_type());
        ret.add_node_type(map_get_node_type());
        ret.add_node_type(filter_get_node_type());
        ret.add_node_type(fold_get_node_type());
        ret.add_node_type(foreach_get_node_type());
        ret.add_node_type(closure_get_node_type());
        ret.add_node_type(apply_get_node_type());
        ret.add_node_type(print_get_node_type());
        ret.add_node_type(sequence_get_node_type());
        ret.add_node_type(string_get_node_type());
        ret.add_node_type(bool_get_node_type());

        ret.add_node_type(int_get_node_type());
        ret.add_node_type(float_get_node_type());
        ret.add_node_type(ivec2_get_node_type());
        ret.add_node_type(ivec3_get_node_type());
        ret.add_node_type(vec2_get_node_type());
        ret.add_node_type(vec3_get_node_type());
        ret.add_node_type(imat3_rows_get_node_type());
        ret.add_node_type(imat3_cols_get_node_type());
        ret.add_node_type(imat3_diag_get_node_type());
        ret.add_node_type(mat3_rows_get_node_type());
        ret.add_node_type(mat3_cols_get_node_type());
        ret.add_node_type(mat3_diag_get_node_type());
        ret.add_node_type(range_get_node_type());
        ret.add_node_type(record_construct_get_node_type());
        ret.add_node_type(record_destructure_get_node_type());
        ret.add_node_type(product_get_node_type());
        ret.add_node_type(array_at_get_node_type());
        ret.add_node_type(array_len_get_node_type());
        ret.add_node_type(array_concat_get_node_type());
        ret.add_node_type(array_append_get_node_type());
        ret.add_node_type(collect_get_node_type());
        ret.add_node_type(lattice_vecs_get_node_type());

        ret.add_node_type(rect_get_node_type());
        ret.add_node_type(circle_get_node_type());
        ret.add_node_type(reg_poly_get_node_type());
        ret.add_node_type(polygon_get_node_type());
        ret.add_node_type(union_2d_get_node_type());
        ret.add_node_type(intersect_2d_get_node_type());
        ret.add_node_type(diff_2d_get_node_type());
        ret.add_node_type(half_plane_get_node_type());

        ret.add_node_type(extrude_get_node_type());
        ret.add_node_type(cuboid_get_node_type());
        ret.add_node_type(sphere_get_node_type());
        ret.add_node_type(half_space_get_node_type());
        ret.add_node_type(drawing_plane_get_node_type());
        ret.add_node_type(facet_shell_get_node_type());
        ret.add_node_type(union_get_node_type());
        ret.add_node_type(intersect_get_node_type());
        ret.add_node_type(diff_get_node_type());
        ret.add_node_type(geo_trans_get_node_type());
        ret.add_node_type(lattice_symop_get_node_type());
        ret.add_node_type(structure_move_get_node_type());
        ret.add_node_type(structure_rot_get_node_type());
        ret.add_node_type(motif_get_node_type());
        ret.add_node_type(motif_sub_get_node_type());
        ret.add_node_type(structure_get_node_type());
        ret.add_node_type(supercell_get_node_type());
        ret.add_node_type(get_structure_get_node_type());
        ret.add_node_type(with_structure_get_node_type());
        ret.add_node_type(materialize_get_node_type());
        ret.add_node_type(dematerialize_get_node_type());
        ret.add_node_type(exit_structure_get_node_type());
        ret.add_node_type(enter_structure_get_node_type());
        ret.add_node_type(edit_atom_get_node_type());
        ret.add_node_type(atom_edit_get_node_type());
        ret.add_node_type(motif_edit_get_node_type());
        ret.add_node_type(free_move_get_node_type());
        ret.add_node_type(free_rot_get_node_type());
        ret.add_node_type(atom_union_get_node_type());
        ret.add_node_type(apply_diff_get_node_type());
        ret.add_node_type(atom_composediff_get_node_type());
        ret.add_node_type(import_xyz_get_node_type());
        ret.add_node_type(import_cif_get_node_type());
        ret.add_node_type(export_xyz_get_node_type());
        ret.add_node_type(atom_cut_get_node_type());
        ret.add_node_type(relax_get_node_type());
        ret.add_node_type(add_hydrogen_get_node_type());
        ret.add_node_type(remove_hydrogen_get_node_type());
        ret.add_node_type(infer_bonds_get_node_type());
        ret.add_node_type(atom_replace_get_node_type());

        ret
    }

    /// Returns node types that have at least one pin compatible with the given source type.
    ///
    /// - When `dragging_from_output` is true: find nodes with compatible INPUT pins
    ///   (any input that accepts the source type)
    /// - When `dragging_from_output` is false: find nodes with compatible OUTPUT pins
    ///   (output can be converted to the source type)
    pub fn get_compatible_node_types(
        &self,
        source_type: &DataType,
        dragging_from_output: bool,
    ) -> Vec<APINodeCategoryView> {
        let direction = if dragging_from_output {
            crate::structure_designer::node_data::DragDirection::FromOutput
        } else {
            crate::structure_designer::node_data::DragDirection::FromInput
        };

        // Create iterator of (node_type, category) for all public nodes
        let built_in_iter = self
            .built_in_node_types
            .values()
            .filter(|nt| nt.public)
            .map(|nt| (nt, nt.category.clone()));

        let custom_iter = self
            .node_networks
            .values()
            .map(|network| (&network.node_type, NodeTypeCategory::Custom));

        // Two-step compatibility check per candidate node type:
        // 1. Static fast path (permissive `static_match`) — covers every
        //    node with no type properties. Author-declared collection pins
        //    keep their `S → Array[T]` / `S → Iter[T]` broadcast affordance.
        // 2. Adapter slow path — only allocates for type-parameterized nodes
        //    whose static defaults didn't match. The adapter's claim is
        //    verified by `static_match_strict` against the resolved node
        //    type, which rejects matches that only land via scalar
        //    broadcast. Adapter-shapeshifted collection pins therefore do
        //    not surface when the user dragged a scalar — see
        //    `doc/design_drag_aware_add_node.md` §"Asymmetric verification".
        let all_views: Vec<APINodeTypeView> = built_in_iter
            .chain(custom_iter)
            .filter(|(node_type, _)| {
                if static_match(node_type, source_type, direction, self) {
                    return true;
                }
                let default_data = (node_type.node_data_creator)();
                let Some(adapted) =
                    default_data.adapt_for_drag_source(source_type, direction, self)
                else {
                    return false;
                };
                let resolved = adapted
                    .calculate_custom_node_type(node_type)
                    .unwrap_or_else(|| (*node_type).clone());
                static_match_strict(&resolved, source_type, direction, self)
            })
            .map(|(node_type, category)| APINodeTypeView {
                name: node_type.name.clone(),
                description: node_type.description.clone(),
                summary: node_type.summary.clone(),
                category,
            })
            .collect();

        // Group by category
        let mut category_map: HashMap<NodeTypeCategory, Vec<APINodeTypeView>> = HashMap::new();
        for view in all_views {
            category_map
                .entry(view.category.clone())
                .or_default()
                .push(view);
        }

        // Sort nodes within each category alphabetically
        for nodes in category_map.values_mut() {
            nodes.sort_by(|a, b| a.name.cmp(&b.name));
        }

        // Build result in semantic order
        let ordered_categories = vec![
            NodeTypeCategory::Annotation,
            NodeTypeCategory::MathAndProgramming,
            NodeTypeCategory::Geometry2D,
            NodeTypeCategory::Geometry3D,
            NodeTypeCategory::AtomicStructure,
            NodeTypeCategory::OtherBuiltin,
            NodeTypeCategory::Custom,
        ];

        let mut result: Vec<APINodeCategoryView> = Vec::new();
        for category in ordered_categories {
            if let Some(nodes) = category_map.get(&category) {
                if !nodes.is_empty() {
                    result.push(APINodeCategoryView {
                        category: category.clone(),
                        nodes: nodes.clone(),
                    });
                }
            }
        }

        result
    }

    /// Retrieves views of all public node types available to users, grouped by category.
    /// Only built-in node types can be non-public; all node networks are considered public.
    pub fn get_node_type_views(&self) -> Vec<APINodeCategoryView> {
        use std::collections::HashMap;

        // Collect all node views with their categories
        let mut all_views: Vec<APINodeTypeView> = Vec::new();

        // Add built-in node types
        all_views.extend(
            self.built_in_node_types
                .values()
                .filter(|node| node.public)
                .map(|node| APINodeTypeView {
                    name: node.name.clone(),
                    description: node.description.clone(),
                    summary: node.summary.clone(),
                    category: node.category.clone(),
                }),
        );

        // Add custom node networks (all have Custom category)
        all_views.extend(self.node_networks.values().map(|network| APINodeTypeView {
            name: network.node_type.name.clone(),
            description: network.node_type.description.clone(),
            summary: network.node_type.summary.clone(),
            category: NodeTypeCategory::Custom,
        }));

        // Group by category
        let mut category_map: HashMap<NodeTypeCategory, Vec<APINodeTypeView>> = HashMap::new();
        for view in all_views {
            category_map
                .entry(view.category.clone())
                .or_default()
                .push(view);
        }

        // Sort nodes within each category alphabetically by name
        for nodes in category_map.values_mut() {
            nodes.sort_by(|a, b| a.name.cmp(&b.name));
        }

        // Build result in semantic order
        let mut result: Vec<APINodeCategoryView> = Vec::new();
        let ordered_categories = vec![
            NodeTypeCategory::Annotation,
            NodeTypeCategory::MathAndProgramming,
            NodeTypeCategory::Geometry2D,
            NodeTypeCategory::Geometry3D,
            NodeTypeCategory::AtomicStructure,
            NodeTypeCategory::OtherBuiltin,
            NodeTypeCategory::Custom,
        ];

        for category in ordered_categories {
            if let Some(nodes) = category_map.get(&category) {
                if !nodes.is_empty() {
                    result.push(APINodeCategoryView {
                        category: category.clone(),
                        nodes: nodes.clone(),
                    });
                }
            }
        }

        result
    }

    pub fn get_node_network_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .node_networks
            .values()
            .map(|network| network.node_type.name.clone())
            .collect();
        names.sort();
        names
    }

    /// Checks if a node type name corresponds to a custom node (i.e., a user-defined node network).
    pub fn is_custom_node_type(&self, node_type_name: &str) -> bool {
        self.node_networks.contains_key(node_type_name)
    }

    pub fn get_node_networks_with_validation(&self) -> Vec<APINetworkWithValidationErrors> {
        let mut networks: Vec<APINetworkWithValidationErrors> = self
            .node_networks
            .values()
            .map(|network| {
                let validation_errors = if network.validation_errors.is_empty() {
                    None
                } else {
                    Some(
                        network
                            .validation_errors
                            .iter()
                            .map(|error| error.error_text.clone())
                            .collect::<Vec<String>>()
                            .join("\n"),
                    )
                };

                APINetworkWithValidationErrors {
                    name: network.node_type.name.clone(),
                    validation_errors,
                }
            })
            .collect();
        networks.sort_by(|a, b| a.name.cmp(&b.name));
        networks
    }

    pub fn get_node_type(&self, node_type_name: &str) -> Option<&NodeType> {
        if let Some(nt) = self.built_in_node_types.get(node_type_name) {
            return Some(nt);
        }
        let node_network = self.node_networks.get(node_type_name)?;
        Some(&node_network.node_type)
    }

    /// Gets a dynamic node type for a specific node instance, handling parameter and expr nodes
    pub fn get_node_type_for_node<'a>(&'a self, node: &'a Node) -> Option<&'a NodeType> {
        // First check if the node has a cached custom node type
        if let Some(ref custom_node_type) = node.custom_node_type {
            return Some(custom_node_type);
        }

        // For regular nodes, get the standard node type
        if let Some(node_type) = self.built_in_node_types.get(&node.node_type_name) {
            return Some(node_type);
        }

        // Check if it's a custom network node type
        if let Some(node_network) = self.node_networks.get(&node.node_type_name) {
            return Some(&node_network.node_type);
        }

        None
    }

    /// Initializes custom node type cache for all parameter and expr nodes in a network,
    /// recursing into HOF zone bodies so nodes inside an `Arc<NodeNetwork>` body
    /// (e.g. an `expr` inside a `map`'s zone) also get their dynamic pin layouts.
    /// Without the zone recursion, body nodes whose pin list is built by
    /// `calculate_custom_node_type` would fall back to the bare built-in type
    /// after a `.cnnd` round-trip and panic on first parameter access.
    pub fn initialize_custom_node_types_for_network(&self, network: &mut NodeNetwork) {
        Self::initialize_custom_node_types_for_network_with_types(
            &self.built_in_node_types,
            &self.record_type_defs,
            &self.built_in_record_type_defs,
            network,
        );
    }

    /// Static, `node_networks`-free recursive variant of
    /// [`initialize_custom_node_types_for_network`]. Because it consults only the
    /// read-only type maps (never `node_networks`), it can repopulate caches for
    /// a body network that itself still lives inside `node_networks` — the caller
    /// destructures the registry's sibling fields and passes the maps in (the
    /// split-borrow pattern used by `add_node_scoped` / `inline_custom_node` for
    /// body-scoped edits).
    pub fn initialize_custom_node_types_for_network_with_types(
        built_in_types: &std::collections::HashMap<String, NodeType>,
        record_type_defs: &std::collections::HashMap<String, RecordTypeDef>,
        built_in_record_type_defs: &std::collections::HashMap<String, RecordTypeDef>,
        network: &mut NodeNetwork,
    ) {
        for node in network.nodes.values_mut() {
            Self::populate_custom_node_type_cache_with_types(
                built_in_types,
                record_type_defs,
                built_in_record_type_defs,
                node,
                false,
            );
            if let Some(body) = node.zone_mut() {
                Self::initialize_custom_node_types_for_network_with_types(
                    built_in_types,
                    record_type_defs,
                    built_in_record_type_defs,
                    body,
                );
            }
        }
    }

    /// Static helper function to populate custom node type cache without borrowing conflicts
    /// Returns whether a custom node type was populated or not
    ///
    /// `record_type_defs` and `built_in_record_type_defs` are consulted only
    /// by record-typed nodes (`record_construct`, `record_destructure`,
    /// `product`) — every other node derives its custom type from per-node
    /// data via `calculate_custom_node_type`. The two maps are looked up
    /// user-first then built-in (matching `lookup_record_type_def`).
    pub fn populate_custom_node_type_cache_with_types(
        built_in_types: &std::collections::HashMap<String, NodeType>,
        record_type_defs: &std::collections::HashMap<String, RecordTypeDef>,
        built_in_record_type_defs: &std::collections::HashMap<String, RecordTypeDef>,
        node: &mut Node,
        refresh_args: bool,
    ) -> bool {
        let Some(base_node_type) = built_in_types.get(&node.node_type_name) else {
            return false;
        };

        // Record nodes derive their custom node type from the registry (the
        // schema's authored fields), not from per-node data alone. We use a
        // wrapper registry here that exposes only the record-type-defs slices
        // — `build_node_type_for_schema` reads only those fields.
        if node.node_type_name == "record_construct" {
            if let Some(data) = node
                .data
                .as_any_ref()
                .downcast_ref::<crate::structure_designer::nodes::record_construct::RecordConstructData>()
            {
                let schema = data.schema.clone();
                let custom = crate::structure_designer::nodes::record_construct::build_node_type_for_schema_with_defs(
                    base_node_type,
                    &schema,
                    record_type_defs,
                    built_in_record_type_defs,
                );
                node.set_custom_node_type(Some(custom), refresh_args);
                return true;
            }
        } else if node.node_type_name == "record_destructure" {
            if let Some(data) = node
                .data
                .as_any_ref()
                .downcast_ref::<crate::structure_designer::nodes::record_destructure::RecordDestructureData>()
            {
                let schema = data.schema.clone();
                let custom = crate::structure_designer::nodes::record_destructure::build_node_type_for_schema_with_defs(
                    base_node_type,
                    &schema,
                    record_type_defs,
                    built_in_record_type_defs,
                );
                node.set_custom_node_type(Some(custom), refresh_args);
                return true;
            }
        } else if node.node_type_name == "product" {
            if let Some(data) =
                node.data
                    .as_any_ref()
                    .downcast_ref::<crate::structure_designer::nodes::product::ProductData>()
            {
                let target = data.target.clone();
                let custom =
                    crate::structure_designer::nodes::product::build_node_type_for_target_with_defs(
                        base_node_type,
                        &target,
                        record_type_defs,
                        built_in_record_type_defs,
                    );
                node.set_custom_node_type(Some(custom), refresh_args);
                return true;
            }
        }

        let custom_node_type = node.data.calculate_custom_node_type(base_node_type);
        let has_custom_node_type = custom_node_type.is_some();
        // Initialize zone state from the resolved type (custom if any, else
        // base) before installing the custom type — `ensure_zone_init` needs
        // a stable reference to the type for its `has_zone()` check.
        let resolved_type = custom_node_type.as_ref().unwrap_or(base_node_type);
        node.ensure_zone_init(resolved_type);
        node.set_custom_node_type(custom_node_type, refresh_args);
        has_custom_node_type
    }

    /// Populates the custom node type cache for nodes with dynamic node types
    pub fn populate_custom_node_type_cache(&self, node: &mut Node, refresh_args: bool) -> bool {
        Self::populate_custom_node_type_cache_with_types(
            &self.built_in_node_types,
            &self.record_type_defs,
            &self.built_in_record_type_defs,
            node,
            refresh_args,
        )
    }

    pub fn get_node_param_data_type(&self, node: &Node, parameter_index: usize) -> DataType {
        let node_type = self.get_node_type_for_node(node).unwrap();
        node_type.parameters[parameter_index].data_type.clone()
    }

    /// Resolves the concrete `DataType` of one of `node`'s output pins in `network`.
    ///
    /// - `output_pin_index == -1` returns the node's function type.
    /// - For a `Fixed(t)` pin, returns `Some(t)` (or `None` if `t` is abstract).
    /// - For `SameAsInput`, resolves the concrete type of the upstream wire
    ///   feeding the named input pin (recursively). When the input pin has zero
    ///   connections, the pin's `fallback_if_disconnected` is returned if set.
    /// - For `SameAsArrayElements(name)`, resolves the concrete element type
    ///   common to every source feeding the array input (`None` on mismatch,
    ///   disconnected, or unresolved upstream).
    ///
    /// Returns `None` whenever resolution fails for any reason. The returned
    /// type is never abstract.
    pub fn resolve_output_type(
        &self,
        node: &Node,
        network: &NodeNetwork,
        output_pin_index: i32,
    ) -> Option<DataType> {
        self.resolve_output_type_scoped(node, network, output_pin_index, &[], &[])
    }

    /// Scope-aware variant of [`Self::resolve_output_type`]. `ancestors` /
    /// `ancestor_hof_ids` describe the enclosing-zone chain of `network`, using
    /// the same indexing convention as the validator: `ancestors[i]` is the
    /// network at depth `i` from the root, and `ancestor_hof_ids[i]` is the HOF
    /// id in `ancestors[i]` whose zone body is `ancestors[i + 1]` — the deepest
    /// entry being the HOF whose body is `network` itself. Pass empty slices
    /// when `network` is a top-level network (no enclosing zones).
    ///
    /// Without the chain a `SameAsInput` pin fed *directly* by a body's
    /// delayed-argument (`ZoneInput`) pin — or by a cross-scope capture — has
    /// no resolvable source and dead-ends at `None`. With it, such a pin
    /// resolves to the enclosing HOF's concrete zone-input (element) type, so
    /// e.g. wiring a `map` body's `element` straight into `free_rot`'s abstract
    /// `HasFreeLinOps` input refines to the concrete element type. Build the
    /// chain with `StructureDesigner::get_scope_ancestors`.
    pub fn resolve_output_type_scoped(
        &self,
        node: &Node,
        network: &NodeNetwork,
        output_pin_index: i32,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
    ) -> Option<DataType> {
        self.resolve_output_type_detailed_scoped(
            node,
            network,
            output_pin_index,
            ancestors,
            ancestor_hof_ids,
        )
        .map(|r| r.data_type)
    }

    /// Same as `resolve_output_type`, but also reports whether the pin was
    /// resolved via the `SameAsInput` disconnected-input fallback. The Flutter
    /// API surfaces this so the UI can label fallback-resolved types as
    /// "default — no input connected" in the pin tooltip.
    pub fn resolve_output_type_detailed(
        &self,
        node: &Node,
        network: &NodeNetwork,
        output_pin_index: i32,
    ) -> Option<ResolvedOutputType> {
        self.resolve_output_type_detailed_scoped(node, network, output_pin_index, &[], &[])
    }

    /// Scope-aware variant of [`Self::resolve_output_type_detailed`]. See
    /// [`Self::resolve_output_type_scoped`] for the meaning of `ancestors` /
    /// `ancestor_hof_ids`.
    pub fn resolve_output_type_detailed_scoped(
        &self,
        node: &Node,
        network: &NodeNetwork,
        output_pin_index: i32,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
    ) -> Option<ResolvedOutputType> {
        let node_type = self.get_node_type_for_node(node)?;
        if output_pin_index == -1 {
            // Wiring-aware function pin type
            // (`doc/design_node_function_pin_captures.md`): the parameters are
            // the node's *unconnected* input pins (the connected ones are frozen
            // as captures), in pin order; the return is pin 0's resolved type.
            // Built from the specific node instance's wiring, not just its
            // declaration — so wiring/unwiring an input changes the exposed
            // function arity. Both `can_connect_nodes` and `validate_wires`
            // route through here, so the wiring-aware type is consistent
            // everywhere. Returns `None` if pin 0's type can't resolve
            // (polymorphic / unresolved), which rejects the `-1` connection
            // until resolvable (design Open Question 1).
            let return_type =
                self.resolve_output_type_scoped(node, network, 0, ancestors, ancestor_hof_ids)?;
            let params: Vec<DataType> = node_type
                .parameters
                .iter()
                .enumerate()
                .filter(|(i, _)| node.arguments.get(*i).map(|a| a.is_empty()).unwrap_or(true))
                .map(|(_, p)| p.data_type.clone())
                .collect();
            return Some(ResolvedOutputType {
                data_type: DataType::Function(FunctionType::new(params, return_type)),
                via_fallback: false,
            });
        }
        let pin = node_type.output_pins.get(output_pin_index as usize)?;
        self.resolve_pin_output_type_scoped(
            &pin.data_type,
            node,
            node_type,
            network,
            ancestors,
            ancestor_hof_ids,
        )
    }

    fn resolve_pin_output_type_scoped(
        &self,
        pin_type: &PinOutputType,
        node: &Node,
        node_type: &NodeType,
        network: &NodeNetwork,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
    ) -> Option<ResolvedOutputType> {
        match pin_type {
            PinOutputType::Fixed(t) => {
                if t.is_abstract() {
                    None
                } else {
                    Some(ResolvedOutputType {
                        data_type: t.clone(),
                        via_fallback: false,
                    })
                }
            }
            PinOutputType::SameAsInput {
                input_pin_name,
                fallback_if_disconnected,
            } => {
                // Locate the single incoming wire on the named input pin.
                // `SameAsInput` is only meaningful for single-connection
                // (non-array) input pins; a pin with 0 or >1 wires falls through
                // to the fallback/None branch below. Unlike the former
                // `single_source_for_input`, this inspects the wire directly, so
                // a `ZoneInput` (delayed-argument) or cross-scope capture source
                // is followed rather than silently dropped.
                let single_wire = node_type
                    .parameters
                    .iter()
                    .position(|p| p.name == *input_pin_name)
                    .and_then(|i| node.arguments.get(i))
                    .filter(|arg| arg.incoming_wires.len() == 1)
                    .and_then(|arg| arg.incoming_wires.first());
                match single_wire {
                    Some(wire) => self.resolve_wire_source_type_scoped(
                        wire,
                        network,
                        ancestors,
                        ancestor_hof_ids,
                    ),
                    None => {
                        // No single connected source. Apply the fallback if the
                        // input pin is genuinely disconnected (zero connections);
                        // a malformed pin name or multi-connection still yields
                        // None so type errors stay visible.
                        if self.input_is_disconnected(node, node_type, input_pin_name) {
                            fallback_if_disconnected
                                .as_ref()
                                .map(|t| ResolvedOutputType {
                                    data_type: t.clone(),
                                    via_fallback: true,
                                })
                        } else {
                            None
                        }
                    }
                }
            }
            PinOutputType::SameAsArrayElements(input_pin_name) => {
                let arg_index = node_type
                    .parameters
                    .iter()
                    .position(|p| p.name == *input_pin_name)?;
                let argument = node.arguments.get(arg_index)?;
                if argument.is_empty() {
                    return None;
                }
                let mut common: Option<DataType> = None;
                for (src_node_id, src_pin_index) in argument.iter_source_pins() {
                    let src_node = network.nodes.get(&src_node_id)?;
                    let src_ty = self.resolve_output_type(src_node, network, src_pin_index)?;
                    // Peel a single Array wrapper if present; non-array sources broadcast
                    // as single elements of that type.
                    let element_ty = match src_ty {
                        DataType::Array(inner) => *inner,
                        other => other,
                    };
                    if element_ty.is_abstract() {
                        return None;
                    }
                    match &common {
                        None => common = Some(element_ty),
                        Some(existing) if *existing == element_ty => {}
                        _ => return None,
                    }
                }
                common.map(|t| ResolvedOutputType {
                    data_type: t,
                    via_fallback: false,
                })
            }
        }
    }

    /// Resolve the concrete output type produced by a single incoming wire,
    /// following local node outputs (depth 0), cross-scope captures
    /// (`NodeOutput` at depth ≥ 1), and zone-input / delayed-argument sources
    /// (`ZoneInput` at depth ≥ 1). `network` is the wire's storage network and
    /// `ancestors` / `ancestor_hof_ids` are its enclosing-zone chain (see
    /// [`Self::resolve_output_type_scoped`]). Returns `None` for abstract or
    /// otherwise-unresolvable sources, matching the rest of the resolver.
    fn resolve_wire_source_type_scoped(
        &self,
        wire: &IncomingWire,
        network: &NodeNetwork,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
    ) -> Option<ResolvedOutputType> {
        let depth = wire.source_scope_depth as usize;
        match wire.source_pin {
            SourcePin::NodeOutput { pin_index } => {
                if depth == 0 {
                    // Local source in the same network.
                    let src_node = network.nodes.get(&wire.source_node_id)?;
                    self.resolve_output_type_detailed_scoped(
                        src_node,
                        network,
                        pin_index,
                        ancestors,
                        ancestor_hof_ids,
                    )
                } else {
                    // Capture from an ancestor network `depth` frames up.
                    if depth > ancestors.len() {
                        return None;
                    }
                    let src_network = ancestors[ancestors.len() - depth];
                    let src_node = src_network.nodes.get(&wire.source_node_id)?;
                    let new_ancestors = &ancestors[..ancestors.len() - depth];
                    let new_hof_ids = &ancestor_hof_ids[..ancestor_hof_ids.len() - depth];
                    self.resolve_output_type_detailed_scoped(
                        src_node,
                        src_network,
                        pin_index,
                        new_ancestors,
                        new_hof_ids,
                    )
                }
            }
            SourcePin::ZoneInput { pin_index } => {
                // Delayed-argument reference to an enclosing HOF's zone-input
                // pin (`element` / `acc`). The HOF lives `depth` frames up; its
                // declared zone-input pin type is the body parameter's type.
                if depth < 1 || depth > ancestors.len() || depth > ancestor_hof_ids.len() {
                    return None;
                }
                let hof_network = ancestors[ancestors.len() - depth];
                let hof_id = ancestor_hof_ids[ancestor_hof_ids.len() - depth];
                let hof_node = hof_network.nodes.get(&hof_id)?;
                let hof_node_type = self.get_node_type_for_node(hof_node)?;
                let pin = hof_node_type.zone_input_pins.get(pin_index)?;
                // Resolve the zone-input pin's declared type against the HOF's
                // own scope. For the common `Fixed(concrete)` case this returns
                // the concrete element type (and `None` for an abstract
                // declaration — concrete-only, like every other pin).
                let new_ancestors = &ancestors[..ancestors.len() - depth];
                let new_hof_ids = &ancestor_hof_ids[..ancestor_hof_ids.len() - depth];
                self.resolve_pin_output_type_scoped(
                    &pin.data_type,
                    hof_node,
                    hof_node_type,
                    hof_network,
                    new_ancestors,
                    new_hof_ids,
                )
            }
        }
    }

    /// Returns `true` when the named input pin exists on this node and has
    /// zero connections wired to it. Used to gate `SameAsInput` fallback
    /// resolution: a malformed pin name or argument count mismatch yields
    /// `false` so genuine errors aren't masked by the fallback.
    fn input_is_disconnected(
        &self,
        node: &Node,
        node_type: &NodeType,
        input_pin_name: &str,
    ) -> bool {
        let arg_index = match node_type
            .parameters
            .iter()
            .position(|p| p.name == input_pin_name)
        {
            Some(i) => i,
            None => return false,
        };
        match node.arguments.get(arg_index) {
            Some(argument) => argument.is_empty(),
            None => false,
        }
    }

    pub fn get_parameter_name(&self, node: &Node, parameter_index: usize) -> String {
        let node_type = self.get_node_type_for_node(node).unwrap();
        node_type.parameters[parameter_index].name.clone()
    }

    pub fn add_node_network(&mut self, node_network: NodeNetwork) {
        self.node_networks
            .insert(node_network.node_type.name.clone(), node_network);
    }

    /// True iff `name` is in use by a user record type def, a built-in record
    /// type def, a custom node network, or a built-in node type. Used as the
    /// namespace-collision check before adding or renaming a user-defined
    /// type.
    pub fn name_is_taken(&self, name: &str) -> bool {
        self.record_type_defs.contains_key(name)
            || self.built_in_record_type_defs.contains_key(name)
            || self.node_networks.contains_key(name)
            || self.built_in_node_types.contains_key(name)
    }

    /// Resolves a record type def by name, consulting user-declared defs first
    /// and then the built-in defs. The single lookup point used by every
    /// type-resolution / pin-layout / dropdown-population call site so that
    /// built-ins are uniformly visible. See
    /// `doc/design_atom_replace_rules_input.md` Phase A.
    pub fn lookup_record_type_def(&self, name: &str) -> Option<&RecordTypeDef> {
        self.record_type_defs
            .get(name)
            .or_else(|| self.built_in_record_type_defs.get(name))
    }

    /// True iff `name` names a built-in record type def. Used by mutation
    /// guards to reject attempts to add/delete/rename/update a built-in.
    pub fn is_built_in_record_type_def(&self, name: &str) -> bool {
        self.built_in_record_type_defs.contains_key(name)
    }

    /// Adds a new record type def. Validates: name not already taken, field
    /// names within the def are distinct, and the def's transitive references
    /// do not form a cycle. On success, the def is inserted into
    /// `record_type_defs`.
    ///
    /// Note: this does not validate that referenced record types exist —
    /// dangling references resolve to `None` at use time and are surfaced by
    /// network validation.
    pub fn add_record_type_def(&mut self, def: RecordTypeDef) -> Result<(), RecordTypeDefError> {
        if self.is_built_in_record_type_def(&def.name) {
            return Err(RecordTypeDefError::BuiltIn(def.name.clone()));
        }
        if self.name_is_taken(&def.name) {
            return Err(RecordTypeDefError::NameCollision(def.name.clone()));
        }
        validate_distinct_fields(&def.name, &def.fields)?;
        self.check_no_cycle(&def.name, &def.fields)?;
        self.record_type_defs.insert(def.name.clone(), def);
        Ok(())
    }

    /// Removes a record type def, returning the removed def. Every
    /// `RecordType::Named(name)` reference now resolves to `None` (dangling)
    /// and is reported as a validation error wherever it appears. Callers that
    /// own a `StructureDesigner` should also call `repair_node_network` on
    /// every affected network.
    ///
    /// Built-in record type defs are immutable — calls naming a built-in are
    /// silently a no-op (return `None`); the guarded entry point at
    /// `StructureDesigner::delete_record_type_def` reports an error to the
    /// user.
    pub fn delete_record_type_def(&mut self, name: &str) -> Option<RecordTypeDef> {
        if self.is_built_in_record_type_def(name) {
            return None;
        }
        self.record_type_defs.remove(name)
    }

    /// Renames a record type def in place. Updates the registry key, every
    /// `DataType` reference (parameter types, pin types, return-node output
    /// types, and DataType fields embedded in node data), and every bare-name
    /// schema property on `record_construct` / `record_destructure` / `product`
    /// nodes (the latter is a no-op until those nodes ship in Phase 3 — the
    /// walker is wired up early so the rename pass is complete).
    pub fn rename_record_type_def(
        &mut self,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), RecordTypeDefError> {
        if old_name == new_name {
            return Ok(());
        }
        if self.is_built_in_record_type_def(old_name) {
            return Err(RecordTypeDefError::BuiltIn(old_name.to_string()));
        }
        if !self.record_type_defs.contains_key(old_name) {
            return Err(RecordTypeDefError::NotFound(old_name.to_string()));
        }
        if self.is_built_in_record_type_def(new_name) {
            return Err(RecordTypeDefError::BuiltIn(new_name.to_string()));
        }
        if self.name_is_taken(new_name) {
            return Err(RecordTypeDefError::NameCollision(new_name.to_string()));
        }
        // Move the def under the new key, updating its `name` field.
        let mut def = self.record_type_defs.remove(old_name).unwrap();
        def.name = new_name.to_string();
        self.record_type_defs.insert(new_name.to_string(), def);

        // Walk every DataType reachable from the registry and rewrite
        // `RecordType::Named(old_name)` to `RecordType::Named(new_name)`.
        rewrite_record_name_in_registry(self, old_name, new_name);

        Ok(())
    }

    /// Replaces the field list of an existing record type def. Validates field
    /// names are distinct and that the new field list does not introduce a
    /// cycle (the rest of the registry plus the new fields must remain
    /// acyclic). Authored field order is preserved.
    ///
    /// Field-level edits to a def need no `DataType` rewrite — every
    /// `Named(name)` reference automatically sees the new schema. Callers should
    /// run `repair_node_network` on every affected network so
    /// `record_construct` / `record_destructure` / `product` pin layouts
    /// re-derive and now-incompatible wires are disconnected.
    pub fn update_record_type_def(
        &mut self,
        name: &str,
        new_fields: Vec<(String, DataType)>,
    ) -> Result<(), RecordTypeDefError> {
        if self.is_built_in_record_type_def(name) {
            return Err(RecordTypeDefError::BuiltIn(name.to_string()));
        }
        if !self.record_type_defs.contains_key(name) {
            return Err(RecordTypeDefError::NotFound(name.to_string()));
        }
        validate_distinct_fields(name, &new_fields)?;
        self.check_no_cycle(name, &new_fields)?;
        if let Some(def) = self.record_type_defs.get_mut(name) {
            def.fields = new_fields;
        }
        Ok(())
    }

    /// Returns true when `def_name` would, under the candidate `fields`, refer
    /// back to itself directly or via any chain of named record references.
    /// Visits other record defs through the current registry (excluding
    /// `def_name`'s old fields, which are about to be replaced).
    fn check_no_cycle(
        &self,
        def_name: &str,
        fields: &[(String, DataType)],
    ) -> Result<(), RecordTypeDefError> {
        // Treat the def-being-validated as if its fields were `fields` (not
        // whatever is currently in the registry — this also handles the
        // add-new-def case where the registry has no entry yet).
        // DFS from each direct dependency, marking visited names. If we ever
        // reach `def_name`, report a cycle with the path.
        let mut path: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let direct_refs = collect_named_record_refs(fields);
        for r in direct_refs {
            path.clear();
            visited.clear();
            path.push(r.clone());
            if dfs_cycle_check(self, def_name, &r, &mut path, &mut visited) {
                let description = if path.is_empty() {
                    format!("'{}' references itself", def_name)
                } else {
                    format!("'{}' references itself via {}", def_name, path.join(" -> "))
                };
                return Err(RecordTypeDefError::CycleDetected { description });
            }
        }
        Ok(())
    }

    fn add_node_type(&mut self, node_type: NodeType) {
        // Debug-only: a built-in node type must never declare `AnyFunction`
        // as the `Fixed` type on any output pin (or on any `SameAsInput`
        // pin's `fallback_if_disconnected`). `AnyFunction` is an
        // input-pin-only acceptance constraint — sources always carry a
        // concrete `Function`. See
        // `doc/design_function_pin_unification.md` Phase A.
        #[cfg(debug_assertions)]
        for pin in &node_type.output_pins {
            match &pin.data_type {
                PinOutputType::Fixed(t) => {
                    assert!(
                        !matches!(t, DataType::AnyFunction { .. }),
                        "Node type '{}' output pin '{}' declares AnyFunction; \
                         AnyFunction is input-only",
                        node_type.name,
                        pin.name,
                    );
                }
                PinOutputType::SameAsInput {
                    fallback_if_disconnected: Some(t),
                    ..
                } => {
                    assert!(
                        !matches!(t, DataType::AnyFunction { .. }),
                        "Node type '{}' output pin '{}' has AnyFunction in \
                         fallback_if_disconnected; AnyFunction is input-only",
                        node_type.name,
                        pin.name,
                    );
                }
                _ => {}
            }
        }
        self.built_in_node_types
            .insert(node_type.name.clone(), node_type);
    }

    /// Finds all networks that use the specified network as a node
    ///
    /// # Parameters
    /// * `network_name` - The name of the network to find parents for
    ///
    /// # Returns
    /// A vector of network names that contain nodes of the specified network type
    pub fn find_parent_networks(&self, network_name: &str) -> Vec<String> {
        let mut parent_networks = Vec::new();

        // Search through all networks to find ones that use this network as a node.
        // A reference inside an HOF's owned body still makes the containing
        // named network a parent, so descend into zones during the search.
        for (parent_name, parent_network) in &self.node_networks {
            // Skip the network itself
            if parent_name == network_name {
                continue;
            }

            let mut found = false;
            crate::structure_designer::node_network::walk_all_nodes(parent_network, &mut |node| {
                if !found && node.node_type_name == network_name {
                    found = true;
                }
            });
            if found {
                parent_networks.push(parent_name.clone());
            }
        }

        parent_networks
    }

    /// Top-level driver for the Currying Phase 3 apply post-pass: for every
    /// `apply` node in `network` whose `f` pin is wired to a resolvable
    /// `Function` source, override the node's `custom_node_type` from the
    /// wired source's declared (canonical, flat) function type and the count
    /// of wired arg pins on this apply.
    ///
    /// Called from `repair_node_network` (heavyweight repair entry, e.g.
    /// `.cnnd` load) and from `network_validator::validate_network` (every
    /// validate pass — so the pin layout is current when `validate_wires`
    /// type-checks the f wire and the arg wires). Idempotent: running it
    /// repeatedly with the same wires produces the same custom_node_type.
    ///
    /// Recurses into every HOF zone body so body-internal `apply` nodes get
    /// the same derived layout. The `f`-source is resolved across scopes via
    /// the threaded ancestor chain, so it works whether `f` is wired from a
    /// node in the same body (depth 0), a cross-scope capture (`NodeOutput`
    /// depth ≥ 1), or a zone-input reference (`ZoneInput` depth ≥ 1 — e.g.
    /// dragging the body's `element`/`acc` pin into `apply.f`). Without this,
    /// dragging a function-typed wire into an `apply` inside a zone produced
    /// no arg pins — the layout stayed collapsed to the bare `f` pin.
    pub fn update_apply_pin_layouts_for_network(&self, network: &mut NodeNetwork) {
        self.update_apply_pin_layouts_scoped(network, &[], &[], true);
    }

    /// Like [`Self::update_apply_pin_layouts_for_network`], but installs the
    /// derived layout with `refresh_args = false`, preserving the existing
    /// `arguments` vector **positionally** rather than rebuilding it by pin
    /// name. Used right after a `initialize_custom_node_types_for_network`
    /// re-init (closure⇄network conversion, body-undo restore): that re-init
    /// resets every `apply` to its bare `[f]` default, erasing the
    /// post-pass-derived arg-pin names, so a subsequent by-name rebuild would
    /// drop the (still-present) arg wires. Re-deriving the layout here with the
    /// args kept in place restores the names so the *next* ordinary
    /// (`refresh_args = true`) post-pass is a no-op. Safe because the caller's
    /// `arguments` already matches the intended arity (a consistent
    /// deserialized / copied graph).
    pub fn update_apply_pin_layouts_for_network_preserving_args(&self, network: &mut NodeNetwork) {
        self.update_apply_pin_layouts_scoped(network, &[], &[], false);
    }

    /// Scope-aware recursive worker for the apply post-pass. `ancestors` /
    /// `ancestor_hof_ids` describe `network`'s enclosing-zone chain using the
    /// same root-first indexing as `validate_zones_recursive` /
    /// `resolve_wire_source_type_scoped` (empty when `network` is top-level).
    /// `refresh_args` is forwarded to `set_custom_node_type` (see
    /// [`Self::update_apply_pin_layouts_for_network_preserving_args`]).
    fn update_apply_pin_layouts_scoped(
        &self,
        network: &mut NodeNetwork,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
        refresh_args: bool,
    ) {
        // Snapshot pass (immutable read).
        let apply_ids: Vec<u64> = network
            .nodes
            .iter()
            .filter_map(|(&id, n)| (n.node_type_name == "apply").then_some(id))
            .collect();
        let mut overrides: Vec<(u64, NodeType)> = Vec::new();
        for id in apply_ids {
            let Some(node) = network.nodes.get(&id) else {
                continue;
            };
            if let Some(custom) = self.compute_apply_custom_type_from_wired_f(
                node,
                network,
                ancestors,
                ancestor_hof_ids,
            ) {
                overrides.push((id, custom));
            }
        }
        // Install pass (mutation).
        for (id, custom) in overrides {
            if let Some(node) = network.nodes.get_mut(&id) {
                node.set_custom_node_type(Some(custom), refresh_args);
            }
        }

        // Recurse into zone bodies with the chain extended by this network and
        // the HOF id, so a body-internal `apply` whose `f` is a cross-scope
        // capture or zone-input reference resolves against the enclosing HOF.
        // Take-and-restore (not `zone_mut`) so `&*network` can serve as the
        // immediate-parent ancestor while the body is mutated — the same
        // borrow-split as `network_validator::validate_zones_recursive`.
        let hof_ids: Vec<u64> = network
            .nodes
            .iter()
            .filter_map(|(&id, n)| n.zone.is_some().then_some(id))
            .collect();
        for hof_id in hof_ids {
            let Some(mut body_arc) = network.nodes.get_mut(&hof_id).and_then(|n| n.zone.take())
            else {
                continue;
            };
            {
                let mut new_ancestors: Vec<&NodeNetwork> = ancestors.to_vec();
                new_ancestors.push(&*network);
                let mut new_hof_ids: Vec<u64> = ancestor_hof_ids.to_vec();
                new_hof_ids.push(hof_id);
                let body = std::sync::Arc::make_mut(&mut body_arc);
                self.update_apply_pin_layouts_scoped(
                    body,
                    &new_ancestors,
                    &new_hof_ids,
                    refresh_args,
                );
            }
            if let Some(node) = network.nodes.get_mut(&hof_id) {
                node.zone = Some(body_arc);
            }
        }
    }

    /// Computes the dynamic `custom_node_type` for an `apply` node whose `f`
    /// pin is wired, derived from the wired source's declared (canonical, flat)
    /// function type and the count of wired arg pins on this apply.
    ///
    /// Returns `Some(custom_type)` to install when:
    /// 1. The apply's `f` pin (argument index 0) carries an incoming wire.
    /// 2. The wired source's output pin resolves to a `Function(_)`.
    ///
    /// Returns `None` to fall back to today's `ApplyData`-driven layout when:
    /// - `f` is disconnected, or
    /// - the source type doesn't resolve (unresolved polymorphic upstream,
    ///   stale wire, cross-scope source with an incomplete ancestor chain), or
    /// - the source type is not a `Function`.
    ///
    /// The `f`-source is resolved across scopes via `ancestors` /
    /// `ancestor_hof_ids`, so it works for a local source (depth 0), a
    /// cross-scope capture (`NodeOutput` depth ≥ 1), or a zone-input
    /// reference (`ZoneInput` depth ≥ 1, e.g. the body's `element`/`acc` pin).
    ///
    /// Currying Phase 3, `doc/design_currying.md` §"`apply` semantics":
    /// - Number of arg pins `N = source's flat parameter_types.len()`.
    /// - Output pin type: `R` when all N args are wired (full evaluation),
    ///   else `Function(declared_params[k..], R)` canonicalized (partial).
    /// - Arg pin names: read from the source's pin names when available
    ///   (closure node's `zone_input_pins`; function-pin's `parameters`),
    ///   else generic `"arg0", "arg1", …`.
    ///
    /// The k-aware output type means the apply's output pin retypes as the
    /// user wires/unwires arg pins. Validation only allows a contiguous
    /// prefix of arg pins to be wired (see
    /// `network_validator::validate_zones_recursive`'s apply rule), so `k`
    /// is unambiguous; non-prefix wiring is an error but we still compute
    /// based on count to keep the output pin's type sensible while the user
    /// resolves the violation.
    fn compute_apply_custom_type_from_wired_f(
        &self,
        apply_node: &Node,
        network: &NodeNetwork,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
    ) -> Option<NodeType> {
        use crate::structure_designer::data_type::FunctionType;
        use crate::structure_designer::node_type::OutputPinDefinition;

        let base = self.built_in_node_types.get("apply")?;
        let f_arg = apply_node.arguments.first()?;
        let f_wire = f_arg.incoming_wires.first()?;
        // Resolve the f source's type across scopes — local (depth 0),
        // cross-scope capture (`NodeOutput` depth ≥ 1), or zone-input
        // reference (`ZoneInput` depth ≥ 1, e.g. dragging the body's
        // `element`/`acc` pin into apply.f). Returns `None` for abstract or
        // otherwise-unresolvable sources (including a cross-scope source whose
        // ancestor chain isn't available — e.g. the body is being processed in
        // isolation during repair; the top-level pass resolves it correctly).
        let src_type = self
            .resolve_wire_source_type_scoped(f_wire, network, ancestors, ancestor_hof_ids)?
            .data_type;
        let DataType::Function(src_ft) = src_type else {
            return None;
        };

        let total_arity = src_ft.parameter_types.len();
        let return_type = (*src_ft.output_type).clone();

        // Pin-name policy: preserve the *existing* parameter names of this
        // apply at overlapping indices, so `set_custom_node_type`'s by-name
        // wire preservation does not drop wires when the post-pass overrides
        // an ApplyData-driven layout (e.g. Map kind's "element" or Custom
        // kind's authored "x"/"lhs"). New pin slots that didn't exist in
        // the OLD layout get a stable `arg{i}` fallback.
        //
        // Source-derived names (read from `closure.zone_input_pins` /
        // function-pin source's parameters) are intentionally *not* used
        // here — they would be the right UX choice if we had stable
        // parameter IDs for wire preservation, but with `id: None` the
        // name change would drop a freshly wired arg every time. Keeping
        // OLD names is the conservative trade-off; the editor can show
        // source-authored names in a label overlay (Phase 5 UI work).
        let current_params: &[Parameter] = apply_node
            .custom_node_type
            .as_ref()
            .map(|nt| nt.parameters.as_slice())
            .unwrap_or(&base.parameters);

        let mut custom = base.clone();

        // External pins: f + N arg pins. The f-pin's declared type is
        // permanently `AnyFunction { leading_params: vec![] }` (set by
        // `ApplyData::calculate_custom_node_type` and inherited here via
        // `base.clone()`). The post-pass no longer rewrites it — the
        // standard `Function(_) → AnyFunction { vec![] }` compatibility rule
        // makes the f wire type-check on its own. See
        // `doc/design_function_pin_unification.md` (Phase B).
        let mut parameters = Vec::with_capacity(1 + total_arity);
        parameters.push(Parameter {
            id: None,
            name: "f".to_string(),
            data_type: DataType::AnyFunction {
                leading_params: vec![],
            },
        });
        for (i, param_ty) in src_ft.parameter_types.iter().enumerate() {
            // OLD index for arg{i} is at parameter slot `1 + i` (after `f`).
            let name = current_params
                .get(1 + i)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("arg{}", i));
            parameters.push(Parameter {
                id: None,
                name,
                data_type: param_ty.clone(),
            });
        }

        // k = count of the contiguous prefix of wired arg pins on THIS apply.
        // The prefix-only validation rule means k = total wired prefix; a
        // bad wiring is flagged separately but doesn't break this calc.
        let mut k: usize = 0;
        for i in 0..total_arity {
            let idx = 1 + i;
            match apply_node.arguments.get(idx) {
                Some(a) if !a.incoming_wires.is_empty() => k += 1,
                _ => break,
            }
        }

        // Output pin type: full eval ⇒ R; partial ⇒ Function(remaining, R).
        let output_type = if k >= total_arity {
            return_type
        } else {
            DataType::Function(FunctionType::new(
                src_ft.parameter_types[k..].to_vec(),
                return_type,
            ))
        };

        custom.parameters = parameters;
        custom.output_pins = OutputPinDefinition::single_fixed(output_type);

        Some(custom)
    }

    /// Top-level driver for the Currying Phase 4 `map` post-pass: for every
    /// `map` node in `network` whose `f` pin is wired to a resolvable `Function`
    /// source whose declared (canonical, flat) parameter list **starts with**
    /// the map's element type, override the node's `custom_node_type` so that
    /// `map`'s output pin type becomes `Iter[derived_output]` where
    /// `derived_output` is either `*src.output_type` (when the source's
    /// parameter list is just `[element_type]`) or
    /// `Function(tail, *src.output_type)` (when the source has extra params
    /// that absorb as partial-application tail).
    ///
    /// The HOF auto-partialization rule from `doc/design_currying.md` Phase 4:
    /// any `Function` source whose parameter list starts with `[element_type]`
    /// can flow into `map.f`; the per-element evaluation produces a partially-
    /// applied closure carrying that element and the remaining `tail`
    /// parameters. The zone-body pins (`zone_input_pins`, `zone_output_pins`)
    /// are intentionally left at `MapData`-driven values so that disconnecting
    /// `f` restores the user's inline-body shape cleanly.
    ///
    /// `map.f`'s declared type is permanently
    /// `AnyFunction { leading_params: [element_type] }` (set by
    /// `MapData::calculate_custom_node_type`) and is **not** rewritten here —
    /// the standard `Function(_) → AnyFunction { [element_type] }`
    /// compatibility rule (Phase A) handles wire type-checking against
    /// arbitrary higher-arity sources. See
    /// `doc/design_function_pin_unification.md` (Phase C).
    ///
    /// Called from `repair_node_network` (heavyweight repair entry, e.g.
    /// `.cnnd` load) and from `network_validator::validate_network` (every
    /// validate pass). Idempotent: running it on a steady state is a no-op.
    ///
    /// Recurses into HOF zone bodies (with the ancestor chain threaded), so a
    /// body-internal `map` whose `f` is wired — including from a cross-scope
    /// capture or a zone-input pin — derives its layout too.
    pub fn update_map_pin_layouts_for_network(&self, network: &mut NodeNetwork) {
        self.update_map_pin_layouts_scoped(network, &[], &[], true);
    }

    /// `refresh_args = false` counterpart of
    /// [`Self::update_map_pin_layouts_for_network`] — see
    /// [`Self::update_apply_pin_layouts_for_network_preserving_args`] for why
    /// the conversion / body-undo restore paths re-derive layouts without
    /// rebuilding the arguments vector.
    pub fn update_map_pin_layouts_for_network_preserving_args(&self, network: &mut NodeNetwork) {
        self.update_map_pin_layouts_scoped(network, &[], &[], false);
    }

    /// Scope-aware recursive worker for the map post-pass. See
    /// [`Self::update_apply_pin_layouts_scoped`] for the chain convention and
    /// the take-and-restore borrow-split rationale. `refresh_args` is forwarded
    /// to `set_custom_node_type`.
    fn update_map_pin_layouts_scoped(
        &self,
        network: &mut NodeNetwork,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
        refresh_args: bool,
    ) {
        // Snapshot pass (immutable read). Recompute *every* map node's
        // custom_node_type so disconnecting `f` restores the MapData-driven
        // layout cleanly — without this, an override installed by a previous
        // run would persist after the wire is gone. The MapData-driven default
        // is identical to what `populate_custom_node_type_cache` would produce,
        // so re-installing it on every revalidate is a no-op for nodes that
        // weren't overridden (`set_custom_node_type`'s by-name parameter
        // preservation keeps the existing arguments untouched).
        let map_ids: Vec<u64> = network
            .nodes
            .iter()
            .filter_map(|(&id, n)| (n.node_type_name == "map").then_some(id))
            .collect();
        let mut updates: Vec<(u64, NodeType)> = Vec::new();
        for id in map_ids {
            let Some(node) = network.nodes.get(&id) else {
                continue;
            };
            if let Some(custom) =
                self.compute_map_custom_type(node, network, ancestors, ancestor_hof_ids)
            {
                updates.push((id, custom));
            }
        }
        // Install pass (mutation).
        for (id, custom) in updates {
            if let Some(node) = network.nodes.get_mut(&id) {
                node.set_custom_node_type(Some(custom), refresh_args);
            }
        }

        // Recurse into zone bodies with the chain extended. Take-and-restore
        // so `&*network` can serve as the immediate-parent ancestor while the
        // body is mutated (see `update_apply_pin_layouts_scoped`).
        let hof_ids: Vec<u64> = network
            .nodes
            .iter()
            .filter_map(|(&id, n)| n.zone.is_some().then_some(id))
            .collect();
        for hof_id in hof_ids {
            let Some(mut body_arc) = network.nodes.get_mut(&hof_id).and_then(|n| n.zone.take())
            else {
                continue;
            };
            {
                let mut new_ancestors: Vec<&NodeNetwork> = ancestors.to_vec();
                new_ancestors.push(&*network);
                let mut new_hof_ids: Vec<u64> = ancestor_hof_ids.to_vec();
                new_hof_ids.push(hof_id);
                let body = std::sync::Arc::make_mut(&mut body_arc);
                self.update_map_pin_layouts_scoped(
                    body,
                    &new_ancestors,
                    &new_hof_ids,
                    refresh_args,
                );
            }
            if let Some(node) = network.nodes.get_mut(&hof_id) {
                node.zone = Some(body_arc);
            }
        }
    }

    /// Resolves the custom_node_type for a `map` node:
    /// - `f` wired and derivable (starts-with rule matches) → the derived
    ///   layout.
    /// - `f` disconnected → the MapData-driven default (so disconnect cleanly
    ///   restores the user's inline-body shape).
    /// - `f` wired but **unresolvable** (cross-scope source whose ancestor
    ///   chain isn't available in the current call, or a stale/abstract
    ///   source) → `None`, meaning "leave the existing layout untouched". This
    ///   keeps the body-recursion of `repair_node_network` (which processes
    ///   each body once more with no ancestor chain) from clobbering a derived
    ///   cross-scope layout that the top-level pass already installed.
    ///
    /// Returns `None` when the base `map` node type is missing or
    /// `calculate_custom_node_type` produces nothing.
    fn compute_map_custom_type(
        &self,
        map_node: &Node,
        network: &NodeNetwork,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
    ) -> Option<NodeType> {
        let base = self.built_in_node_types.get("map")?;
        let map_data_default = map_node.data.calculate_custom_node_type(base)?;
        if let Some(derived) = self.compute_map_custom_type_from_wired_f(
            map_node,
            network,
            &map_data_default,
            ancestors,
            ancestor_hof_ids,
        ) {
            return Some(derived);
        }
        // `f` wired but the source didn't resolve to a starts-with-compatible
        // Function in this scope context — keep the existing layout rather
        // than reverting to the MapData default (avoids the repair-recursion
        // clobber). Disconnected `f` falls through to the default.
        let f_wired = map_node
            .arguments
            .get(1)
            .map(|a| !a.incoming_wires.is_empty())
            .unwrap_or(false);
        if f_wired {
            None
        } else {
            Some(map_data_default)
        }
    }

    /// Computes the dynamic `custom_node_type` for a `map` node whose `f` pin
    /// is wired with a starts-with-compatible `Function` source. Returns
    /// `None` to fall back to today's `MapData`-driven layout when:
    /// - `f` is disconnected, or
    /// - the source type doesn't resolve (unresolved polymorphic upstream,
    ///   stale wire, etc.), or
    /// - the source type is not a `Function`, or
    /// - the source's parameter list does not start with `[element_type]`.
    ///
    /// Currying Phase 4, `doc/design_currying.md` §"HOF auto-partialization
    /// (`map`)". The derived layout:
    /// - `xs` pin: `Iter[element_type]` (unchanged from `MapData`-driven).
    /// - `f` pin: **left untouched** — its declared type is permanently
    ///   `AnyFunction { leading_params: [element_type] }` (set by
    ///   `MapData::calculate_custom_node_type`); the Phase A
    ///   `Function(_) → AnyFunction { … }` compatibility rule handles
    ///   structural wire checking against the source. See
    ///   `doc/design_function_pin_unification.md` (Phase C).
    /// - Output pin: `Iter[derived_output]` where `derived_output` is
    ///   `Function(tail, R)` for a non-empty tail (canonicalized) or `R` when
    ///   the tail is empty.
    /// - `zone_input_pins`/`zone_output_pins`: unchanged from the existing
    ///   `MapData`-driven layout (so disconnecting `f` restores cleanly).
    fn compute_map_custom_type_from_wired_f(
        &self,
        map_node: &Node,
        network: &NodeNetwork,
        map_data_default: &NodeType,
        ancestors: &[&NodeNetwork],
        ancestor_hof_ids: &[u64],
    ) -> Option<NodeType> {
        use crate::structure_designer::data_type::FunctionType;
        use crate::structure_designer::node_type::OutputPinDefinition;

        // map.f is parameter index 1. Resolve its source across scopes — local
        // (depth 0), cross-scope capture, or zone-input reference — so a `map`
        // inside a zone body whose `f` is fed by an outer `element`/`acc` pin
        // derives its layout too.
        let f_arg = map_node.arguments.get(1)?;
        let f_wire = f_arg.incoming_wires.first()?;
        let src_type = self
            .resolve_wire_source_type_scoped(f_wire, network, ancestors, ancestor_hof_ids)?
            .data_type;
        let DataType::Function(src_ft) = src_type else {
            return None;
        };

        // Starts-with rule: the source's parameter list must begin with
        // `[element_type]`. element_type is the MapData-driven f pin's
        // `AnyFunction` leading-param entry — what
        // `calculate_custom_node_type` installs as
        // `AnyFunction { leading_params: vec![input_type] }`.
        let f_pin_type = &map_data_default.parameters.get(1)?.data_type;
        let DataType::AnyFunction { leading_params } = f_pin_type else {
            return None;
        };
        let element_type = leading_params.first()?.clone();
        if src_ft.parameter_types.first() != Some(&element_type) {
            return None;
        }

        // Derive the output type. Tail = params after the leading element_type.
        let tail = &src_ft.parameter_types[1..];
        let return_type = (*src_ft.output_type).clone();
        let derived_output = if tail.is_empty() {
            return_type
        } else {
            DataType::Function(FunctionType::new(tail.to_vec(), return_type))
        };

        // Build the override on top of the MapData-driven default. The f-pin
        // declared type stays at `AnyFunction { leading_params: [element] }` —
        // Phase C no longer rewrites it. Only the output pin is updated.
        let mut custom = map_data_default.clone();
        // Output pin: Iter[derived_output].
        custom.output_pins =
            OutputPinDefinition::single_fixed(DataType::Iterator(Box::new(derived_output)));

        Some(custom)
    }

    /// Repairs a node network by ensuring all nodes have the correct number of arguments
    /// to match their node type parameters. Adds empty arguments if a node has fewer
    /// arguments than its node type requires.
    ///
    /// Also recurses into HOF nodes' owned zone bodies, applying the same repairs and
    /// dropping body wires whose `ZoneInput` source pin index has fallen out of range
    /// or whose source pin's declared type is no longer compatible with the
    /// destination's declared type (the typical trigger is a `map`/`filter`/`fold`'s
    /// `input_type` or `output_type` changing — body wires that referenced the now-
    /// retyped pin get disconnected, matching the existing record-type-def repair
    /// pattern).
    ///
    /// # Parameters
    /// * `network` - A mutable reference to the node network to repair
    pub fn repair_node_network(&self, network: &mut NodeNetwork) {
        // Refresh every node's custom_node_type FIRST so the parameter /
        // output-pin counts derived from per-node data and the registry are
        // visible to the arg-count and wire-pin repair passes below.
        //
        // Why every node, not just record nodes: a record-def rename rewrites
        // `Record(Named(old))` to `Record(Named(new))` inside per-node data
        // for *every* dynamic-arg node (parameter, expr, map, filter, fold,
        // foreach, sequence, array_*, …). Their cached `custom_node_type`
        // still carries the stale `Named(old)` reference until we re-derive
        // it from data here. Skipping non-record nodes used to leave them in
        // a state where a subsequent eval indexes `parameters[0]` on a base
        // type with `parameters: vec![]` and panics — see
        // `tests/structure_designer/record_types_phase2_test.rs::rename_record_type_def_repopulates_sequence_custom_node_type`.
        //
        // refresh_args=true relies on the existing cache's parameter
        // names/IDs to preserve wires when the structure is unchanged
        // (the common rename case); structural changes (field add/remove on
        // delete/update) fall back to ID-then-name matching for surviving
        // wires. Nodes whose `calculate_custom_node_type` returns None are
        // unaffected — they never carry a custom cache.
        for node in network.nodes.values_mut() {
            Self::populate_custom_node_type_cache_with_types(
                &self.built_in_node_types,
                &self.record_type_defs,
                &self.built_in_record_type_defs,
                node,
                true,
            );
        }

        // Currying Phase 3 (`doc/design_currying.md`): `apply` nodes whose
        // `f` (Function) pin is wired derive their arg-pin enumeration and
        // output pin type from the wired source's declared (canonical,
        // flat) function type — overriding the ApplyData-driven layout
        // produced by the populate loop above. See
        // `update_apply_pin_layouts_for_network` for the borrow-split
        // snapshot + install pattern.
        self.update_apply_pin_layouts_for_network(network);

        // Currying Phase 4 (`doc/design_currying.md`, §"HOF auto-partialization
        // (`map`)"): `map` nodes whose `f` (Function) pin is wired with a
        // starts-with-compatible higher-arity source absorb the excess
        // parameters as partial-application tail; `output_type` is derived
        // from `f`. Must run AFTER the apply post-pass so an `apply` source
        // feeding `map.f` has its output type resolved against its updated
        // arg-pin layout first.
        self.update_map_pin_layouts_for_network(network);

        let node_ids: HashSet<u64> = network.nodes.keys().copied().collect();

        // Build a map of node_id -> output_pin_count for wire validation
        let pin_counts: HashMap<u64, usize> = network
            .nodes
            .iter()
            .filter_map(|(&nid, n)| {
                self.get_node_type_for_node(n)
                    .map(|nt| (nid, nt.output_pin_count()))
            })
            .collect();

        // Iterate through all nodes in the network
        for node in network.nodes.values_mut() {
            // Get the node type for this node
            if let Some(node_type) = self.get_node_type_for_node(node) {
                // Phase 2 invariant: only zone-bearing types may carry a
                // populated `zone` / `zone_output_arguments`. Cheap no-op in
                // release; loud panic in debug.
                node.debug_assert_zone_consistency(node_type);

                let required_params = node_type.parameters.len();
                let current_args = node.arguments.len();

                // If the node has fewer arguments than required parameters, add empty arguments
                if current_args < required_params {
                    let missing_args = required_params - current_args;
                    for _ in 0..missing_args {
                        node.arguments.push(Argument::new());
                    }
                }
            }

            // Remove obviously invalid wire entries to avoid loading dangerous state.
            // - Drop connections referencing non-existent source nodes
            // - Drop connections with unsupported output pin indices
            //   (-1=function pin, 0..N-1=result output pins based on the source node's type)
            for argument in node.arguments.iter_mut() {
                argument.incoming_wires.retain(|wire| {
                    let Some((source_node_id, output_pin_index)) = wire.as_legacy_pair() else {
                        // Non-legacy wires (zone-input or cross-scope) are
                        // not validated here — they live inside bodies and
                        // their resolution depends on the ancestor chain,
                        // which is handled by the zone-body repair pass
                        // below (Phase 6).
                        return true;
                    };
                    if !node_ids.contains(&source_node_id) {
                        return false;
                    }
                    if output_pin_index == -1 {
                        return true;
                    }
                    if let Some(&count) = pin_counts.get(&source_node_id) {
                        (output_pin_index as usize) < count
                    } else {
                        // Unknown type — keep wire, let validator catch it
                        true
                    }
                });
            }
        }

        // Zone-body repair pass. For every HOF node in this network, walk
        // its owned body and drop body-internal wires whose `ZoneInput`
        // source pin has fallen out of range or whose declared source type
        // is no longer convertible to the body destination's declared type.
        // Recurse so nested zones inside the body are repaired too.
        //
        // The HOF's `zone_input_pins` were just refreshed at the top of
        // this function (via `populate_custom_node_type_cache_with_types`),
        // so the pin layout we read here is the up-to-date one. The
        // body's `zone_output_arguments` count was likewise resized by
        // `Node::ensure_zone_init`, so wires terminating at no-longer-
        // existing zone-output pins have already been truncated.
        let hof_ids: Vec<u64> = network
            .nodes
            .iter()
            .filter_map(|(&id, n)| if n.zone.is_some() { Some(id) } else { None })
            .collect();

        for hof_id in hof_ids {
            // Snapshot the HOF's zone-input pin types — body wires that
            // read `ZoneInput { pin_index = i }` must be compatible with
            // `zone_input_pins[i]`'s declared type.
            let zone_input_pin_types: Vec<Option<DataType>> = network
                .nodes
                .get(&hof_id)
                .and_then(|n| self.get_node_type_for_node(n))
                .map(|nt| {
                    nt.zone_input_pins
                        .iter()
                        .map(|p| p.fixed_type().cloned())
                        .collect()
                })
                .unwrap_or_default();

            // Mutably borrow the body via `zone_mut` (CoW via Arc::make_mut).
            if let Some(node) = network.nodes.get_mut(&hof_id) {
                if let Some(body) = node.zone_mut() {
                    self.repair_zone_body(body, hof_id, &zone_input_pin_types);
                }
            }
        }
    }

    /// Repair body wires inside `body` (owned by HOF `hof_id`). Drops
    /// `ZoneInput { pin_index }` wires referencing the HOF whose pin index
    /// is out of range or whose declared source type isn't convertible to
    /// the body destination's declared type. Then recurses via
    /// `repair_node_network` so nested zones are repaired in turn.
    fn repair_zone_body(
        &self,
        body: &mut NodeNetwork,
        hof_id: u64,
        zone_input_pin_types: &[Option<DataType>],
    ) {
        // First-level repair: drop now-invalid body wires sourced from this
        // HOF's zone-input pins. We need the destination's declared type to
        // run the compatibility check, so collect (node_id, arg_index,
        // dest_data_type) tuples up front.
        let body_nodes: Vec<u64> = body.nodes.keys().copied().collect();

        for body_node_id in body_nodes {
            // Snapshot the per-argument dest type so we don't borrow body
            // mutably and immutably at the same time.
            let dest_types: Vec<DataType> = {
                let body_node = body.nodes.get(&body_node_id).unwrap();
                let nt = self.get_node_type_for_node(body_node);
                let num_args = body_node.arguments.len();
                (0..num_args)
                    .map(|i| {
                        nt.map(|t| {
                            t.parameters
                                .get(i)
                                .map(|p| p.data_type.clone())
                                .unwrap_or(DataType::None)
                        })
                        .unwrap_or(DataType::None)
                    })
                    .collect()
            };

            let body_node_mut = body.nodes.get_mut(&body_node_id).unwrap();
            for (arg_index, dest_type) in dest_types.iter().enumerate() {
                if let Some(arg) = body_node_mut.arguments.get_mut(arg_index) {
                    arg.incoming_wires.retain(|wire| {
                        // Only repair ZoneInput wires that point at THIS
                        // hof_id at depth 1 — that's the only case we have
                        // local knowledge to repair. Deeper ZoneInput
                        // references and capture wires live across scopes
                        // and would need an ancestor chain we don't have
                        // here; validation surfaces them as errors.
                        if wire.source_node_id != hof_id || wire.source_scope_depth != 1 {
                            return true;
                        }
                        let SourcePin::ZoneInput { pin_index } = wire.source_pin else {
                            return true;
                        };
                        // Out-of-range pin index — drop.
                        let Some(maybe_src_type) = zone_input_pin_types.get(pin_index) else {
                            return false;
                        };
                        // Source type unknown (HOF type unresolved or
                        // declares an abstract zone-input) — keep, let
                        // validator surface any deeper issue.
                        let Some(src_type) = maybe_src_type.as_ref() else {
                            return true;
                        };
                        DataType::can_be_converted_to(src_type, dest_type, self)
                    });
                }
            }
        }

        // Now recurse — bodies can themselves contain HOFs whose own
        // zone state may have shifted. `repair_node_network` handles arg
        // counts, dangling wire cleanup, and another level of zone-body
        // repair.
        self.repair_node_network(body);
    }

    /// Computes the transitive closure of node network dependencies.
    ///
    /// Given a vector of node network names, returns a vector containing all the networks
    /// they depend on (directly and indirectly), including the original networks.
    ///
    /// A node network 'A' depends on 'B' if there is a node in 'A' with node_type_name 'B'.
    ///
    /// # Arguments
    /// * `network_names` - The initial set of node network names
    ///
    /// # Returns
    /// A vector containing all networks in the transitive closure of dependencies
    pub fn compute_transitive_dependencies(&self, network_names: &[String]) -> Vec<String> {
        let mut result = HashSet::new();
        let mut visited = HashSet::new();

        // Start DFS from each requested network
        for network_name in network_names {
            self.dfs_dependencies(network_name, &mut result, &mut visited);
        }

        // Convert to sorted vector for deterministic output
        let mut result_vec: Vec<String> = result.into_iter().collect();
        result_vec.sort();
        result_vec
    }

    /// Depth-first search to find all dependencies of a node network
    fn dfs_dependencies(
        &self,
        network_name: &str,
        result: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) {
        // Avoid infinite recursion in case of circular dependencies
        if visited.contains(network_name) {
            return;
        }
        visited.insert(network_name.to_string());

        // Add this network to the result
        result.insert(network_name.to_string());

        // Find the network in our registry
        if let Some(network) = self.node_networks.get(network_name) {
            // Examine every node in this network, including nodes inside HOF
            // zone bodies — a body-internal node may reference another
            // user-defined network just like a top-level one.
            let mut referenced: Vec<String> = Vec::new();
            crate::structure_designer::node_network::walk_all_nodes(network, &mut |node| {
                if self.node_networks.contains_key(&node.node_type_name) {
                    referenced.push(node.node_type_name.clone());
                }
            });
            for name in referenced {
                self.dfs_dependencies(&name, result, visited);
            }
        }

        // Remove from visited to allow revisiting in different paths
        // (This is safe because we use the result set to track what we've already processed)
        visited.remove(network_name);
    }

    /// Returns all node network names in topological order where dependencies come first.
    /// Networks with no dependencies appear first, networks that depend on others appear later.
    /// This ensures that when validating in this order, dependencies are validated before their dependents.
    ///
    /// # Returns
    /// A vector of all node network names in dependency-first order
    pub fn get_networks_in_dependency_order(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_mark = HashSet::new();

        // Get all network names
        let network_names: Vec<String> = self.node_networks.keys().cloned().collect();

        // Visit each network (DFS post-order traversal)
        for network_name in &network_names {
            if !visited.contains(network_name) {
                self.dfs_topological_sort(network_name, &mut result, &mut visited, &mut temp_mark);
            }
        }

        result
    }

    /// DFS helper for topological sort. Uses post-order traversal to ensure dependencies come before dependents.
    fn dfs_topological_sort(
        &self,
        network_name: &str,
        result: &mut Vec<String>,
        visited: &mut HashSet<String>,
        temp_mark: &mut HashSet<String>,
    ) {
        // Detect cycles (should not happen in valid designs)
        if temp_mark.contains(network_name) {
            return; // Circular dependency detected, skip
        }

        // Already processed
        if visited.contains(network_name) {
            return;
        }

        // Mark as temporarily visited (for cycle detection)
        temp_mark.insert(network_name.to_string());

        // Find dependencies and visit them first. Recurse into HOF zone
        // bodies so a body-internal reference to another user-defined network
        // pulls that network into the topological order.
        if let Some(network) = self.node_networks.get(network_name) {
            let mut referenced: Vec<String> = Vec::new();
            crate::structure_designer::node_network::walk_all_nodes(network, &mut |node| {
                if self.node_networks.contains_key(&node.node_type_name) {
                    referenced.push(node.node_type_name.clone());
                }
            });
            for name in referenced {
                self.dfs_topological_sort(&name, result, visited, temp_mark);
            }
        }

        // Remove temporary mark
        temp_mark.remove(network_name);

        // Mark as visited
        visited.insert(network_name.to_string());

        // Add to result AFTER visiting all dependencies (post-order)
        result.push(network_name.to_string());
    }
}

// ---------------------------------------------------------------------------
// Record type def helpers (free functions, not methods on NodeTypeRegistry).
// See `doc/design_record_types.md` Phase 2 for the design.
// ---------------------------------------------------------------------------

/// Validates that field names within `fields` are distinct. Returns
/// `DuplicateField` on the first repeated name.
fn validate_distinct_fields(
    def_name: &str,
    fields: &[(String, DataType)],
) -> Result<(), RecordTypeDefError> {
    let mut seen: HashSet<&str> = HashSet::new();
    for (name, _) in fields {
        if !seen.insert(name.as_str()) {
            return Err(RecordTypeDefError::DuplicateField(
                def_name.to_string(),
                name.clone(),
            ));
        }
    }
    Ok(())
}

/// Collects every `RecordType::Named(N)` reference reachable from a field
/// list. Recurses through `Array`, `Function`, and nested `Record::Anonymous`
/// shapes; `Record::Named` references are leaves (the def itself is followed
/// by `dfs_cycle_check`).
fn collect_named_record_refs(fields: &[(String, DataType)]) -> Vec<String> {
    let mut refs = Vec::new();
    for (_, ty) in fields {
        collect_named_record_refs_in_type(ty, &mut refs);
    }
    refs
}

fn collect_named_record_refs_in_type(t: &DataType, out: &mut Vec<String>) {
    match t {
        DataType::Array(inner) => collect_named_record_refs_in_type(inner, out),
        DataType::Function(func) => {
            for p in &func.parameter_types {
                collect_named_record_refs_in_type(p, out);
            }
            collect_named_record_refs_in_type(&func.output_type, out);
        }
        DataType::AnyFunction { leading_params } => {
            for p in leading_params {
                collect_named_record_refs_in_type(p, out);
            }
        }
        DataType::Record(RecordType::Named(name)) => out.push(name.clone()),
        DataType::Record(RecordType::Anonymous(fs)) => {
            for (_, ty) in fs {
                collect_named_record_refs_in_type(ty, out);
            }
        }
        _ => {}
    }
}

/// Returns `true` if a DFS from `current` (a referenced record name) revisits
/// `def_name` (the def being validated). `path` accumulates the chain of names
/// for error reporting.
fn dfs_cycle_check(
    registry: &NodeTypeRegistry,
    def_name: &str,
    current: &str,
    path: &mut Vec<String>,
    visited: &mut HashSet<String>,
) -> bool {
    if current == def_name {
        return true;
    }
    if !visited.insert(current.to_string()) {
        return false;
    }
    // Built-in defs are visited too: they may contain `Named` references to
    // other built-ins or (defensively) to user defs. Built-ins themselves
    // never reference the def-being-validated (they're added before any user
    // def exists), so cycles cannot reach back via a built-in — but the walk
    // is still well-defined.
    let Some(def) = registry.lookup_record_type_def(current) else {
        // Dangling reference — ignore for cycle detection. Validation will
        // surface dangling refs separately.
        return false;
    };
    for r in collect_named_record_refs(&def.fields) {
        path.push(r.clone());
        if dfs_cycle_check(registry, def_name, &r, path, visited) {
            return true;
        }
        path.pop();
    }
    false
}

/// Walks every `DataType` reachable through the registry — every network's
/// node-type signature (parameters, output pins), every node's per-data-type
/// field, and every existing record def's field types — and rewrites
/// `RecordType::Named(old_name)` to `RecordType::Named(new_name)`.
///
/// The walker is feature-complete the moment Phase 3 lands its
/// `record_construct` / `record_destructure` / `product` nodes (which carry a
/// bare-name `schema` / `target` `String` property). Adding the corresponding
/// downcasts to those node-data types is the only change required there.
fn rewrite_record_name_in_registry(
    registry: &mut NodeTypeRegistry,
    old_name: &str,
    new_name: &str,
) {
    use crate::structure_designer::nodes::array_append::ArrayAppendData;
    use crate::structure_designer::nodes::array_at::ArrayAtData;
    use crate::structure_designer::nodes::array_concat::ArrayConcatData;
    use crate::structure_designer::nodes::array_len::ArrayLenData;
    use crate::structure_designer::nodes::expr::ExprData;
    use crate::structure_designer::nodes::filter::FilterData;
    use crate::structure_designer::nodes::fold::FoldData;
    use crate::structure_designer::nodes::foreach::ForeachData;
    use crate::structure_designer::nodes::map::MapData;
    use crate::structure_designer::nodes::parameter::ParameterData;
    use crate::structure_designer::nodes::product::ProductData;
    use crate::structure_designer::nodes::record_construct::RecordConstructData;
    use crate::structure_designer::nodes::record_destructure::RecordDestructureData;
    use crate::structure_designer::nodes::sequence::SequenceData;

    let mut rename = |name: &mut String| {
        if name == old_name {
            *name = new_name.to_string();
        }
    };

    // Walk every record def's fields too — `Box = { p: Record(Old) }` should
    // see the rename. The def being renamed itself is updated by the caller.
    for def in registry.record_type_defs.values_mut() {
        for (_, ty) in def.fields.iter_mut() {
            walk_data_type_record_names_mut(ty, &mut rename);
        }
    }

    for network in registry.node_networks.values_mut() {
        // Custom-network signature: parameter types and output pin types.
        for param in network.node_type.parameters.iter_mut() {
            walk_data_type_record_names_mut(&mut param.data_type, &mut rename);
        }
        for pin in network.node_type.output_pins.iter_mut() {
            if let crate::structure_designer::node_type::PinOutputType::Fixed(t) =
                &mut pin.data_type
            {
                walk_data_type_record_names_mut(t, &mut rename);
            }
        }

        // Recurse into HOF zone bodies — a body-internal `expr` / `map` / `record_*`
        // node may carry the renamed `Named` reference in its per-node data type
        // fields just like a top-level one.
        crate::structure_designer::node_network::walk_all_nodes_mut(network, &mut |node| {
            // Per-node data containers that embed a DataType.
            let data: &mut dyn crate::structure_designer::node_data::NodeData = node.data.as_mut();
            if let Some(d) = data.as_any_mut().downcast_mut::<ParameterData>() {
                walk_data_type_record_names_mut(&mut d.data_type, &mut rename);
                // Refresh the cached display string so save round-trips agree
                // with the in-memory type.
                if d.data_type_str.is_some() {
                    d.data_type_str = Some(d.data_type.to_string());
                }
            } else if let Some(d) = data.as_any_mut().downcast_mut::<ExprData>() {
                for p in d.parameters.iter_mut() {
                    walk_data_type_record_names_mut(&mut p.data_type, &mut rename);
                    if p.data_type_str.is_some() {
                        p.data_type_str = Some(p.data_type.to_string());
                    }
                }
                if let Some(out) = d.output_type.as_mut() {
                    walk_data_type_record_names_mut(out, &mut rename);
                }
            } else if let Some(d) = data.as_any_mut().downcast_mut::<MapData>() {
                walk_data_type_record_names_mut(&mut d.input_type, &mut rename);
                walk_data_type_record_names_mut(&mut d.output_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<SequenceData>() {
                walk_data_type_record_names_mut(&mut d.element_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<FilterData>() {
                walk_data_type_record_names_mut(&mut d.element_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<FoldData>() {
                walk_data_type_record_names_mut(&mut d.element_type, &mut rename);
                walk_data_type_record_names_mut(&mut d.accumulator_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<ForeachData>() {
                walk_data_type_record_names_mut(&mut d.input_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<ArrayAtData>() {
                walk_data_type_record_names_mut(&mut d.element_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<ArrayAppendData>() {
                walk_data_type_record_names_mut(&mut d.element_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<ArrayConcatData>() {
                walk_data_type_record_names_mut(&mut d.element_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<ArrayLenData>() {
                walk_data_type_record_names_mut(&mut d.element_type, &mut rename);
            } else if let Some(d) = data.as_any_mut().downcast_mut::<RecordConstructData>() {
                // `schema` is a bare record-def name; rewrite if it matches.
                if d.schema == old_name {
                    d.schema = new_name.to_string();
                }
            } else if let Some(d) = data.as_any_mut().downcast_mut::<RecordDestructureData>() {
                if d.schema == old_name {
                    d.schema = new_name.to_string();
                }
            } else if let Some(d) = data.as_any_mut().downcast_mut::<ProductData>() {
                // `target` is a bare record-def name; rewrite if it matches.
                if d.target == old_name {
                    d.target = new_name.to_string();
                }
            }

            // Cached `custom_node_type` is regenerated on next type-resolution
            // call from the (now-renamed) inputs; clear it defensively so any
            // stale `Named(old)` reference is not observable in the meantime.
            node.custom_node_type = None;
        });
    }
}

/// Validates the entire registry: re-runs the cycle check on every record def
/// and reports every dangling `RecordType::Named(N)` reference (i.e., names
/// referenced from a record def's fields but missing from `record_type_defs`).
///
/// This is intended for on-load (post-deserialize) defense against hand-edited
/// files that smuggle in cycles or dangling refs. It does **not** walk node
/// networks for dangling references — those surface naturally during the
/// per-network validation pass that runs after load.
///
/// Returns a list of error strings; empty when the registry is consistent.
pub fn validate_record_type_defs(registry: &NodeTypeRegistry) -> Vec<String> {
    let mut errors = Vec::new();

    // Cycle check: walk each def's references through the rest of the
    // registry and look for a path back to the def itself.
    for (name, def) in &registry.record_type_defs {
        if let Err(RecordTypeDefError::CycleDetected { description }) =
            registry.check_no_cycle(name, &def.fields)
        {
            errors.push(format!("record type def {}", description));
        }
    }

    // Dangling reference check: every `Named(N)` inside any def's fields must
    // point at an existing record def in the registry. Built-in defs are
    // resolved through `lookup_record_type_def`, so a user def referencing
    // a built-in (e.g. `ElementMapping`) is not dangling.
    for (name, def) in &registry.record_type_defs {
        for r in collect_named_record_refs(&def.fields) {
            if registry.lookup_record_type_def(&r).is_none() {
                errors.push(format!(
                    "record type def '{}' has dangling reference to '{}'",
                    name, r
                ));
            }
        }
    }

    errors
}

/// Pure static-pin compatibility check for the drag-aware add-node popup.
///
/// `FromOutput`: the user dragged from an output pin of `source_type`; we
/// want a node type that has at least one input pin accepting `source_type`.
/// `FromInput`: the user dragged from an input pin of `source_type`; we
/// want a node type whose pin-0 output can be converted to `source_type`.
///
/// This is exactly the predicate `get_compatible_node_types` used before
/// drag-aware adapters were introduced. The same helper is run at create
/// time inside `StructureDesigner::add_node` to verify an adapter's claim
/// before adopting its output, so over-promising adapters are silently
/// dropped to default data. See `doc/design_drag_aware_add_node.md`.
pub fn static_match(
    node_type: &NodeType,
    source_type: &DataType,
    direction: crate::structure_designer::node_data::DragDirection,
    registry: &NodeTypeRegistry,
) -> bool {
    use crate::structure_designer::node_data::DragDirection;
    match direction {
        DragDirection::FromOutput => node_type
            .parameters
            .iter()
            .any(|param| DataType::can_be_converted_to(source_type, &param.data_type, registry)),
        DragDirection::FromInput => {
            DataType::can_be_converted_to(node_type.output_type(), source_type, registry)
        }
    }
}

/// Like `static_match`, but uses `DataType::can_be_converted_to_strict_no_broadcast`
/// — rejects matches that only land via the `S → Array[T]` or `S → Iter[T]`
/// scalar broadcast rules.
///
/// Used at the Stage-2 adapter-verification site in
/// `get_compatible_node_types` and the mirror site in
/// `StructureDesigner::add_node_with_drag_source`. Stage-1 statically-typed
/// candidates still use the permissive `static_match` — the node author
/// declared the collection pin, so broadcasting into it is a legitimate
/// type-system convenience. The strict variant kicks in only after an
/// adapter has shapeshifted the node, where a broadcast-only match would
/// amount to silently wrapping the user's one value in a singleton
/// collection. See `doc/design_drag_aware_add_node.md`
/// §"Asymmetric verification".
pub fn static_match_strict(
    node_type: &NodeType,
    source_type: &DataType,
    direction: crate::structure_designer::node_data::DragDirection,
    registry: &NodeTypeRegistry,
) -> bool {
    use crate::structure_designer::node_data::DragDirection;
    match direction {
        DragDirection::FromOutput => node_type.parameters.iter().any(|param| {
            DataType::can_be_converted_to_strict_no_broadcast(
                source_type,
                &param.data_type,
                registry,
            )
        }),
        DragDirection::FromInput => DataType::can_be_converted_to_strict_no_broadcast(
            node_type.output_type(),
            source_type,
            registry,
        ),
    }
}
