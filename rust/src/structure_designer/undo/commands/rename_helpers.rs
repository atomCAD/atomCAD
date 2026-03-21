use crate::structure_designer::node_type_registry::NodeTypeRegistry;

/// Core rename logic shared between single rename and namespace rename,
/// in both main-method and undo-command contexts.
///
/// Handles: registry move, active name update, node type reference cascade,
/// backtick reference cascade.
///
/// Does NOT handle: validation, navigation history, clipboard, dirty/refresh, undo push.
pub fn apply_rename_core(
    registry: &mut NodeTypeRegistry,
    active_name: &mut Option<String>,
    old_name: &str,
    new_name: &str,
) {
    // Take the network out and re-insert with new name
    let mut network = match registry.node_networks.remove(old_name) {
        Some(n) => n,
        None => return,
    };
    network.node_type.name = new_name.to_string();
    registry.node_networks.insert(new_name.to_string(), network);

    // Update active network name if it was the renamed network
    if active_name.as_deref() == Some(old_name) {
        *active_name = Some(new_name.to_string());
    }

    // Update all nodes in all networks that reference the old type name
    for network in registry.node_networks.values_mut() {
        for node in network.nodes.values_mut() {
            if node.node_type_name == old_name {
                node.node_type_name = new_name.to_string();
            }
        }
    }

    // Update backtick references in comment nodes and network metadata
    let old_pattern = format!("`{}`", old_name);
    let new_pattern = format!("`{}`", new_name);
    for network in registry.node_networks.values_mut() {
        if network.node_type.description.contains(&old_pattern) {
            network.node_type.description = network
                .node_type
                .description
                .replace(&old_pattern, &new_pattern);
        }
        if let Some(ref mut summary) = network.node_type.summary {
            if summary.contains(&old_pattern) {
                *summary = summary.replace(&old_pattern, &new_pattern);
            }
        }

        for node in network.nodes.values_mut() {
            if node.node_type_name == "Comment" {
                if let Some(comment_data) =
                    node.data
                        .as_any_mut()
                        .downcast_mut::<crate::structure_designer::nodes::comment::CommentData>()
                {
                    if comment_data.label.contains(&old_pattern) {
                        comment_data.label = comment_data.label.replace(&old_pattern, &new_pattern);
                    }
                    if comment_data.text.contains(&old_pattern) {
                        comment_data.text = comment_data.text.replace(&old_pattern, &new_pattern);
                    }
                }
            }
        }
    }
}
