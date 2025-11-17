use crate::structure_designer::nodes::relax::RelaxEvalCache;
use crate::api::api_common::with_cad_instance_or;

#[flutter_rust_bridge::frb(sync)]
pub fn get_relax_message() -> String {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        // Check if the selected node is a relax node
        if let Some(_node_id) = cad_instance.structure_designer.get_selected_node_id_with_type("relax") {
          // Try to get the evaluation cache and downcast it to RelaxEvalCache
          if let Some(eval_cache) = cad_instance.structure_designer.get_selected_node_eval_cache() {
            if let Some(relax_cache) = eval_cache.downcast_ref::<RelaxEvalCache>() {
              return relax_cache.relax_message.clone();
            }
          }
        }
        // Return empty string if no relax node is selected or no cache is available
        String::new()
      },
      String::new()
    )
  }
}