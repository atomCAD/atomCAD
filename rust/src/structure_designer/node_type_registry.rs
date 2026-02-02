use std::collections::{HashMap, HashSet};
use super::node_type::NodeType;
use super::nodes::string::get_node_type as string_get_node_type;
use super::nodes::bool::get_node_type as bool_get_node_type;
use super::nodes::int::get_node_type as int_get_node_type;
use super::nodes::float::get_node_type as float_get_node_type;
use super::nodes::ivec2::get_node_type as ivec2_get_node_type;
use super::nodes::ivec3::get_node_type as ivec3_get_node_type;
use super::nodes::range::get_node_type as range_get_node_type;
use super::nodes::vec2::get_node_type as vec2_get_node_type;
use super::nodes::vec3::get_node_type as vec3_get_node_type;
use super::nodes::expr::get_node_type as expr_get_node_type;
use super::nodes::value::get_node_type as value_get_node_type;
use super::nodes::map::get_node_type as map_get_node_type;
use super::nodes::motif::get_node_type as motif_get_node_type;
use super::nodes::comment::get_node_type as comment_get_node_type;
use crate::structure_designer::node_network::NodeNetwork;
use crate::api::structure_designer::structure_designer_api_types::APINetworkWithValidationErrors;
use crate::api::structure_designer::structure_designer_api_types::APINodeCategoryView;
use crate::api::structure_designer::structure_designer_api_types::APINodeTypeView;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::node_network::Node;
use super::nodes::extrude::get_node_type as extrude_get_node_type;
use super::nodes::facet_shell::get_node_type as facet_shell_get_node_type;
use super::nodes::parameter::get_node_type as parameter_get_node_type;
use super::nodes::unit_cell::get_node_type as unit_cell_get_node_type;
use super::nodes::cuboid::get_node_type as cuboid_get_node_type;
use super::nodes::polygon::get_node_type as polygon_get_node_type;
use super::nodes::reg_poly::get_node_type as reg_poly_get_node_type;
use super::nodes::sphere::get_node_type as sphere_get_node_type;
use super::nodes::circle::get_node_type as circle_get_node_type;
use super::nodes::rect::get_node_type as rect_get_node_type;
use super::nodes::half_plane::get_node_type as half_plane_get_node_type;
use super::nodes::half_space::get_node_type as half_space_get_node_type;
use super::nodes::drawing_plane::get_node_type as drawing_plane_get_node_type;
use super::nodes::union::get_node_type as union_get_node_type;
use super::nodes::union_2d::get_node_type as union_2d_get_node_type;
use super::nodes::intersect::get_node_type as intersect_get_node_type;
use super::nodes::intersect_2d::get_node_type as intersect_2d_get_node_type;
use super::nodes::diff::get_node_type as diff_get_node_type;
use super::nodes::diff_2d::get_node_type as diff_2d_get_node_type;
use super::nodes::geo_trans::get_node_type as geo_trans_get_node_type;
use super::nodes::lattice_symop::get_node_type as lattice_symop_get_node_type;
use super::nodes::lattice_move::get_node_type as lattice_move_get_node_type;
use super::nodes::lattice_rot::get_node_type as lattice_rot_get_node_type;
use super::nodes::atom_cut::get_node_type as atom_cut_get_node_type;
use super::nodes::relax::get_node_type as relax_get_node_type;
use super::nodes::atom_move::get_node_type as atom_move_get_node_type;
use super::nodes::atom_trans::get_node_type as atom_trans_get_node_type;
use super::nodes::edit_atom::edit_atom::get_node_type as edit_atom_get_node_type;
use super::nodes::atom_fill::get_node_type as atom_fill_get_node_type;
use super::nodes::import_xyz::get_node_type as import_xyz_get_node_type;
use super::nodes::export_xyz::get_node_type as export_xyz_get_node_type;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_network::Argument;


pub struct NodeTypeRegistry {
  pub built_in_node_types: HashMap<String, NodeType>,
  pub node_networks: HashMap<String, NodeNetwork>,
  pub design_file_name: Option<String>,
}

impl NodeTypeRegistry {

  pub fn new() -> Self {

    let mut ret = Self {
      built_in_node_types: HashMap::new(),
      node_networks: HashMap::new(),
      design_file_name: None,
    };

    // Annotation nodes
    ret.add_node_type(comment_get_node_type());

    ret.add_node_type(parameter_get_node_type());

    ret.add_node_type(expr_get_node_type());
    ret.add_node_type(value_get_node_type());
    ret.add_node_type(map_get_node_type());
    ret.add_node_type(string_get_node_type());
    ret.add_node_type(bool_get_node_type());

    ret.add_node_type(int_get_node_type());
    ret.add_node_type(float_get_node_type());
    ret.add_node_type(ivec2_get_node_type());
    ret.add_node_type(ivec3_get_node_type());
    ret.add_node_type(vec2_get_node_type());
    ret.add_node_type(vec3_get_node_type());
    ret.add_node_type(range_get_node_type());
    ret.add_node_type(unit_cell_get_node_type());

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
    ret.add_node_type(lattice_move_get_node_type());
    ret.add_node_type(lattice_rot_get_node_type());
    ret.add_node_type(motif_get_node_type());
    ret.add_node_type(atom_fill_get_node_type());
    ret.add_node_type(edit_atom_get_node_type());
    ret.add_node_type(atom_move_get_node_type());
    ret.add_node_type(atom_trans_get_node_type());
    ret.add_node_type(import_xyz_get_node_type());
    ret.add_node_type(export_xyz_get_node_type());
    ret.add_node_type(atom_cut_get_node_type());
    ret.add_node_type(relax_get_node_type());

    return ret;
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
    // Create iterator of (node_type, category) for all public nodes
    let built_in_iter = self.built_in_node_types.values()
      .filter(|nt| nt.public)
      .map(|nt| (nt, nt.category.clone()));
    
    let custom_iter = self.node_networks.values()
      .map(|network| (&network.node_type, NodeTypeCategory::Custom));
    
    // Filter by compatibility and collect views
    let all_views: Vec<APINodeTypeView> = built_in_iter.chain(custom_iter)
      .filter(|(node_type, _)| {
        if dragging_from_output {
          node_type.parameters.iter().any(|param| {
            DataType::can_be_converted_to(source_type, &param.data_type)
          })
        } else {
          DataType::can_be_converted_to(&node_type.output_type, source_type)
        }
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
      category_map.entry(view.category.clone())
        .or_insert_with(Vec::new)
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
            })
    );
    
    // Add custom node networks (all have Custom category)
    all_views.extend(
        self.node_networks
            .values()
            .map(|network| APINodeTypeView {
              name: network.node_type.name.clone(),
              description: network.node_type.description.clone(),
              summary: network.node_type.summary.clone(),
              category: NodeTypeCategory::Custom,
            })
    );
    
    // Group by category
    let mut category_map: HashMap<NodeTypeCategory, Vec<APINodeTypeView>> = HashMap::new();
    for view in all_views {
      category_map.entry(view.category.clone())
          .or_insert_with(Vec::new)
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
    let mut names: Vec<String> = self.node_networks
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
    let mut networks: Vec<APINetworkWithValidationErrors> = self.node_networks
      .values()
      .map(|network| {
        let validation_errors = if network.validation_errors.is_empty() {
          None
        } else {
          Some(
            network.validation_errors
              .iter()
              .map(|error| error.error_text.clone())
              .collect::<Vec<String>>()
              .join("\n")
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
    let node_type = self.built_in_node_types.get(node_type_name);
    if let Some(nt) = node_type {
      return Some(nt);
    }
    let node_network = self.node_networks.get(node_type_name)?;
    return Some(&node_network.node_type);
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

  /// Initializes custom node type cache for all parameter and expr nodes in a network
  pub fn initialize_custom_node_types_for_network(&self, network: &mut NodeNetwork) {
    for node in network.nodes.values_mut() {
      self.populate_custom_node_type_cache(node, false);
    }
  }

  /// Static helper function to populate custom node type cache without borrowing conflicts
  /// Returns whether a custom node type was populated or not
  pub fn populate_custom_node_type_cache_with_types(built_in_types: &std::collections::HashMap<String, NodeType>, node: &mut Node, refresh_args: bool) -> bool {
    if let Some(base_node_type) = built_in_types.get(&node.node_type_name) {
      let custom_node_type = node.data.calculate_custom_node_type(base_node_type);
      let has_custom_node_type = custom_node_type.is_some();
      node.set_custom_node_type(custom_node_type, refresh_args);
      return has_custom_node_type;
    }
    return false;
  }

  /// Populates the custom node type cache for nodes with dynamic node types
  pub fn populate_custom_node_type_cache(&self, node: &mut Node, refresh_args: bool) -> bool {
    Self::populate_custom_node_type_cache_with_types(&self.built_in_node_types, node, refresh_args)
  }

  pub fn get_node_param_data_type(&self, node: &Node, parameter_index: usize) -> DataType {
    let node_type = self.get_node_type_for_node(node).unwrap();
    node_type.parameters[parameter_index].data_type.clone()
  }

  pub fn get_parameter_name(&self, node: &Node, parameter_index: usize) -> String {
    let node_type = self.get_node_type_for_node(node).unwrap();
    node_type.parameters[parameter_index].name.clone()
  }

  pub fn add_node_network(&mut self, node_network: NodeNetwork) {
    self.node_networks.insert(node_network.node_type.name.clone(), node_network);
  }

  fn add_node_type(&mut self, node_type: NodeType) {
    self.built_in_node_types.insert(node_type.name.clone(), node_type);
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
    
    // Search through all networks to find ones that use this network as a node
    for (parent_name, parent_network) in &self.node_networks {
      // Skip the network itself
      if parent_name == network_name {
        continue;
      }
      
      // Check if any node in the parent network uses this network as its type
      for node in parent_network.nodes.values() {
        if node.node_type_name == network_name {
          parent_networks.push(parent_name.clone());
          break; // No need to check other nodes in this network
        }
      }
    }
    
    parent_networks
  }

  /// Repairs a node network by ensuring all nodes have the correct number of arguments
  /// to match their node type parameters. Adds empty arguments if a node has fewer
  /// arguments than its node type requires.
  /// 
  /// # Parameters
  /// * `network` - A mutable reference to the node network to repair
  pub fn repair_node_network(&self, network: &mut NodeNetwork) {
    let node_ids: HashSet<u64> = network.nodes.keys().copied().collect();

    // Iterate through all nodes in the network
    for node in network.nodes.values_mut() {
      // Get the node type for this node
      if let Some(node_type) = self.get_node_type_for_node(node) {
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
      //   (currently only -1=function pin and 0=regular output pin are valid)
      for argument in node.arguments.iter_mut() {
        argument.argument_output_pins.retain(|source_node_id, output_pin_index| {
          node_ids.contains(source_node_id)
            && (*output_pin_index == -1 || *output_pin_index == 0)
        });
      }
    }
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
  fn dfs_dependencies(&self, network_name: &str, result: &mut HashSet<String>, visited: &mut HashSet<String>) {
    // Avoid infinite recursion in case of circular dependencies
    if visited.contains(network_name) {
      return;
    }
    visited.insert(network_name.to_string());
    
    // Add this network to the result
    result.insert(network_name.to_string());
    
    // Find the network in our registry
    if let Some(network) = self.node_networks.get(network_name) {
      // Examine all nodes in this network
      for node in network.nodes.values() {
        let node_type_name = &node.node_type_name;
        
        // Check if this node references another user-defined network
        // (Skip built-in node types)
        if self.node_networks.contains_key(node_type_name) {
          // Recursively find dependencies of this referenced network
          self.dfs_dependencies(node_type_name, result, visited);
        }
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
    
    // Find dependencies and visit them first
    if let Some(network) = self.node_networks.get(network_name) {
      for node in network.nodes.values() {
        let node_type_name = &node.node_type_name;
        
        // Check if this node references another user-defined network
        if self.node_networks.contains_key(node_type_name) {
          // Visit dependency first
          self.dfs_topological_sort(node_type_name, result, visited, temp_mark);
        }
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
















