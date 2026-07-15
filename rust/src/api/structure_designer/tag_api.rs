use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
};
use crate::api::structure_designer::structure_designer_api_types::{APITagData, APIUntagData};
use crate::structure_designer::nodes::tag::{TagData, UntagData};
use std::cell::RefCell;

/// Reads the stored data of a `tag` node: the stored `name` plus the input
/// structure's tag names snapshotted at the last eval (offered as editor
/// suggestions, §Existing-names suggestions). Takes a `scope_path` like every
/// sibling node-data accessor.
#[flutter_rust_bridge::frb(sync)]
pub fn get_tag_data(scope_path: Vec<u64>, node_id: u64) -> Option<APITagData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let tag_data = node_data.as_any_ref().downcast_ref::<TagData>()?;
                Some(APITagData {
                    name: tag_data.name.clone(),
                    available_tags: tag_data.available_tags.borrow().clone(),
                })
            },
            None,
        )
    }
}

/// Writes the stored `name` of a `tag` node. Undoable via the shared
/// `SetNodeDataCommand` pushed by `set_node_network_data_scoped`.
#[flutter_rust_bridge::frb(sync)]
pub fn set_tag_data(scope_path: Vec<u64>, node_id: u64, data: APITagData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let tag_data = Box::new(TagData {
                name: data.name,
                available_tags: RefCell::new(Vec::new()),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, tag_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Reads the stored data of an `untag` node (see `get_tag_data`).
#[flutter_rust_bridge::frb(sync)]
pub fn get_untag_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIUntagData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let untag_data = node_data.as_any_ref().downcast_ref::<UntagData>()?;
                Some(APIUntagData {
                    name: untag_data.name.clone(),
                    available_tags: untag_data.available_tags.borrow().clone(),
                })
            },
            None,
        )
    }
}

/// Writes the stored `name` of an `untag` node (see `set_tag_data`).
#[flutter_rust_bridge::frb(sync)]
pub fn set_untag_data(scope_path: Vec<u64>, node_id: u64, data: APIUntagData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let untag_data = Box::new(UntagData {
                name: data.name,
                available_tags: RefCell::new(Vec::new()),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, untag_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}
