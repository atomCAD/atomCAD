use std::collections::HashMap;
use glam::DVec2;
use super::node_type::NodeType;
use super::node_type::Parameter;
use super::nodes::int::IntData;
use super::nodes::float::FloatData;
use super::nodes::ivec2::IVec2Data;
use super::nodes::ivec3::IVec3Data;
use super::nodes::vec2::Vec2Data;
use super::nodes::vec3::Vec3Data;
use super::nodes::expr::ExprData;
use super::nodes::expr::ExprParameter;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;
use crate::structure_designer::node_network::NodeNetwork;
use crate::api::structure_designer::structure_designer_api_types::APINetworkWithValidationErrors;
use crate::structure_designer::node_network::Node;
use super::nodes::extrude::ExtrudeData;
use super::nodes::facet_shell::FacetShellData;
use super::nodes::parameter::ParameterData;
use super::nodes::cuboid::CuboidData;
use super::nodes::polygon::PolygonData;
use super::nodes::reg_poly::RegPolyData;
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
use super::nodes::import_xyz::ImportXYZData;
use super::nodes::stamp::StampData;
use super::node_data::NoData;
use glam::{IVec3, DVec3, IVec2};

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

    ret.add_node_type(NodeType {
      name: "parameter".to_string(),
      parameters: vec![
          Parameter {
              name: "default".to_string(),
              data_type: APIDataType::Geometry, // will change based on  ParameterData::data_type.
              multi: false,
          },
      ],
      output_type: APIDataType::Geometry, // will change based on ParameterData::data_type.
      node_data_creator: || Box::new(ParameterData {
        param_index: 0,
        param_name: "param".to_string(),
        data_type: APIDataType::Geometry,
        multi: false,
        sort_order: 0,
      }),
    });

    ret.add_node_type(NodeType {
      name: "expr".to_string(),
      parameters: vec![],
      output_type: APIDataType::None, // will change based on the expression
      node_data_creator: || Box::new(ExprData {
        parameters: vec![
          ExprParameter {
            name: "x".to_string(),
            data_type: APIDataType::Float,
          },
        ],
        expression: "x".to_string(),
        expr: None,
        error: None,
        output_type: Some(APIDataType::Float),
      }),
    });

    ret.add_node_type(NodeType {
      name: "int".to_string(),
      parameters: vec![],
      output_type: APIDataType::Int,
      node_data_creator: || Box::new(IntData {
        value: 0
      }),
    });

    ret.add_node_type(NodeType {
      name: "float".to_string(),
      parameters: vec![],
      output_type: APIDataType::Float,
      node_data_creator: || Box::new(FloatData {
        value: 0.0
      }),
    });

    ret.add_node_type(NodeType {
      name: "ivec2".to_string(),
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: APIDataType::Int,
            multi: false,
        },
        Parameter {
            name: "y".to_string(),
            data_type: APIDataType::Int,
            multi: false,
        },        
      ],
      output_type: APIDataType::IVec2,
      node_data_creator: || Box::new(IVec2Data {
        value: IVec2::new(0, 0)
      }),
    });

    ret.add_node_type(NodeType {
      name: "ivec3".to_string(),
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: APIDataType::Int,
            multi: false,
        },
        Parameter {
            name: "y".to_string(),
            data_type: APIDataType::Int,
            multi: false,
        },
        Parameter {
            name: "z".to_string(),
            data_type: APIDataType::Int,
            multi: false,
        },        
      ],
      output_type: APIDataType::IVec3,
      node_data_creator: || Box::new(IVec3Data {
        value: IVec3::new(0, 0, 0)
      }),
    });

    ret.add_node_type(NodeType {
      name: "vec2".to_string(),
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: APIDataType::Float,
            multi: false,
        },
        Parameter {
            name: "y".to_string(),
            data_type: APIDataType::Float,
            multi: false,
        },        
      ],
      output_type: APIDataType::Vec2,
      node_data_creator: || Box::new(Vec2Data {
        value: DVec2::new(0.0, 0.0)
      }),
    });

    ret.add_node_type(NodeType {
      name: "vec3".to_string(),
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: APIDataType::Float,
            multi: false,
        },
        Parameter {
            name: "y".to_string(),
            data_type: APIDataType::Float,
            multi: false,
        },
        Parameter {
            name: "z".to_string(),
            data_type: APIDataType::Float,
            multi: false,
        },        
      ],
      output_type: APIDataType::Vec3,
      node_data_creator: || Box::new(Vec3Data {
        value: DVec3::new(0.0, 0.0, 0.0)
      }),
    });

    ret.add_node_type(NodeType {
      name: "rect".to_string(),
      parameters: vec![
        Parameter {
            name: "min_corner".to_string(),
            data_type: APIDataType::IVec2,
            multi: false,
        },
        Parameter {
          name: "extent".to_string(),
          data_type: APIDataType::IVec2,
          multi: false,
        },
      ],
      output_type: APIDataType::Geometry2D,
      node_data_creator: || Box::new(RectData {
        min_corner: IVec2::new(-1, -1),
        extent: IVec2::new(2, 2),
      }),
    });

    ret.add_node_type(NodeType {
      name: "circle".to_string(),
      parameters: vec![
        Parameter {
            name: "center".to_string(),
            data_type: APIDataType::IVec2,
            multi: false,
        },
        Parameter {
          name: "radius".to_string(),
          data_type: APIDataType::Int,
          multi: false,
        },
      ],
      output_type: APIDataType::Geometry2D,
      node_data_creator: || Box::new(CircleData {
        center: IVec2::new(0, 0),
        radius: 1,
      }),
    });

    ret.add_node_type(NodeType {
      name: "reg_poly".to_string(),
      parameters: Vec::new(),
      output_type: APIDataType::Geometry2D,
      node_data_creator: || Box::new(RegPolyData {
        num_sides: 3,
        radius: 3,
      }),
    });

    ret.add_node_type(NodeType {
      name: "polygon".to_string(),
      parameters: Vec::new(),
      output_type: APIDataType::Geometry2D,
      node_data_creator: || Box::new(PolygonData {
        vertices: vec![
          IVec2::new(-1, -1),
          IVec2::new(1, -1),
          IVec2::new(0, 1),
        ],
      }),
    });

    ret.add_node_type(NodeType {
      name: "union_2d".to_string(),
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: APIDataType::Geometry2D,
              multi: true,
          },
      ],
      output_type: APIDataType::Geometry2D,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "intersect_2d".to_string(),
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: APIDataType::Geometry2D,
              multi: true,
          },
      ],
      output_type: APIDataType::Geometry2D,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "diff_2d".to_string(),
      parameters: vec![
          Parameter {
              name: "base".to_string(),
              data_type: APIDataType::Geometry2D,
              multi: true, // If multiple shapes are given, they are unioned.
          },
          Parameter {
              name: "sub".to_string(),
              data_type: APIDataType::Geometry2D,
              multi: true, // A set of shapes to subtract from base
          },
      ],
      output_type: APIDataType::Geometry2D,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "half_plane".to_string(),
      parameters: Vec::new(),
      output_type: APIDataType::Geometry2D,
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
              data_type: APIDataType::Geometry2D,
              multi: false,
          },
      ],
      output_type: APIDataType::Geometry,
      node_data_creator: || Box::new(ExtrudeData {
        height: 1,
      }),
    });

    ret.add_node_type(NodeType {
      name: "cuboid".to_string(),
      parameters: vec![
        Parameter {
            name: "min_corner".to_string(),
            data_type: APIDataType::IVec3,
            multi: false,
        },
        Parameter {
          name: "extent".to_string(),
          data_type: APIDataType::IVec3,
          multi: false,
        },
      ],
      output_type: APIDataType::Geometry,
      node_data_creator: || Box::new(CuboidData {
        min_corner: IVec3::new(-1, -1, -1),
        extent: IVec3::new(2, 2, 2),
      }),
    });

    ret.add_node_type(NodeType {
      name: "sphere".to_string(),
      parameters: vec![
        Parameter {
            name: "center".to_string(),
            data_type: APIDataType::IVec3,
            multi: false,
        },
        Parameter {
          name: "radius".to_string(),
          data_type: APIDataType::Int,
          multi: false,
        },
      ],
      output_type: APIDataType::Geometry,
      node_data_creator: || Box::new(SphereData {
        center: IVec3::new(0, 0, 0),
        radius: 1,
      }),
    });

    ret.add_node_type(NodeType {
      name: "half_space".to_string(),
      parameters: Vec::new(),
      output_type: APIDataType::Geometry,
      node_data_creator: || Box::new(HalfSpaceData {
        max_miller_index: 2,
        miller_index: IVec3::new(0, 1, 0), // Default normal along y-axis
        center: IVec3::new(0, 0, 0),
        shift: 0,
      }),
    });

    ret.add_node_type(NodeType {
      name: "facet_shell".to_string(),
      parameters: Vec::new(),
      output_type: APIDataType::Geometry,
      node_data_creator: || Box::new(FacetShellData::default()),
    });

    ret.add_node_type(NodeType {
      name: "union".to_string(),
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: APIDataType::Geometry,
              multi: true,
          },
      ],
      output_type: APIDataType::Geometry,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "intersect".to_string(),
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: APIDataType::Geometry,
              multi: true,
          },
      ],
      output_type: APIDataType::Geometry,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "diff".to_string(),
      parameters: vec![
          Parameter {
              name: "base".to_string(),
              data_type: APIDataType::Geometry,
              multi: true, // If multiple shapes are given, they are unioned.
          },
          Parameter {
              name: "sub".to_string(),
              data_type: APIDataType::Geometry,
              multi: true, // A set of shapes to subtract from base
          },
      ],
      output_type: APIDataType::Geometry,
      node_data_creator: || Box::new(NoData {}),
    });

    ret.add_node_type(NodeType {
      name: "geo_trans".to_string(),
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: APIDataType::Geometry,
              multi: false,
          },
          Parameter {
            name: "translation".to_string(),
            data_type: APIDataType::IVec3,
            multi: false,
          },
          Parameter {
            name: "rotation".to_string(),
            data_type: APIDataType::IVec3,
            multi: false,
          },
      ],
      output_type: APIDataType::Geometry,
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
              data_type: APIDataType::Geometry,
              multi: false,
          },
      ],
      output_type: APIDataType::Atomic,
      node_data_creator: || Box::new(GeoToAtomData {
        primary_atomic_number: 6,
        secondary_atomic_number: 6,
        hydrogen_passivation: true,
      }),
    });

    ret.add_node_type(NodeType {
      name: "edit_atom".to_string(),
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: APIDataType::Atomic,
              multi: false,
          },
      ],
      output_type: APIDataType::Atomic,
      node_data_creator: || Box::new(EditAtomData::new()),
    });

    ret.add_node_type(NodeType {
      name: "atom_trans".to_string(),
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: APIDataType::Atomic,
              multi: false,
          },
          Parameter {
            name: "translation".to_string(),
            data_type: APIDataType::Vec3,
            multi: false,
          },
          Parameter {
            name: "rotation".to_string(),
            data_type: APIDataType::Vec3,
            multi: false,
          },
      ],
      output_type: APIDataType::Atomic,
      node_data_creator: || Box::new(AtomTransData {
        translation: DVec3::new(0.0, 0.0, 0.0),
        rotation: DVec3::new(0.0, 0.0, 0.0),
      }),
    });

    ret.add_node_type(NodeType {
      name: "import_xyz".to_string(),
      parameters: vec![],
      output_type: APIDataType::Atomic,
      node_data_creator: || Box::new(ImportXYZData::new()),
    });

    ret.add_node_type(NodeType {
      name: "anchor".to_string(),
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: APIDataType::Atomic,
              multi: false,
          },
      ],
      output_type: APIDataType::Atomic,
      node_data_creator: || Box::new(AnchorData::new()),
    });

    ret.add_node_type(NodeType {
      name: "stamp".to_string(),
      parameters: vec![
        Parameter {
          name: "crystal".to_string(),
          data_type: APIDataType::Atomic,
          multi: false,
        },
        Parameter {
          name: "stamp".to_string(),
          data_type: APIDataType::Atomic,
          multi: false,
        },
      ],
      output_type: APIDataType::Atomic,
      node_data_creator: || Box::new(StampData::new()),
    });

    ret.add_node_type(NodeType {
      name: "relax".to_string(),
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: APIDataType::Atomic,
              multi: false,
          },
      ],
      output_type: APIDataType::Atomic,
      node_data_creator: || Box::new(NoData {}),
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

  pub fn get_node_networks_with_validation(&self) -> Vec<APINetworkWithValidationErrors> {
    self.node_networks
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
      .collect()
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
      self.populate_custom_node_type_cache(node);
    }
  }

  /// Static helper function to populate custom node type cache without borrowing conflicts
  pub fn populate_custom_node_type_cache_with_types(built_in_types: &std::collections::HashMap<String, NodeType>, node: &mut Node) {
    match node.node_type_name.as_str() {
      "parameter" => {
        if let Some(param_data) = (*node.data).as_any_ref().downcast_ref::<crate::structure_designer::nodes::parameter::ParameterData>() {
          if let Some(base_node_type) = built_in_types.get("parameter") {
            let mut custom_node_type = base_node_type.clone();

            custom_node_type.parameters[0].data_type = param_data.data_type;
            custom_node_type.parameters[0].multi = param_data.multi;

            custom_node_type.output_type = param_data.data_type;
            
            node.set_custom_node_type(Some(custom_node_type));
          }
        }
      },
      "expr" => {
        if let Some(expr_data) = (*node.data).as_any_ref().downcast_ref::<crate::structure_designer::nodes::expr::ExprData>() {
          if let Some(base_node_type) = built_in_types.get("expr") {
            let mut custom_node_type = base_node_type.clone();
            
            // Update the output type - use APIDataType::None if expr_data.output_type is None
            custom_node_type.output_type = expr_data.output_type.unwrap_or(APIDataType::None);
            
            // Convert ExprParameter to Parameter
            custom_node_type.parameters = expr_data.parameters.iter()
              .map(|expr_param| Parameter {
                name: expr_param.name.clone(),
                data_type: expr_param.data_type,
                multi: false, // Expression parameters are single-valued
              })
              .collect();

            node.set_custom_node_type(Some(custom_node_type));
          }
        }
      },
      _ => {}
    }
  }

  /// Populates the custom node type cache for parameter and expr nodes
  pub fn populate_custom_node_type_cache(&self, node: &mut Node) {
    Self::populate_custom_node_type_cache_with_types(&self.built_in_node_types, node);
  }

  pub fn get_node_param_data_type(&self, node: &Node, parameter_index: usize) -> APIDataType {
    let node_type = self.get_node_type_for_node(node).unwrap();
    node_type.parameters[parameter_index].data_type
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
  
  /// Checks if a source data type can be converted to a destination data type
  /// 
  /// # Parameters
  /// * `source_type` - The source data type
  /// * `dest_type` - The destination data type
  /// 
  /// # Returns
  /// True if the source type can be converted to the destination type
  pub fn can_be_converted_to(&self, source_type: APIDataType, dest_type: APIDataType) -> bool {
    // Same types are always compatible
    if source_type == dest_type {
      return true;
    }
    
    // Define conversion rules
    match (source_type, dest_type) {
      // Int <-> Float conversions
      (APIDataType::Int, APIDataType::Float) => true,
      (APIDataType::Float, APIDataType::Int) => true,
      
      // IVec2 <-> Vec2 conversions
      (APIDataType::IVec2, APIDataType::Vec2) => true,
      (APIDataType::Vec2, APIDataType::IVec2) => true,
      
      // IVec3 <-> Vec3 conversions
      (APIDataType::IVec3, APIDataType::Vec3) => true,
      (APIDataType::Vec3, APIDataType::IVec3) => true,
      
      // All other combinations are not compatible
      _ => false,
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure_designer::node_network::Node;
    use crate::structure_designer::nodes::parameter::ParameterData;
    use crate::structure_designer::nodes::expr::{ExprData, ExprParameter};
    use crate::api::structure_designer::structure_designer_api_types::APIDataType;



    #[test]
    fn test_regular_node_type() {
        let registry = NodeTypeRegistry::new();
        
        // Test that regular nodes still work correctly
        let node_type = registry.get_node_type("int").unwrap();
        assert_eq!(node_type.output_type, APIDataType::Int);
        assert_eq!(node_type.parameters.len(), 0);
    }
}
