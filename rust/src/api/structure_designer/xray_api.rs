use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
};
use crate::api::structure_designer::structure_designer_api_types::APIXrayData;
use crate::structure_designer::nodes::xray::XrayData;

/// Reads the stored data of an `xray` node (`alpha` + `opaque_depth`).
/// Takes a `scope_path` like every sibling node-data accessor.
#[flutter_rust_bridge::frb(sync)]
pub fn get_xray_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIXrayData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let xray_data = node_data.as_any_ref().downcast_ref::<XrayData>()?;
                Some(APIXrayData {
                    alpha: xray_data.alpha,
                    opaque_depth: xray_data.opaque_depth,
                })
            },
            None,
        )
    }
}

/// Writes the stored data of an `xray` node. Undoable via the shared
/// `SetNodeDataCommand` pushed by `set_node_network_data_scoped`.
#[flutter_rust_bridge::frb(sync)]
pub fn set_xray_data(scope_path: Vec<u64>, node_id: u64, data: APIXrayData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let xray_data = Box::new(XrayData {
                alpha: data.alpha,
                opaque_depth: data.opaque_depth,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, xray_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}
