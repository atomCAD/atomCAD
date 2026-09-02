//! Shared scaffolding for the incremental-layout tests
//! (`doc/design_incremental_layout.md`).
//!
//! Kept beside [`layout_oracle`](super::layout_oracle), which holds the
//! invariants; this module holds only the boring parts — an empty network, a
//! path lookup — so no layout test has to grow its own copy.

#![allow(dead_code)]

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_data::NoData;
use atomcad_structure_designer::node_network::{Node, NodeNetwork};
use atomcad_structure_designer::node_type::{
    NodeType, NodeTypeCategory, OutputPinDefinition, no_data_loader, no_data_saver,
};

/// A bare network to run `edit_network` against.
pub fn empty_network() -> NodeNetwork {
    NodeNetwork::new(NodeType {
        name: "test".to_string(),
        description: String::new(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    })
}

/// The node at a name path, descending through zone bodies. Panics with the
/// path if any segment is missing — a layout test that mistypes a name should
/// say so, not silently assert nothing.
pub fn node_by_path<'a>(network: &'a NodeNetwork, path: &[&str]) -> &'a Node {
    let (last, prefix) = path.split_last().expect("empty node path");
    let mut scope = network;
    for name in prefix {
        let node = named(scope, name, path);
        scope = node
            .zone
            .as_deref()
            .unwrap_or_else(|| panic!("`{name}` in {path:?} owns no body"));
    }
    named(scope, last, path)
}

/// The id of the node at a name path.
pub fn id_by_path(network: &NodeNetwork, path: &[&str]) -> u64 {
    node_by_path(network, path).id
}

fn named<'a>(scope: &'a NodeNetwork, name: &str, path: &[&str]) -> &'a Node {
    scope
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named `{name}` on path {path:?}"))
}
