//! View builders: domain state → Dart-facing view models.
//!
//! These five functions used to be methods on `NodeTypeRegistry` and
//! `StructureDesigner`. They are presentation logic that happened to be written
//! in the domain layer for convenience, and they were the *only* genuine part of
//! the `structure_designer → api` cycle — every other reference was a type that
//! belonged in the domain all along (D9). D10 of
//! `doc/design_rust_crate_split.md` settles the direction: **the view-builders
//! move up, the transport shapes stay up.** The test is what a type means, not
//! which direction removes more references.
//!
//! They take `&NodeTypeRegistry` / `&StructureDesigner` instead of `&self` and
//! are otherwise unchanged. No new accessors were needed: every field they touch
//! (`built_in_node_types`, `node_networks`, `eval_error_snapshots`,
//! `active_node_network_name`) is already `pub`.
//!
//! **This module is deliberately absent from `flutter_rust_bridge.yaml`'s
//! `rust_input`**, exactly like `api::api_common`. Its `pub fn`s take domain
//! types, and every `pub fn` in a scanned namespace becomes a Dart API — which
//! would drag `NodeTypeRegistry` into codegen as an opaque handle. The thin
//! `#[frb(sync)]` wrappers that Dart actually calls live in
//! `structure_designer_api.rs`.

use crate::api::structure_designer::structure_designer_api_types::{
    APIErrorRootCause, APIErrorSource, APINetworkWithValidationErrors, APINodeCategoryView,
    APINodeTypeView, APIValidationError,
};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::eval_errors::RootCauseRef;
use crate::structure_designer::node_type::NodeTypeCategory;
use crate::structure_designer::node_type_registry::{
    NodeTypeRegistry, allowed_in_zone_body, static_match, static_match_strict,
};
use crate::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashMap;

/// The palette's category order. Shared by both node-type view builders so the
/// add-node popup and the drag-aware popup can never disagree.
const ORDERED_CATEGORIES: [NodeTypeCategory; 7] = [
    NodeTypeCategory::Annotation,
    NodeTypeCategory::MathAndProgramming,
    NodeTypeCategory::Geometry2D,
    NodeTypeCategory::Geometry3D,
    NodeTypeCategory::AtomicStructure,
    NodeTypeCategory::OtherBuiltin,
    NodeTypeCategory::Custom,
];

/// Sorts each category's nodes by name and emits the categories in
/// `ORDERED_CATEGORIES` order, dropping empty ones.
fn group_into_category_views(
    mut category_map: HashMap<NodeTypeCategory, Vec<APINodeTypeView>>,
) -> Vec<APINodeCategoryView> {
    for nodes in category_map.values_mut() {
        nodes.sort_by(|a, b| a.name.cmp(&b.name));
    }

    let mut result: Vec<APINodeCategoryView> = Vec::new();
    for category in ORDERED_CATEGORIES {
        if let Some(nodes) = category_map.get(&category)
            && !nodes.is_empty()
        {
            result.push(APINodeCategoryView {
                category: category.into(),
                nodes: nodes.clone(),
            });
        }
    }
    result
}

/// Returns node types that have at least one pin compatible with the given source type.
///
/// - When `dragging_from_output` is true: find nodes with compatible INPUT pins
///   (any input that accepts the source type)
/// - When `dragging_from_output` is false: find nodes with compatible OUTPUT pins
///   (output can be converted to the source type)
pub fn get_compatible_node_types(
    registry: &NodeTypeRegistry,
    source_type: &DataType,
    dragging_from_output: bool,
) -> Vec<APINodeCategoryView> {
    let direction = if dragging_from_output {
        crate::structure_designer::node_data::DragDirection::FromOutput
    } else {
        crate::structure_designer::node_data::DragDirection::FromInput
    };

    // Create iterator of (node_type, category) for all public nodes
    let built_in_iter = registry
        .built_in_node_types
        .values()
        .filter(|nt| nt.public)
        .map(|nt| (nt, nt.category.clone()));

    let custom_iter = registry
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
    let all_views: Vec<(NodeTypeCategory, APINodeTypeView)> = built_in_iter
        .chain(custom_iter)
        .filter(|(node_type, _)| {
            if static_match(node_type, source_type, direction, registry) {
                return true;
            }
            let default_data = (node_type.node_data_creator)();
            let Some(adapted) =
                default_data.adapt_for_drag_source(source_type, direction, registry)
            else {
                return false;
            };
            let resolved = registry.resolve_drag_candidate_type(node_type, adapted.as_ref());
            static_match_strict(&resolved, source_type, direction, registry)
        })
        .map(|(node_type, category)| {
            (
                category.clone(),
                APINodeTypeView {
                    name: node_type.name.clone(),
                    description: node_type.description.clone(),
                    summary: node_type.summary.clone(),
                    category: category.into(),
                    allowed_in_zone_body: allowed_in_zone_body(&node_type.name),
                },
            )
        })
        .collect();

    // Group by category
    let mut category_map: HashMap<NodeTypeCategory, Vec<APINodeTypeView>> = HashMap::new();
    for (category, view) in all_views {
        category_map.entry(category).or_default().push(view);
    }

    group_into_category_views(category_map)
}

/// Retrieves views of all public node types available to users, grouped by category.
/// Only built-in node types can be non-public; all node networks are considered public.
pub fn get_node_type_views(registry: &NodeTypeRegistry) -> Vec<APINodeCategoryView> {
    // Collect all node views with their (domain) categories
    let mut all_views: Vec<(NodeTypeCategory, APINodeTypeView)> = Vec::new();

    // Add built-in node types
    all_views.extend(
        registry
            .built_in_node_types
            .values()
            .filter(|node| node.public)
            .map(|node| {
                (
                    node.category.clone(),
                    APINodeTypeView {
                        name: node.name.clone(),
                        description: node.description.clone(),
                        summary: node.summary.clone(),
                        category: node.category.clone().into(),
                        allowed_in_zone_body: allowed_in_zone_body(&node.name),
                    },
                )
            }),
    );

    // Add custom node networks (all have Custom category)
    all_views.extend(registry.node_networks.values().map(|network| {
        (
            NodeTypeCategory::Custom,
            APINodeTypeView {
                name: network.node_type.name.clone(),
                description: network.node_type.description.clone(),
                summary: network.node_type.summary.clone(),
                category: NodeTypeCategory::Custom.into(),
                allowed_in_zone_body: allowed_in_zone_body(&network.node_type.name),
            },
        )
    }));

    // Group by category
    let mut category_map: HashMap<NodeTypeCategory, Vec<APINodeTypeView>> = HashMap::new();
    for (category, view) in all_views {
        category_map.entry(category).or_default().push(view);
    }

    group_into_category_views(category_map)
}

/// One entry per custom network, carrying that network's validation errors
/// (its own and its zone bodies'), sorted by network name.
pub fn get_node_networks_with_validation(
    registry: &NodeTypeRegistry,
) -> Vec<APINetworkWithValidationErrors> {
    use crate::structure_designer::network_usages::{
        node_label, resolve_scope_labels, resolve_scope_network,
    };
    use crate::structure_designer::scoped_validation_errors::collect_scoped_validation_errors;

    let mut networks: Vec<APINetworkWithValidationErrors> = registry
        .node_networks
        .values()
        .map(|network| {
            // Collect this network's errors *and its zone bodies'* errors,
            // each tagged with the scope path of the body it lives in, so
            // the panel can jump to the offending node (not just the HOF).
            let validation_errors = collect_scoped_validation_errors(network)
                .into_iter()
                .map(|scoped| {
                    // The label and the body qualifier resolve against the
                    // body the error lives in, exactly like a Find Usages
                    // row — reusing the same helpers keeps the two strings
                    // consistent.
                    let node_label = scoped.node_id.and_then(|node_id| {
                        resolve_scope_network(network, &scoped.scope_path)
                            .and_then(|scope| scope.nodes.get(&node_id))
                            .map(node_label)
                    });
                    let body_qualifier = if scoped.scope_path.is_empty() {
                        None
                    } else {
                        let labels = resolve_scope_labels(network, &scoped.scope_path);
                        if labels.is_empty() {
                            None
                        } else {
                            Some(format!("in {} body", labels.join(" > ")))
                        }
                    };
                    APIValidationError {
                        error_text: scoped.error_text,
                        blocking: scoped.blocking,
                        // Validation entries are always fresh (validation
                        // covers the whole design on every pass), so they
                        // are never stale. Eval entries are appended by
                        // `get_node_networks_with_errors`.
                        source: APIErrorSource::Validation,
                        stale: false,
                        scope_path: scoped.scope_path,
                        node_id: scoped.node_id,
                        node_label,
                        body_qualifier,
                        // A validation entry always addresses a node of the
                        // network it is listed under, and is never derived
                        // (it has no upstream error to come from).
                        host_network: None,
                        root_cause: None,
                    }
                })
                .collect();

            APINetworkWithValidationErrors {
                name: network.node_type.name.clone(),
                validation_errors,
            }
        })
        .collect();
    networks.sort_by(|a, b| a.name.cmp(&b.name));
    networks
}

/// The unified per-network error list for the user-types panel
/// (`doc/design_error_management.md` D1): the registry's validation
/// entries plus each network's last-known evaluation errors — live for
/// the active network (its snapshot was harvested by the latest refresh),
/// dimmed (`stale == true`) for inactive ones.
///
/// Snapshot entries are re-validated against the current network here:
/// an entry whose node vanished is dropped rather than returned (a jump
/// must never target a dead node), and an entry whose node has since
/// gained a blocking validation error is deduped away (D8 — the predicate
/// state can change after harvest, e.g. a cross-network cascade stamping
/// "References invalid node network" on an instance in an inactive
/// network).
pub fn get_node_networks_with_errors(
    designer: &StructureDesigner,
) -> Vec<APINetworkWithValidationErrors> {
    use crate::structure_designer::eval_errors::has_blocking_validation_error;
    use crate::structure_designer::network_usages::{
        node_label, resolve_scope_labels, resolve_scope_network,
    };

    let mut networks = get_node_networks_with_validation(&designer.node_type_registry);
    for entry in &mut networks {
        let Some(snapshot) = designer.eval_error_snapshots.get(&entry.name) else {
            continue;
        };
        let stale = designer.active_node_network_name.as_deref() != Some(entry.name.as_str());
        for eval_error in snapshot {
            // The offending node lives either in this network or — for a
            // root cause reached across a custom-network boundary (D7) — in
            // the network the entry names.
            let host_name = eval_error.host_network.as_deref().unwrap_or(&entry.name);
            let Some(host) = designer.node_type_registry.node_networks.get(host_name) else {
                continue;
            };
            let Some(scope) = resolve_scope_network(host, &eval_error.scope_path) else {
                continue;
            };
            let Some(node) = scope.nodes.get(&eval_error.node_id) else {
                continue;
            };
            // The D8 dedupe applies to rows of *this* network: a poisoned
            // node is represented by its blocking validation row here. A
            // cross-network row is exempt — that validation row lives in the
            // other network's list, so dropping it here would leave the
            // derived entries with no collapse parent.
            if eval_error.host_network.is_none()
                && has_blocking_validation_error(scope, eval_error.node_id)
            {
                continue;
            }
            // Label + qualifier resolve exactly like a validation row's
            // (`get_node_networks_with_validation`), so the two entry
            // kinds render consistently in the picker; a cross-network row
            // additionally names its host network as provenance.
            let body_labels = if eval_error.scope_path.is_empty() {
                None
            } else {
                let labels = resolve_scope_labels(host, &eval_error.scope_path);
                (!labels.is_empty()).then(|| labels.join(" > "))
            };
            let body_qualifier = match (eval_error.host_network.as_deref(), body_labels) {
                (None, None) => None,
                (None, Some(labels)) => Some(format!("in {} body", labels)),
                (Some(host_name), None) => Some(format!("in {}", host_name)),
                (Some(host_name), Some(labels)) => {
                    Some(format!("in {} > {} body", host_name, labels))
                }
            };
            entry.validation_errors.push(APIValidationError {
                error_text: eval_error.error_text.clone(),
                // An evaluation error always means "this node's output is
                // unavailable" — Error severity (red), never advisory.
                blocking: true,
                source: APIErrorSource::Evaluation,
                stale,
                scope_path: eval_error.scope_path.clone(),
                node_id: Some(eval_error.node_id),
                node_label: Some(node_label(node)),
                body_qualifier,
                host_network: eval_error.host_network.clone(),
                root_cause: resolve_api_root_cause(designer, eval_error.root.as_ref()),
            });
        }
    }
    networks
}

/// Resolve a stored [`RootCauseRef`] into the API shape, dropping it when
/// the target no longer exists (a vanished root makes its derived entries
/// plain top-level rows rather than rows pointing at a dead node).
fn resolve_api_root_cause(
    designer: &StructureDesigner,
    root: Option<&RootCauseRef>,
) -> Option<APIErrorRootCause> {
    use crate::structure_designer::network_usages::{node_label, resolve_scope_network};

    let root = root?;
    let network = designer
        .node_type_registry
        .node_networks
        .get(&root.address.host_network)?;
    let scope = resolve_scope_network(network, &root.address.scope_path)?;
    let node = scope.nodes.get(&root.address.node_id)?;
    Some(APIErrorRootCause {
        host_network: root.address.host_network.clone(),
        scope_path: root.address.scope_path.clone(),
        node_id: root.address.node_id,
        node_label: node_label(node),
        error_text: root.error_text.clone(),
    })
}

/// "Go to root cause" for one node of the **active** network
/// (`doc/design_error_management.md` D7): the terminal of its origin-link
/// walk, or `None` when the node is itself a root cause (or carries no
/// evaluation error at all). Backs the node context-menu action; the picker
/// rows read the same address off `APIValidationError::root_cause`.
pub fn get_node_root_cause(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIErrorRootCause> {
    let network_name = designer.active_node_network_name.as_deref()?;
    let snapshot = designer.eval_error_snapshots.get(network_name)?;
    let entry = snapshot
        .iter()
        .find(|e| e.host_network.is_none() && e.node_id == node_id && e.scope_path == scope_path)?;
    resolve_api_root_cause(designer, entry.root.as_ref())
}
