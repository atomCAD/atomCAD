use std::collections::HashMap;
use super::node_type::DataType;
use super::node_type::NodeType;
use super::node_type::Parameter;
use super::node_network::NodeNetwork;

pub struct NodeTypeRegistry {
  pub built_in_node_types: HashMap<String, NodeType>,
  pub node_networks: HashMap<String, NodeNetwork>,
}

impl NodeTypeRegistry {

  pub fn new() -> Self {

    let mut ret = Self {
      built_in_node_types: HashMap::new(),
      node_networks: HashMap::new(),
    };

    ret.add_node_type(NodeType {
      name: "parameter".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry, // is not used, the parameter node's output type will be determined by the node network's node type.
    });

    ret.add_node_type(NodeType {
      name: "cuboid".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry,
    });

    ret.add_node_type(NodeType {
      name: "sphere".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry,
    });

    ret.add_node_type(NodeType {
      name: "half_space".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry,
    });

    ret.add_node_type(NodeType {
      name: "union".to_string(),
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Geometry,
              multi: true,
          },
      ],
      output_type: DataType::Geometry,
    });

    ret.add_node_type(NodeType {
      name: "intersect".to_string(),
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Geometry,
              multi: true,
          },
      ],
      output_type: DataType::Geometry,
    });

    ret.add_node_type(NodeType {
      name: "negate".to_string(),
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
              multi: false,
          },
      ],
      output_type: DataType::Geometry,
    });

    ret.add_node_type(NodeType {
      name: "diff".to_string(),
      parameters: vec![
          Parameter {
              name: "base".to_string(),
              data_type: DataType::Geometry,
              multi: true, // If multiple shapes are given, they are unioned.
          },
          Parameter {
              name: "sub".to_string(),
              data_type: DataType::Geometry,
              multi: true, // A set of shapes to subtract from base
          },
      ],
      output_type: DataType::Geometry,
    });

    ret.add_node_type(NodeType {
      name: "geo_trans".to_string(),
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
              multi: false,
          },
      ],
      output_type: DataType::Geometry,
    });

    ret.add_node_type(NodeType {
      name: "geo_to_atom".to_string(),
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
              multi: false,
          },
      ],
      output_type: DataType::Atomic,
    });

    ret.add_node_type(NodeType {
      name: "edit_atom".to_string(),
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
              multi: false,
          },
      ],
      output_type: DataType::Atomic,
    });

    ret.add_node_type(NodeType {
      name: "atom_trans".to_string(),
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
              multi: false,
          },
      ],
      output_type: DataType::Atomic,
    });

    return ret;
  }

  pub fn get_node_type_names(&self) -> Vec<String> {
    let mut names: Vec<String> = self
        .built_in_node_types
        .values()
        .map(|node| node.name.clone())
        .collect();

    names.extend(
        self.node_networks
            .values()
            .map(|network| network.node_type.name.clone()),
    );

    names
}

  pub fn get_node_type(&self, node_type_name: &str) -> Option<&NodeType> {
    let node_type = self.built_in_node_types.get(node_type_name);
    if let Some(_nt) = node_type {
      return node_type;
    }
    let node_network = self.node_networks.get(node_type_name)?;
    return Some(&node_network.node_type);
  }

  pub fn add_node_network(&mut self, node_network: NodeNetwork) {
    self.node_networks.insert(node_network.node_type.name.clone(), node_network);
  }

  fn add_node_type(&mut self, node_type: NodeType) {
    self.built_in_node_types.insert(node_type.name.clone(), node_type);
  }
}
