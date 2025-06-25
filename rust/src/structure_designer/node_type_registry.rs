use std::collections::HashMap;
use super::node_type::DataType;
use super::node_type::NodeType;
use super::node_type::Parameter;
use super::node_network::NodeNetwork;
use super::nodes::extrude::ExtrudeData;
use super::nodes::parameter::ParameterData;
use super::nodes::cuboid::CuboidData;
use super::nodes::polygon::PolygonData;
use super::nodes::sphere::SphereData;
use super::nodes::circle::CircleData;
use super::nodes::rect::RectData;
use super::nodes::half_plane::HalfPlaneData;
use super::nodes::half_space::HalfSpaceData;
use super::nodes::geo_trans::GeoTransData;
use super::nodes::atom_trans::AtomTransData;
use super::nodes::edit_atom::edit_atom::EditAtomData;
use super::nodes::geo_to_atom::GeoToAtomData;
use super::nodes::anchor::AnchorData;
use super::nodes::stamp::StampData;
use super::node_data::NoData;
use glam::{IVec3, DVec3, IVec2};

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
      node_data_creator: || Box::new(ParameterData {
        param_index: 0,
      }),
    });

    ret.add_node_type(NodeType {
      name: "rect".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry2D,
      node_data_creator: || Box::new(RectData {
        min_corner: IVec2::new(-1, -1),
        extent: IVec2::new(2, 2),
      }),
    });

    ret.add_node_type(NodeType {
      name: "circle".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry2D,
      node_data_creator: || Box::new(CircleData {
        center: IVec2::new(0, 0),
        radius: 1,
      }),
    });

    ret.add_node_type(NodeType {
      name: "polygon".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry2D,
      node_data_creator: || Box::new(PolygonData {
        num_sides: 3,
        radius: 3,
      }),
    });

    ret.add_node_type(NodeType {
      name: "union_2d".to_string(),
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Geometry2D,
              multi: true,
          },
      ],
      output_type: DataType::Geometry2D,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "intersect_2d".to_string(),
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Geometry2D,
              multi: true,
          },
      ],
      output_type: DataType::Geometry2D,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "diff_2d".to_string(),
      parameters: vec![
          Parameter {
              name: "base".to_string(),
              data_type: DataType::Geometry2D,
              multi: true, // If multiple shapes are given, they are unioned.
          },
          Parameter {
              name: "sub".to_string(),
              data_type: DataType::Geometry2D,
              multi: true, // A set of shapes to subtract from base
          },
      ],
      output_type: DataType::Geometry2D,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "half_plane".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry2D,
      node_data_creator: || Box::new(HalfPlaneData {
        point1: IVec2::new(0, 0),
        point2: IVec2::new(1, 0),
      }),
    });

    ret.add_node_type(NodeType {
      name: "extrude".to_string(),
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry2D,
              multi: false,
          },
      ],
      output_type: DataType::Geometry,
      node_data_creator: || Box::new(ExtrudeData {
        height: 1,
      }),
    });

    ret.add_node_type(NodeType {
      name: "cuboid".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry,
      node_data_creator: || Box::new(CuboidData {
        min_corner: IVec3::new(-1, -1, -1),
        extent: IVec3::new(2, 2, 2),
      }),
    });

    ret.add_node_type(NodeType {
      name: "sphere".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry,
      node_data_creator: || Box::new(SphereData {
        center: IVec3::new(0, 0, 0),
        radius: 1,
      }),
    });

    ret.add_node_type(NodeType {
      name: "half_space".to_string(),
      parameters: Vec::new(),
      output_type: DataType::Geometry,
      node_data_creator: || Box::new(HalfSpaceData {
        miller_index: IVec3::new(1, 0, 0), // Default normal along x-axis
        center: IVec3::new(0, 0, 0),
      }),
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
      node_data_creator: || Box::new(NoData {}),
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
      node_data_creator: || Box::new(NoData {}),
    });

    /*
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
      node_data_creator: || Box::new(NoData {}),
    });
    */

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
      node_data_creator: || Box::new(NoData {}),
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
      node_data_creator: || Box::new(GeoTransData {
        translation: IVec3::new(0, 0, 0),
        rotation: IVec3::new(0, 0, 0),
        transform_only_frame: false,
      }),
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
      node_data_creator: || Box::new(GeoToAtomData {
        primary_atomic_number: 6,
        secondary_atomic_number: 6,
      }),
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
      node_data_creator: || Box::new(EditAtomData::new()),
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
      node_data_creator: || Box::new(AtomTransData {
        translation: DVec3::new(0.0, 0.0, 0.0),
        rotation: DVec3::new(0.0, 0.0, 0.0),
      }),
    });

    ret.add_node_type(NodeType {
      name: "anchor".to_string(),
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
              multi: false,
          },
      ],
      output_type: DataType::Atomic,
      node_data_creator: || Box::new(AnchorData::new()),
    });

    ret.add_node_type(NodeType {
      name: "stamp".to_string(),
      parameters: vec![
        Parameter {
          name: "crystal".to_string(),
          data_type: DataType::Atomic,
          multi: false,
        },
        Parameter {
          name: "stamp".to_string(),
          data_type: DataType::Atomic,
          multi: false,
        },
      ],
      output_type: DataType::Atomic,
      node_data_creator: || Box::new(StampData::new()),
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

  pub fn get_node_network_names(&self) -> Vec<String> {
    self.node_networks
            .values()
            .map(|network| network.node_type.name.clone())
            .collect()
  }

  pub fn get_node_type(&self, node_type_name: &str) -> Option<&NodeType> {
    let node_type = self.built_in_node_types.get(node_type_name);
    if let Some(_nt) = node_type {
      return node_type;
    }
    let node_network = self.node_networks.get(node_type_name)?;
    return Some(&node_network.node_type);
  }

  pub fn get_parameter_name(&self, node_type_name: &str, parameter_index: usize) -> String {
    let node_type = self.get_node_type(node_type_name).unwrap();
    node_type.parameters[parameter_index].name.clone()
  }

  pub fn add_node_network(&mut self, node_network: NodeNetwork) {
    self.node_networks.insert(node_network.node_type.name.clone(), node_network);
  }

  fn add_node_type(&mut self, node_type: NodeType) {
    self.built_in_node_types.insert(node_type.name.clone(), node_type);
  }
}
