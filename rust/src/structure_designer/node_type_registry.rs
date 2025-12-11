use std::collections::{HashMap, HashSet};
use std::io;
use glam::DVec2;
use super::node_type::NodeType;
use super::node_type::Parameter;
use super::nodes::string::StringData;
use super::nodes::bool::BoolData;
use super::nodes::int::IntData;
use super::nodes::float::FloatData;
use super::nodes::ivec2::IVec2Data;
use super::nodes::ivec3::IVec3Data;
use super::nodes::range::RangeData;
use super::nodes::vec2::Vec2Data;
use super::nodes::vec3::Vec3Data;
use super::nodes::expr::{ExprData, expr_data_loader};
use super::nodes::expr::ExprParameter;
use super::nodes::value::ValueData;
use super::nodes::map::MapData;
use super::nodes::motif::{MotifData, motif_data_loader};
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::api::structure_designer::structure_designer_api_types::APINetworkWithValidationErrors;
use crate::api::structure_designer::structure_designer_api_types::APINodeTypeView;
use crate::api::structure_designer::structure_designer_api_types::APINodeCategoryView;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::node_network::Node;
use super::nodes::extrude::ExtrudeData;
use super::nodes::facet_shell::FacetShellData;
use super::nodes::parameter::ParameterData;
use super::nodes::unit_cell::UnitCellData;
use super::nodes::cuboid::CuboidData;
use super::nodes::polygon::PolygonData;
use super::nodes::reg_poly::RegPolyData;
use super::nodes::sphere::SphereData;
use super::nodes::circle::CircleData;
use super::nodes::rect::RectData;
use super::nodes::half_plane::HalfPlaneData;
use super::nodes::half_space::HalfSpaceData;
use super::nodes::drawing_plane::DrawingPlaneData;
use super::nodes::union::UnionData;
use super::nodes::union_2d::Union2DData;
use super::nodes::intersect::IntersectData;
use super::nodes::intersect_2d::Intersect2DData;
use super::nodes::diff::DiffData;
use super::nodes::diff_2d::Diff2DData;
use super::nodes::geo_trans::GeoTransData;
use super::nodes::lattice_symop::LatticeSymopData;
use super::nodes::lattice_move::LatticeMoveData;
use super::nodes::lattice_rot::LatticeRotData;
use super::nodes::atom_cut::AtomCutData;
use super::nodes::relax::RelaxData;
use super::nodes::atom_trans::AtomTransData;
use super::nodes::edit_atom::edit_atom::EditAtomData;
use super::nodes::atom_fill::AtomFillData;
use super::nodes::import_xyz::{ImportXYZData, import_xyz_data_loader, import_xyz_data_saver};
use super::nodes::export_xyz::{ExportXYZData, export_xyz_data_loader, export_xyz_data_saver};
use super::node_type::{generic_node_data_saver, generic_node_data_loader};
use crate::structure_designer::serialization::edit_atom_data_serialization::{edit_atom_data_to_serializable, serializable_to_edit_atom_data, SerializableEditAtomData};
use glam::{IVec3, DVec3, IVec2};
use crate::structure_designer::data_type::{DataType, FunctionType};
use crate::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
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

    ret.add_node_type(NodeType {
      name: "parameter".to_string(),
      description: "To set up an input pin (parameter) of your custom node you need to use a parameter node in your subnetwork.
The sort order property of a parameter determines the order of the parameters in the resulting custom node.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
          Parameter {
              name: "default".to_string(),
              data_type: DataType::Int, // will change based on  ParameterData::data_type.
          },
      ],
      output_type: DataType::Int, // will change based on ParameterData::data_type.
      public: true,
      node_data_creator: || Box::new(ParameterData {
        param_index: 0,
        param_name: "param".to_string(),
        data_type: DataType::Int,
        sort_order: 0,
        data_type_str: None,
        error: None,
      }),
      node_data_saver: generic_node_data_saver::<ParameterData>,
      node_data_loader: generic_node_data_loader::<ParameterData>,
    });

    ret.add_node_type(NodeType {
      name: "expr".to_string(),
      description: "You can type in a mathematical expression and it will be evaluated on its output pin.
The input pins can be dynamically added on the node editor panel, you can select the name and data type of the input parameters.

The expr node supports scalar arithmetic, vector operations, conditional expressions, and a comprehensive set of built-in mathematical functions. See the atomCAD reference guide for more details.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::None, // will change based on the expression
      public: true,
      node_data_creator: || Box::new(ExprData {
        parameters: vec![
          ExprParameter {
            name: "x".to_string(),
            data_type: DataType::Int,
            data_type_str: None,
          },
        ],
        expression: "x".to_string(),
        expr: None,
        error: None,
        output_type: Some(DataType::Int),
      }),
      node_data_saver: generic_node_data_saver::<ExprData>,
      node_data_loader: expr_data_loader,
    });

    ret.add_node_type(NodeType {
      name: "value".to_string(),
      description: "".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::None,
      public: false,
      node_data_creator: || Box::new(ValueData {
        value: NetworkResult::None,
      }),
      node_data_saver: generic_node_data_saver::<ValueData>,
      node_data_loader: generic_node_data_loader::<ValueData>,
    });

    ret.add_node_type(NodeType {
      name: "map".to_string(),
      description: "Takes an array of values (`xs`), applies the supplied `f` function on all of them and produces an array of the output values.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
          name: "xs".to_string(),
          data_type: DataType::Array(Box::new(DataType::Float)), // will change based on  ParameterData::data_type.
        },
        Parameter {
          name: "f".to_string(),
          data_type: DataType::Function(FunctionType {
            parameter_types: vec![DataType::Float],
            output_type: Box::new(DataType::Float),
          }), // will change based on  ParameterData::data_type.
        },
      ],
      output_type: DataType::Array(Box::new(DataType::Float)), // will change based on the output type
      public: true,
      node_data_creator: || Box::new(MapData {
        input_type: DataType::Float,
        output_type: DataType::Float,
      }),
      node_data_saver: generic_node_data_saver::<MapData>,
      node_data_loader: generic_node_data_loader::<MapData>,
    });

    ret.add_node_type(NodeType {
      name: "string".to_string(),
      description: "Outputs a string value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::String,
      public: true,
      node_data_creator: || Box::new(StringData {
        value: "".to_string(),
      }),
      node_data_saver: generic_node_data_saver::<StringData>,
      node_data_loader: generic_node_data_loader::<StringData>,
    });

    ret.add_node_type(NodeType {
      name: "bool".to_string(),
      description: "Outputs a bool value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::Bool,
      public: true,
      node_data_creator: || Box::new(BoolData {
        value: false
      }),
      node_data_saver: generic_node_data_saver::<BoolData>,
      node_data_loader: generic_node_data_loader::<BoolData>,
    });

    ret.add_node_type(NodeType {
      name: "int".to_string(),
      description: "Outputs an integer value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::Int,
      public: true,
      node_data_creator: || Box::new(IntData {
        value: 0
      }),
      node_data_saver: generic_node_data_saver::<IntData>,
      node_data_loader: generic_node_data_loader::<IntData>,
    });

    ret.add_node_type(NodeType {
      name: "float".to_string(),
      description: "Outputs a float value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::Float,
      public: true,
      node_data_creator: || Box::new(FloatData {
        value: 0.0
      }),
      node_data_saver: generic_node_data_saver::<FloatData>,
      node_data_loader: generic_node_data_loader::<FloatData>,
    });

    ret.add_node_type(NodeType {
      name: "ivec2".to_string(),
      description: "Outputs an IVec2 value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: DataType::Int,
        },
        Parameter {
            name: "y".to_string(),
            data_type: DataType::Int,
        },        
      ],
      output_type: DataType::IVec2,
      public: true,
      node_data_creator: || Box::new(IVec2Data {
        value: IVec2::new(0, 0)
      }),
      node_data_saver: generic_node_data_saver::<IVec2Data>,
      node_data_loader: generic_node_data_loader::<IVec2Data>,
    });

    ret.add_node_type(NodeType {
      name: "ivec3".to_string(),
      description: "Outputs an IVec3 value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: DataType::Int,
        },
        Parameter {
            name: "y".to_string(),
            data_type: DataType::Int,
        },
        Parameter {
            name: "z".to_string(),
            data_type: DataType::Int,
        },        
      ],
      output_type: DataType::IVec3,
      public: true,
      node_data_creator: || Box::new(IVec3Data {
        value: IVec3::new(0, 0, 0)
      }),
      node_data_saver: generic_node_data_saver::<IVec3Data>,
      node_data_loader: generic_node_data_loader::<IVec3Data>,
    });

    ret.add_node_type(NodeType {
      name: "vec2".to_string(),
      description: "Outputs an Vec2 value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: DataType::Float,
        },
        Parameter {
            name: "y".to_string(),
            data_type: DataType::Float,
        },        
      ],
      output_type: DataType::Vec2,
      public: true,
      node_data_creator: || Box::new(Vec2Data {
        value: DVec2::new(0.0, 0.0)
      }),
      node_data_saver: generic_node_data_saver::<Vec2Data>,
      node_data_loader: generic_node_data_loader::<Vec2Data>,
    });

    ret.add_node_type(NodeType {
      name: "vec3".to_string(),
      description: "Outputs an Vec3 value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: DataType::Float,
        },
        Parameter {
            name: "y".to_string(),
            data_type: DataType::Float,
        },
        Parameter {
            name: "z".to_string(),
            data_type: DataType::Float,
        },        
      ],
      output_type: DataType::Vec3,
      public: true,
      node_data_creator: || Box::new(Vec3Data {
        value: DVec3::new(0.0, 0.0, 0.0)
      }),
      node_data_saver: generic_node_data_saver::<Vec3Data>,
      node_data_loader: generic_node_data_loader::<Vec3Data>,
    });

    ret.add_node_type(NodeType {
      name: "range".to_string(),
      description: "Creates an array of integers starting from an integer value and having a specified step between them. The number of integers in the array can also be specified (count).".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
            name: "start".to_string(),
            data_type: DataType::Int,
        },
        Parameter {
            name: "step".to_string(),
            data_type: DataType::Int,
        },
        Parameter {
            name: "count".to_string(),
            data_type: DataType::Int,
        },        
      ],
      output_type: DataType::Array(Box::new(DataType::Int)),
      public: true,
      node_data_creator: || Box::new(RangeData {
        start: 0,
        step: 1,
        count: 1,
      }),
      node_data_saver: generic_node_data_saver::<RangeData>,
      node_data_loader: generic_node_data_loader::<RangeData>,
    });

    ret.add_node_type(NodeType {
      name: "unit_cell".to_string(),
      description: "Produces a `UnitCell` value representing the three lattice basis vectors defined by the lattice parameters `(a, b, c, α, β, γ)`.".to_string(),
      category: NodeTypeCategory::OtherBuiltin,
      parameters: vec![
        Parameter {
            name: "a".to_string(),
            data_type: DataType::Vec3,
        },
        Parameter {
          name: "b".to_string(),
          data_type: DataType::Vec3,
        },
        Parameter {
          name: "c".to_string(),
          data_type: DataType::Vec3,
        },
      ],
      output_type: DataType::UnitCell,
      public: true,
      node_data_creator: || Box::new(UnitCellData {
        cell_length_a: DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
        cell_length_b: DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
        cell_length_c: DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
        cell_angle_alpha: 90.0,
        cell_angle_beta: 90.0,
        cell_angle_gamma: 90.0,
      }),
      node_data_saver: generic_node_data_saver::<UnitCellData>,
      node_data_loader: generic_node_data_loader::<UnitCellData>,
    });

    ret.add_node_type(NodeType {
      name: "rect".to_string(),
      description: "Outputs a rectangle with integer minimum corner coordinates and integer width and height.".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
            name: "min_corner".to_string(),
            data_type: DataType::IVec2,
        },
        Parameter {
          name: "extent".to_string(),
          data_type: DataType::IVec2,
        },
        Parameter {
          name: "d_plane".to_string(),
          data_type: DataType::DrawingPlane,
        },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(RectData {
        min_corner: IVec2::new(-1, -1),
        extent: IVec2::new(2, 2),
      }),
      node_data_saver: generic_node_data_saver::<RectData>,
      node_data_loader: generic_node_data_loader::<RectData>,
    });

    ret.add_node_type(NodeType {
      name: "circle".to_string(),
      description: "Outputs a circle with integer center coordinates and integer radius.".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
            name: "center".to_string(),
            data_type: DataType::IVec2,
        },
        Parameter {
          name: "radius".to_string(),
          data_type: DataType::Int,
        },
        Parameter {
          name: "d_plane".to_string(),
          data_type: DataType::DrawingPlane,
        },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(CircleData {
        center: IVec2::new(0, 0),
        radius: 1,
      }),
      node_data_saver: generic_node_data_saver::<CircleData>,
      node_data_loader: generic_node_data_loader::<CircleData>,
    });

    ret.add_node_type(NodeType {
      name: "reg_poly".to_string(),
      description: "Outputs a regular polygon with integer radius. The number of sides is a property too.
Now that we have general polygon node this node is less used.".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
          name: "d_plane".to_string(),
          data_type: DataType::DrawingPlane,
        },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(RegPolyData {
        num_sides: 3,
        radius: 3,
      }),
      node_data_saver: generic_node_data_saver::<RegPolyData>,
      node_data_loader: generic_node_data_loader::<RegPolyData>,
    });

    ret.add_node_type(NodeType {
      name: "polygon".to_string(),
      description: "Outputs a general polygon with integer coordinate vertices. Both convex and concave polygons can be created with this node.
The vertices can be freely dragged.
You can create a new vertex by dragging an edge.
Delete a vertex by dragging it onto one of its neighbour.".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
          name: "d_plane".to_string(),
          data_type: DataType::DrawingPlane,
        },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(PolygonData {
        vertices: vec![
          IVec2::new(-1, -1),
          IVec2::new(1, -1),
          IVec2::new(0, 1),
        ],
      }),
      node_data_saver: generic_node_data_saver::<PolygonData>,
      node_data_loader: generic_node_data_loader::<PolygonData>,
    });

    ret.add_node_type(NodeType {
      name: "union_2d".to_string(),
      description: "Computes the Boolean union of any number of 2D geometries. The `shapes` input accepts an array of `Geometry2D` values (array-typed input; you can connect multiple wires and they will be concatenated).".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry2D)),
          },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(Union2DData {}),
      node_data_saver: generic_node_data_saver::<Union2DData>,
      node_data_loader: generic_node_data_loader::<Union2DData>,
    });

    ret.add_node_type(NodeType {
      name: "intersect_2d".to_string(),
      description: "Computes the Boolean intersection of any number of 2D geometries. The `shapes` input pin accepts an array of `Geometry2D` values.".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry2D)),
          },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(Intersect2DData {}),
      node_data_saver: generic_node_data_saver::<Intersect2DData>,
      node_data_loader: generic_node_data_loader::<Intersect2DData>,
    });

    ret.add_node_type(NodeType {
      name: "diff_2d".to_string(),
      description: "Computes the Boolean difference of two 2D geometries.".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
          Parameter {
              name: "base".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry2D)), // A set of shapes to subtract from
          },
          Parameter {
              name: "sub".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry2D)), // A set of shapes to subtract from base
          },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(Diff2DData {}),
      node_data_saver: generic_node_data_saver::<Diff2DData>,
      node_data_loader: generic_node_data_loader::<Diff2DData>,
    });

    ret.add_node_type(NodeType {
      name: "half_plane".to_string(),
      description: "Outputs a half plane.
You can manipulate the two integer coordinate vertices which define the boundary line of the half plane.
Both vertices are displayed as a triangle-based prism. The direction of the half plane is indicated by the direction of the triangle.".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
          name: "d_plane".to_string(),
          data_type: DataType::DrawingPlane,
        },
        Parameter {
          name: "m_index".to_string(),
          data_type: DataType::IVec2,
        },
        Parameter {
          name: "center".to_string(),
          data_type: DataType::IVec2,
        },
        Parameter {
          name: "shift".to_string(),
          data_type: DataType::Int,
        },
        Parameter {
          name: "subdivision".to_string(),
          data_type: DataType::Int,
        },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(HalfPlaneData {
        point1: IVec2::new(0, 0),
        point2: IVec2::new(1, 0),
      }),
      node_data_saver: generic_node_data_saver::<HalfPlaneData>,
      node_data_loader: generic_node_data_loader::<HalfPlaneData>,
    });

    ret.add_node_type(NodeType {
      name: "extrude".to_string(),
      description: "Extrudes a 2D geometry to a 3D geometry.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry2D,
          },
          Parameter {
            name: "unit_cell".to_string(),
            data_type: DataType::UnitCell,
          },
          Parameter {
            name: "height".to_string(),
            data_type: DataType::Int,
          },  
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(ExtrudeData {
        height: 1,
      }),
      node_data_saver: generic_node_data_saver::<ExtrudeData>,
      node_data_loader: generic_node_data_loader::<ExtrudeData>,
    });

    ret.add_node_type(NodeType {
      name: "cuboid".to_string(),
      description: "Outputs a cuboid with integer minimum corner coordinates and integer extent coordinates. If the unit cell is not cubic, the shape will not necessarily be a cuboid: in the most general case it will be a parallelepiped.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
        Parameter {
            name: "min_corner".to_string(),
            data_type: DataType::IVec3,
        },
        Parameter {
          name: "extent".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          name: "unit_cell".to_string(),
          data_type: DataType::UnitCell,
        },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(CuboidData {
        min_corner: IVec3::new(0, 0, 0),
        extent: IVec3::new(1, 1, 1),
      }),
      node_data_saver: generic_node_data_saver::<CuboidData>,
      node_data_loader: generic_node_data_loader::<CuboidData>,
    });

    ret.add_node_type(NodeType {
      name: "sphere".to_string(),
      description: "Outputs a sphere with integer center coordinates and integer radius.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
        Parameter {
            name: "center".to_string(),
            data_type: DataType::IVec3,
        },
        Parameter {
          name: "radius".to_string(),
          data_type: DataType::Int,
        },
        Parameter {
          name: "unit_cell".to_string(),
          data_type: DataType::UnitCell,
        },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(SphereData {
        center: IVec3::new(0, 0, 0),
        radius: 1,
      }),
      node_data_saver: generic_node_data_saver::<SphereData>,
      node_data_loader: generic_node_data_loader::<SphereData>,
    });

    ret.add_node_type(NodeType {
      name: "half_space".to_string(),
      description: "Outputs a half-space (the region on one side of an infinite plane).".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
        Parameter {
          name: "unit_cell".to_string(),
          data_type: DataType::UnitCell,
        },
        Parameter {
          name: "m_index".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          name: "center".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          name: "shift".to_string(),
          data_type: DataType::Int,
        },
        Parameter {
          name: "subdivision".to_string(),
          data_type: DataType::Int,
        },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(HalfSpaceData {
        max_miller_index: 1,
        miller_index: IVec3::new(0, 0, 1), // Default normal along z-axis
        center: IVec3::new(0, 0, 0),
        shift: 0,
        subdivision: 1,
      }),
      node_data_saver: generic_node_data_saver::<HalfSpaceData>,
      node_data_loader: generic_node_data_loader::<HalfSpaceData>,
    });

    ret.add_node_type(NodeType {
      name: "drawing_plane".to_string(),
      description: "Defines a 2D drawing plane on a crystallographic plane with Miller indices. Use this to specify where 2D shapes are placed before extrusion.".to_string(),
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
          name: "unit_cell".to_string(),
          data_type: DataType::UnitCell,
        },
        Parameter {
          name: "m_index".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          name: "center".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          name: "shift".to_string(),
          data_type: DataType::Int,
        },
        Parameter {
          name: "subdivision".to_string(),
          data_type: DataType::Int,
        },
      ],
      output_type: DataType::DrawingPlane,
      public: true,
      node_data_creator: || Box::new(DrawingPlaneData {
        max_miller_index: 1,
        miller_index: IVec3::new(0, 0, 1), // Default normal along z-axis (001 plane)
        center: IVec3::new(0, 0, 0),
        shift: 0,
        subdivision: 1,
      }),
      node_data_saver: generic_node_data_saver::<DrawingPlaneData>,
      node_data_loader: generic_node_data_loader::<DrawingPlaneData>,
    });

    ret.add_node_type(NodeType {
      name: "facet_shell".to_string(),
      description: "Builds a finite polyhedral shell by clipping an infinite lattice with a user‑supplied set of half‑spaces.
See the atomCAD reference guide for more details.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
        Parameter {
          name: "unit_cell".to_string(),
          data_type: DataType::UnitCell,
        },
        Parameter {
          name: "center".to_string(),
          data_type: DataType::IVec3,
        },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(FacetShellData::default()),
      node_data_saver: generic_node_data_saver::<FacetShellData>,
      node_data_loader: generic_node_data_loader::<FacetShellData>,
    });

    ret.add_node_type(NodeType {
      name: "union".to_string(),
      description: "Computes the Boolean union of any number of 3D geometries. The `shapes` input accepts an array of `Geometry` values (array-typed input; you can connect multiple wires and they will be concatenated).".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry)),
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(UnionData {}),
      node_data_saver: generic_node_data_saver::<UnionData>,
      node_data_loader: generic_node_data_loader::<UnionData>,
    });

    ret.add_node_type(NodeType {
      name: "intersect".to_string(),
      description: "Computes the Boolean intersection of any number of 3D geometries. The `shapes` input accepts an array of `Geometry` values. Use this to cut geometries with a half-space.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry)),
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(IntersectData {}),
      node_data_saver: generic_node_data_saver::<IntersectData>,
      node_data_loader: generic_node_data_loader::<IntersectData>,
    });

    ret.add_node_type(NodeType {
      name: "diff".to_string(),
      description: "Computes the Boolean difference of two 3D geometries.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "base".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry)), // If multiple shapes are given, they are unioned.
          },
          Parameter {
              name: "sub".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry)), // A set of shapes to subtract from base
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(DiffData {}),
      node_data_saver: generic_node_data_saver::<DiffData>,
      node_data_loader: generic_node_data_loader::<DiffData>,
    });

    ret.add_node_type(NodeType {
      name: "geo_trans".to_string(),
      description: "".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
            name: "translation".to_string(),
            data_type: DataType::IVec3,
          },
          Parameter {
            name: "rotation".to_string(),
            data_type: DataType::IVec3,
          },
      ],
      output_type: DataType::Geometry,
      public: false,
      node_data_creator: || Box::new(GeoTransData {
        translation: IVec3::new(0, 0, 0),
        rotation: IVec3::new(0, 0, 0),
        transform_only_frame: false,
      }),
      node_data_saver: generic_node_data_saver::<GeoTransData>,
      node_data_loader: generic_node_data_loader::<GeoTransData>,
    });

    ret.add_node_type(NodeType {
      name: "lattice_symop".to_string(),
      description: "".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
            name: "translation".to_string(),
            data_type: DataType::IVec3,
          },
          Parameter {
            name: "rot_axis".to_string(),
            data_type: DataType::Vec3,
          },
          Parameter {
            name: "rot_angle".to_string(),
            data_type: DataType::Float,
          },
          Parameter {
            name: "keep_geo".to_string(),
            data_type: DataType::Float,
          },
      ],
      output_type: DataType::Geometry,
      public: false,
      node_data_creator: || Box::new(LatticeSymopData {
        translation: IVec3::new(0, 0, 0),
        rotation_axis: None,
        rotation_angle_degrees: 0.0,
        transform_only_frame: false,
      }),
      node_data_saver: generic_node_data_saver::<LatticeSymopData>,
      node_data_loader: generic_node_data_loader::<LatticeSymopData>,
    });

    ret.add_node_type(NodeType {
      name: "lattice_move".to_string(),
      description: "Moves the geometry in the discrete lattice space with a relative vector.
Continuous transformation in the lattice space is not allowed (for continuous transformations use the `atom_trans` node which is only available for atomic structures).
You can directly enter the translation vector or drag the axes of the gadget.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
            name: "translation".to_string(),
            data_type: DataType::IVec3,
          },
          Parameter {
            name: "subdivision".to_string(),
            data_type: DataType::Int,
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(LatticeMoveData {
        translation: IVec3::new(0, 0, 0),
        lattice_subdivision: 1,
      }),
      node_data_saver: generic_node_data_saver::<LatticeMoveData>,
      node_data_loader: generic_node_data_loader::<LatticeMoveData>,
    });

    ret.add_node_type(NodeType {
      name: "lattice_rot".to_string(),
      description: "Rotates geometry in lattice space.
Only rotations that are symmetries of the currently selected unit cell are allowed — the node exposes only those valid lattice-symmetry rotations.
You may provide a pivot point for the rotation; by default the pivot is the origin `(0,0,0)`.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
            name: "axis_index".to_string(),
            data_type: DataType::Int,
          },
          Parameter {
            name: "step".to_string(),
            data_type: DataType::Int,
          },
          Parameter {
            name: "pivot_point".to_string(),
            data_type: DataType::IVec3,
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(LatticeRotData {
        axis_index: None,
        step: 0,
        pivot_point: IVec3::new(0, 0, 0),
      }),
      node_data_saver: generic_node_data_saver::<LatticeRotData>,
      node_data_loader: generic_node_data_loader::<LatticeRotData>,
    });

    ret.add_node_type(NodeType {
      name: "motif".to_string(),
      description: "The `motif` node produces a `Motif` value which can be an input to an `atom_fill` node and determines the content which fills the provided geometry.
The motif is defined textually using atomCAD's motif definition language.
The features of the language are basically parameterized fractional atom sites, explicit & periodic bond definitions.
See the atomCAD reference guide for details on the motif definition language.".to_string(),
      category: NodeTypeCategory::OtherBuiltin,
      parameters: vec![],
      output_type: DataType::Motif,
      public: true,
      node_data_creator: || Box::new(MotifData {
        definition: "".to_string(),
        name: None,
        motif: None,
        error: None,
      }),
      node_data_saver: generic_node_data_saver::<MotifData>,
      node_data_loader: motif_data_loader,
    });

    ret.add_node_type(NodeType {
      name: "atom_fill".to_string(),
      description: "Converts a 3D geometry into an atomic structure by carving out a crystal from an infinite crystal lattice using the geometry on its `shape` input.".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
              name: "motif".to_string(),
              data_type: DataType::Motif,
          },
          Parameter {
              name: "m_offset".to_string(),
              data_type: DataType::Vec3,
          },
          Parameter {
              name: "passivate".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              name: "rm_single".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              name: "surf_recon".to_string(),
              data_type: DataType::Bool,
          },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(AtomFillData {
        parameter_element_value_definition: String::new(),
        motif_offset: DVec3::ZERO,
        hydrogen_passivation: true,
        remove_single_bond_atoms_before_passivation: false,
        surface_reconstruction: false,
        error: None,
        parameter_element_values: HashMap::new(),
      }),
      node_data_saver: generic_node_data_saver::<AtomFillData>,
      node_data_loader: generic_node_data_loader::<AtomFillData>,
    });

    ret.add_node_type(NodeType {
      name: "edit_atom".to_string(),
      description: "Note: The `edit_atom` node will be more usable when we will support atomic structure relaxations.
This node enables the manual editing of atomic structures. In a node network every single atomic modification could be placed into a separate node but this would usually lead to a very complex node network. In atomCAD we made a compromise: an edit_atom_node is a set of atomic editing commands. The user can freely group atomic editing commands into edit_atom_nodes at their will.".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
          },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(EditAtomData::new()),
      node_data_saver: |node_data, _design_dir| {
        if let Some(data) = node_data.as_any_mut().downcast_ref::<EditAtomData>() {
          let serializable_data = edit_atom_data_to_serializable(data)?;
          serde_json::to_value(serializable_data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        } else {
          Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for edit_atom"))
        }
      },
      node_data_loader: |value, _design_dir| {
        let serializable_data: SerializableEditAtomData = serde_json::from_value(value.clone())
          .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Box::new(serializable_to_edit_atom_data(&serializable_data)?))
      },
    });

    ret.add_node_type(NodeType {
      name: "atom_trans".to_string(),
      description: "The atom_trans node transforms atomic structures. The transformation happens not in integer lattice space but in continuous space (real-space) where one unit is one angstrom.
  By dragging the gadget axes you can move the structure. By dragging the thicker end of the gadget axes you can rotate the structure.  ".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
          },
          Parameter {
            name: "translation".to_string(),
            data_type: DataType::Vec3,
          },
          Parameter {
            name: "rotation".to_string(),
            data_type: DataType::Vec3,
          },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(AtomTransData {
        translation: DVec3::new(0.0, 0.0, 0.0),
        rotation: DVec3::new(0.0, 0.0, 0.0),
      }),
      node_data_saver: generic_node_data_saver::<AtomTransData>,
      node_data_loader: generic_node_data_loader::<AtomTransData>,
    });

    ret.add_node_type(NodeType {
      name: "import_xyz".to_string(),
      description: "Imports an atomic structure from an xyz file.
It converts file paths to relative paths whenever possible (if the file is in the same directory as the node or in a subdirectory) so that when you copy your whole project to another location or machine the XYZ file references will remain valid.".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
        Parameter {
          name: "file_name".to_string(),
          data_type: DataType::String,
        },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(ImportXYZData::new()),
      node_data_saver: import_xyz_data_saver,
      node_data_loader: import_xyz_data_loader,
    });

    ret.add_node_type(NodeType {
      name: "export_xyz".to_string(),
      description: "Exports atomic structure on its `molecule` input into an XYZ file.".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
        Parameter {
          name: "molecule".to_string(),
          data_type: DataType::Atomic,
        },
        Parameter {
          name: "file_name".to_string(),
          data_type: DataType::String,
        },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(ExportXYZData::new()),
      node_data_saver: export_xyz_data_saver,
      node_data_loader: export_xyz_data_loader,
    });

    ret.add_node_type(NodeType {
      name: "atom_cut".to_string(),
      description: "Cuts an atomic structure using cutter geometries.".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
          },
          Parameter {
            name: "cutters".to_string(),
            data_type: DataType::Array(Box::new(DataType::Geometry)),
        },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(AtomCutData::new()),
      node_data_saver: generic_node_data_saver::<AtomCutData>,
      node_data_loader: generic_node_data_loader::<AtomCutData>,
    });

    ret.add_node_type(NodeType {
      name: "relax".to_string(),
      description: "".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
          },
      ],
      output_type: DataType::Atomic,
      public: false,
      node_data_creator: || Box::new(RelaxData {}),
      node_data_saver: generic_node_data_saver::<RelaxData>,
      node_data_loader: generic_node_data_loader::<RelaxData>,
    });

    return ret;
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
















