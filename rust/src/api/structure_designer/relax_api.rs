use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
};
use crate::api::structure_designer::structure_designer_api_types::APIRelaxData;
use crate::structure_designer::nodes::relax::{RelaxData, RelaxEvalCache};

#[flutter_rust_bridge::frb(sync)]
pub fn get_relax_message() -> String {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                // Check if the selected node is a relax node
                if let Some(_node_id) = cad_instance
                    .structure_designer
                    .get_selected_node_id_with_type("relax")
                {
                    // Try to get the evaluation cache and downcast it to RelaxEvalCache
                    if let Some(eval_cache) = cad_instance
                        .structure_designer
                        .get_selected_node_eval_cache()
                    {
                        if let Some(relax_cache) = eval_cache.downcast_ref::<RelaxEvalCache>() {
                            return relax_cache.relax_message.clone();
                        }
                    }
                }
                // Return empty string if no relax node is selected or no cache is available
                String::new()
            },
            String::new(),
        )
    }
}

/// Reads the stored data of a `relax` node (currently just `diff_min_move`).
/// Takes a `scope_path` like every sibling node-data accessor.
#[flutter_rust_bridge::frb(sync)]
pub fn get_relax_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIRelaxData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let relax_data = node_data.as_any_ref().downcast_ref::<RelaxData>()?;
                Some(APIRelaxData {
                    diff_min_move: relax_data.diff_min_move,
                })
            },
            None,
        )
    }
}

/// Writes the stored data of a `relax` node. Undoable via the shared
/// `SetNodeDataCommand` pushed by `set_node_network_data_scoped`.
#[flutter_rust_bridge::frb(sync)]
pub fn set_relax_data(scope_path: Vec<u64>, node_id: u64, data: APIRelaxData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let relax_data = Box::new(RelaxData {
                diff_min_move: data.diff_min_move,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, relax_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}
