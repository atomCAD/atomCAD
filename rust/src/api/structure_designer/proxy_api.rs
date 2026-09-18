//! Kernel seam for the `proxy` node's property panel.
//!
//! Phase 4 of `doc/design_proxy_node.md`. Three entry points: the stored
//! properties in and out (§3.3's eight fields plus the `available_tags`
//! snapshot the suggestion chips read), and the read-only report the panel
//! renders — `ProxyStats`, which lives in the **selected node's eval cache**
//! and never on the node data, because subnetwork node state is shared across
//! call sites (§7.6).
//!
//! Each entry point is a thin FRB wrapper over an `#[frb(ignore)]` function
//! taking an explicit `&StructureDesigner`, so the logic is testable without
//! the global `CAD_INSTANCE` (the `mechanosynth_api` / `field_distribution_api`
//! pattern — a `CADInstance` owns a `Renderer`, so no test can build one).

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
};
use crate::api::structure_designer::structure_designer_api_types::{APIProxyData, APIProxyStats};
use atomcad_structure_designer::nodes::proxy::{ProxyData, ProxyEvalCache};
use atomcad_structure_designer::structure_designer::StructureDesigner;

/// The stored data of a `proxy` node, against an explicit designer.
///
/// `None` when `node_id` names no node in `scope_path` or names a node of
/// another type.
#[flutter_rust_bridge::frb(ignore)]
pub fn proxy_node_data(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIProxyData> {
    let data = designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<ProxyData>()?;
    Some(APIProxyData::from(data))
}

/// Writes the stored data of a `proxy` node, against an explicit designer.
///
/// `available_tags` on the incoming twin is ignored: it is an eval-time
/// snapshot the node rewrites on every evaluation, not a property the panel
/// owns. Undo comes from the shared `SetNodeDataCommand` that
/// `set_node_network_data_scoped` pushes.
///
/// A write to a node of another type is a no-op rather than a replacement —
/// the panel can only ever address the node it is rendering, and silently
/// swapping a node's type from here would be unrecoverable.
#[flutter_rust_bridge::frb(ignore)]
pub fn set_proxy_node_data(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    data: &APIProxyData,
) {
    let is_proxy = designer
        .get_node_network_data_scoped(scope_path, node_id)
        .map(|node_data| node_data.as_any_ref().is::<ProxyData>())
        .unwrap_or(false);
    if !is_proxy {
        return;
    }
    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(ProxyData::from(data)));
}

/// The report of the last root evaluation of the **selected** `proxy` node.
///
/// `None` when the selected node is not a `proxy`, or when it is one that has
/// not been evaluated as a root node since it was selected (a node that is not
/// displayed never becomes one). Scope-free by construction: the eval cache is
/// keyed on the active network's active node, which is what `relax`'s message
/// getter reads too.
#[flutter_rust_bridge::frb(ignore)]
pub fn proxy_node_stats(designer: &StructureDesigner) -> Option<APIProxyStats> {
    designer.get_selected_node_id_with_type("proxy")?;
    let cache = designer.get_selected_node_eval_cache()?;
    let proxy_cache = cache.downcast_ref::<ProxyEvalCache>()?;
    Some(APIProxyStats::from(&proxy_cache.stats))
}

// ============================================================================
// FRB wrappers
// ============================================================================

#[flutter_rust_bridge::frb(sync)]
pub fn get_proxy_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIProxyData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| proxy_node_data(&cad_instance.structure_designer, &scope_path, node_id),
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_proxy_data(scope_path: Vec<u64>, node_id: u64, data: APIProxyData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            set_proxy_node_data(
                &mut cad_instance.structure_designer,
                &scope_path,
                node_id,
                &data,
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_proxy_stats() -> Option<APIProxyStats> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| proxy_node_stats(&cad_instance.structure_designer),
            None,
        )
    }
}
