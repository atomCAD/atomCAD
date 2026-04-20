use crate::crystolecule::atomic_constants::ATOM_INFO;
use crate::crystolecule::atomic_structure::{
    AtomicStructure, BOND_AROMATIC, BOND_DATIVE, BOND_DOUBLE, BOND_METALLIC, BOND_QUADRUPLE,
    BOND_SINGLE, BOND_TRIPLE,
};
use crate::crystolecule::drawing_plane::DrawingPlane;
use crate::crystolecule::motif::Motif;
use crate::crystolecule::structure::Structure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;
use glam::f64::DVec3;
use glam::i32::IVec2;
use glam::i32::IVec3;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct GeometrySummary2D {
    pub drawing_plane: DrawingPlane,
    pub frame_transform: Transform2D,
    pub geo_tree_root: GeoNode,
}

impl GeometrySummary2D {
    /// Returns a detailed string representation for snapshot testing.
    pub fn to_detailed_string(&self) -> String {
        let dp = &self.drawing_plane;
        let t = &self.frame_transform;
        format!(
            "drawing_plane:\n  miller_index: ({}, {}, {})\n  center: ({}, {}, {})\n  shift: {}\n  subdivision: {}\n  u_axis: ({}, {}, {})\n  v_axis: ({}, {}, {})\nframe_transform:\n  translation: ({:.6}, {:.6})\n  rotation: {:.6}\ngeo_tree:\n{}",
            dp.miller_index.x,
            dp.miller_index.y,
            dp.miller_index.z,
            dp.center.x,
            dp.center.y,
            dp.center.z,
            dp.shift,
            dp.subdivision,
            dp.u_axis.x,
            dp.u_axis.y,
            dp.u_axis.z,
            dp.v_axis.x,
            dp.v_axis.y,
            dp.v_axis.z,
            t.translation.x,
            t.translation.y,
            t.rotation,
            self.geo_tree_root
        )
    }

    /// Checks if this geometry's drawing plane is compatible with another geometry's drawing plane.
    ///
    /// This is useful for CSG operations where geometries must have compatible drawing planes.
    /// Uses approximate equality with tolerance for small calculation errors.
    ///
    /// # Arguments
    /// * `other` - The other GeometrySummary2D to compare drawing planes with
    ///
    /// # Returns
    /// * `true` if the drawing planes are compatible (same unit cell and plane orientation)
    /// * `false` if they differ significantly
    pub fn has_compatible_drawing_plane(&self, other: &GeometrySummary2D) -> bool {
        self.drawing_plane.is_compatible(&other.drawing_plane)
    }

    /// Checks if all geometries in a vector have compatible drawing planes.
    ///
    /// Compares each geometry's drawing plane to the first geometry's drawing plane.
    /// Returns true if the vector is empty or has only one element.
    ///
    /// # Arguments
    /// * `geometries` - Vector of GeometrySummary2D objects to check
    ///
    /// # Returns
    /// * `true` if all drawing planes are compatible or vector has ≤1 elements
    /// * `false` if any drawing plane is incompatible with the first
    pub fn all_have_compatible_drawing_planes(geometries: &[GeometrySummary2D]) -> bool {
        if geometries.len() <= 1 {
            return true;
        }

        let first_drawing_plane = &geometries[0].drawing_plane;
        geometries
            .iter()
            .skip(1)
            .all(|geometry| first_drawing_plane.is_compatible(&geometry.drawing_plane))
    }
}

/// Tracks a Blueprint/Crystal's registration to its underlying Structure's symmetry.
/// Totally ordered: `Aligned < MotifUnaligned < LatticeUnaligned`. Propagation is `max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Alignment {
    #[default]
    Aligned,
    MotifUnaligned,
    LatticeUnaligned,
}

impl Alignment {
    /// Degrades `self` to `other` if `other` is worse (higher in the ordering).
    pub fn worsen_to(&mut self, other: Self) {
        *self = (*self).max(other);
    }
}

#[derive(Clone)]
pub struct BlueprintData {
    pub structure: Structure,
    pub geo_tree_root: GeoNode,
    pub alignment: Alignment,
}

impl BlueprintData {
    /// Returns a detailed string representation for snapshot testing.
    pub fn to_detailed_string(&self) -> String {
        let lv = &self.structure.lattice_vecs;
        let mo = &self.structure.motif_offset;
        format!(
            "lattice_vecs:\n  a: ({:.6}, {:.6}, {:.6})\n  b: ({:.6}, {:.6}, {:.6})\n  c: ({:.6}, {:.6}, {:.6})\nmotif_offset: ({:.6}, {:.6}, {:.6})\ngeo_tree:\n{}",
            lv.a.x,
            lv.a.y,
            lv.a.z,
            lv.b.x,
            lv.b.y,
            lv.b.z,
            lv.c.x,
            lv.c.y,
            lv.c.z,
            mo.x,
            mo.y,
            mo.z,
            self.geo_tree_root
        )
    }

    /// Checks if this blueprint's lattice vectors are compatible with another's.
    ///
    /// Boolean CSG operations require compatible lattice vectors. Uses approximate
    /// equality with tolerance for small calculation errors. Motif compatibility
    /// is not required at this level.
    pub fn has_compatible_lattice_vecs(&self, other: &BlueprintData) -> bool {
        self.structure
            .lattice_vecs
            .is_approximately_equal(&other.structure.lattice_vecs)
    }

    /// Checks if all blueprints in a vector have approximately the same lattice vectors.
    /// Returns true if the vector is empty or has only one element.
    pub fn all_have_compatible_lattice_vecs(blueprints: &[BlueprintData]) -> bool {
        if blueprints.len() <= 1 {
            return true;
        }

        let first = &blueprints[0].structure.lattice_vecs;
        blueprints
            .iter()
            .skip(1)
            .all(|bp| first.is_approximately_equal(&bp.structure.lattice_vecs))
    }
}

#[derive(Clone)]
pub struct CrystalData {
    pub structure: Structure,
    pub atoms: AtomicStructure,
    pub geo_tree_root: Option<GeoNode>,
    pub alignment: Alignment,
}

#[derive(Clone)]
pub struct MoleculeData {
    pub atoms: AtomicStructure,
    pub geo_tree_root: Option<GeoNode>,
}

#[derive(Clone)]
pub struct Closure {
    pub node_network_name: String,
    pub node_id: u64,
    pub captured_argument_values: Vec<NetworkResult>,
}

#[derive(Clone, Default)]
pub enum NetworkResult {
    #[default]
    None, // Always equivalent with no input pin connected
    Bool(bool),
    String(String),
    Int(i32),
    Float(f64),
    Vec2(DVec2),
    Vec3(DVec3),
    IVec2(IVec2),
    IVec3(IVec3),
    LatticeVecs(UnitCellStruct),
    DrawingPlane(DrawingPlane),
    Geometry2D(GeometrySummary2D),
    Blueprint(BlueprintData),
    Crystal(CrystalData),
    Molecule(MoleculeData),
    Motif(Motif),
    Structure(Structure),
    Array(Vec<NetworkResult>),
    Function(Closure),
    Error(String),
}

impl NetworkResult {
    /// Returns the DataType corresponding to this result's variant,
    /// or None for variants without a clear single type (None, Error, Function, Array).
    pub fn infer_data_type(&self) -> Option<DataType> {
        match self {
            NetworkResult::Bool(_) => Some(DataType::Bool),
            NetworkResult::String(_) => Some(DataType::String),
            NetworkResult::Int(_) => Some(DataType::Int),
            NetworkResult::Float(_) => Some(DataType::Float),
            NetworkResult::Vec2(_) => Some(DataType::Vec2),
            NetworkResult::Vec3(_) => Some(DataType::Vec3),
            NetworkResult::IVec2(_) => Some(DataType::IVec2),
            NetworkResult::IVec3(_) => Some(DataType::IVec3),
            NetworkResult::LatticeVecs(_) => Some(DataType::LatticeVecs),
            NetworkResult::DrawingPlane(_) => Some(DataType::DrawingPlane),
            NetworkResult::Geometry2D(_) => Some(DataType::Geometry2D),
            NetworkResult::Blueprint(_) => Some(DataType::Blueprint),
            NetworkResult::Crystal(_) => Some(DataType::Crystal),
            NetworkResult::Molecule(_) => Some(DataType::Molecule),
            NetworkResult::Motif(_) => Some(DataType::Motif),
            NetworkResult::Structure(_) => Some(DataType::Structure),
            _ => None,
        }
        .map(|t| {
            debug_assert!(!t.is_abstract(), "infer_data_type returned abstract type");
            t
        })
    }

    /// Returns true if this NetworkResult is an Error variant
    pub fn is_error(&self) -> bool {
        matches!(self, NetworkResult::Error(_))
    }

    /// If this is an Error variant, returns it. Otherwise returns None.
    /// Useful for early error propagation in node evaluation.
    pub fn propagate_error(self) -> Option<NetworkResult> {
        match self {
            NetworkResult::Error(_) => Some(self),
            _ => None,
        }
    }

    /// Extracts an UnitCellStruct value from the NetworkResult, returns None if not a LatticeVecs
    pub fn extract_unit_cell(self) -> Option<UnitCellStruct> {
        match self {
            NetworkResult::LatticeVecs(uc) => Some(uc),
            _ => None,
        }
    }

    /// Extracts a DrawingPlane value from the NetworkResult, returns None if not a DrawingPlane
    pub fn extract_drawing_plane(self) -> Option<DrawingPlane> {
        match self {
            NetworkResult::DrawingPlane(dp) => Some(dp),
            _ => None,
        }
    }

    /// Returns the UnitCellStruct associated with this NetworkResult.
    /// For LatticeVecs, DrawingPlane, Geometry2D, and Blueprint variants, returns their unit cell.
    /// For all other variants, returns None.
    pub fn get_unit_cell(&self) -> Option<UnitCellStruct> {
        match self {
            NetworkResult::LatticeVecs(unit_cell) => Some(unit_cell.clone()),
            NetworkResult::DrawingPlane(drawing_plane) => Some(drawing_plane.unit_cell.clone()),
            NetworkResult::Geometry2D(geometry) => Some(geometry.drawing_plane.unit_cell.clone()),
            NetworkResult::Blueprint(bp) => Some(bp.structure.lattice_vecs.clone()),
            NetworkResult::Crystal(c) => Some(c.structure.lattice_vecs.clone()),
            NetworkResult::Structure(structure) => Some(structure.lattice_vecs.clone()),
            _ => None,
        }
    }

    /// Extracts an IVec3 value from the NetworkResult, returns None if not an IVec3
    pub fn extract_ivec3(self) -> Option<IVec3> {
        match self {
            NetworkResult::IVec3(vec) => Some(vec),
            _ => None,
        }
    }

    /// Extracts an IVec2 value from the NetworkResult, returns None if not an IVec2
    pub fn extract_ivec2(self) -> Option<IVec2> {
        match self {
            NetworkResult::IVec2(vec) => Some(vec),
            _ => None,
        }
    }

    /// Extracts a String value from the NetworkResult, returns None if not a String
    pub fn extract_string(self) -> Option<String> {
        match self {
            NetworkResult::String(value) => Some(value),
            _ => None,
        }
    }

    /// Extracts a Bool value from the NetworkResult, returns None if not a Bool
    pub fn extract_bool(self) -> Option<bool> {
        match self {
            NetworkResult::Bool(value) => Some(value),
            _ => None,
        }
    }

    /// Extracts an Int value from the NetworkResult, returns None if not an Int
    pub fn extract_int(self) -> Option<i32> {
        match self {
            NetworkResult::Int(value) => Some(value),
            _ => None,
        }
    }

    /// Extracts a Float value from the NetworkResult, returns None if not a Float
    pub fn extract_float(self) -> Option<f64> {
        match self {
            NetworkResult::Float(value) => Some(value),
            _ => None,
        }
    }

    /// Extracts a Vec2 value from the NetworkResult, returns None if not a Vec2
    pub fn extract_vec2(self) -> Option<DVec2> {
        match self {
            NetworkResult::Vec2(vec) => Some(vec),
            _ => None,
        }
    }

    /// Extracts a Vec3 value from the NetworkResult, returns None if not a Vec3
    pub fn extract_vec3(self) -> Option<DVec3> {
        match self {
            NetworkResult::Vec3(vec) => Some(vec),
            _ => None,
        }
    }

    /// Extracts an optional Vec3 value from the NetworkResult
    /// Returns Some(None) if NetworkResult::None (no input connected)
    /// Returns Some(Some(vec)) if NetworkResult::Vec3(vec)
    /// Returns None if not a Vec3 or None variant
    pub fn extract_optional_dvec3(self) -> Option<Option<DVec3>> {
        match self {
            NetworkResult::None => Some(None),
            NetworkResult::Vec3(vec) => Some(Some(vec)),
            _ => None,
        }
    }

    /// Extracts an optional Int value from the NetworkResult
    /// Returns Some(None) if NetworkResult::None (no input connected)
    /// Returns Some(Some(value)) if NetworkResult::Int(value)
    /// Returns None if not an Int or None variant
    pub fn extract_optional_int(self) -> Option<Option<i32>> {
        match self {
            NetworkResult::None => Some(None),
            NetworkResult::Int(value) => Some(Some(value)),
            _ => None,
        }
    }

    /// Extracts an AtomicStructure value from the NetworkResult.
    /// Accepts both Crystal and Molecule variants (the abstract Atomic supertype).
    pub fn extract_atomic(self) -> Option<AtomicStructure> {
        match self {
            NetworkResult::Crystal(c) => Some(c.atoms),
            NetworkResult::Molecule(m) => Some(m.atoms),
            _ => None,
        }
    }

    /// Extracts a CrystalData value from the NetworkResult, returns None if not a Crystal.
    pub fn extract_crystal(self) -> Option<CrystalData> {
        match self {
            NetworkResult::Crystal(c) => Some(c),
            _ => None,
        }
    }

    /// Extracts a MoleculeData value from the NetworkResult, returns None if not a Molecule.
    pub fn extract_molecule(self) -> Option<MoleculeData> {
        match self {
            NetworkResult::Molecule(m) => Some(m),
            _ => None,
        }
    }

    /// Extracts a Motif value from the NetworkResult, returns None if not a Motif
    pub fn extract_motif(self) -> Option<Motif> {
        match self {
            NetworkResult::Motif(value) => Some(value),
            _ => None,
        }
    }

    /// Extracts a Structure value from the NetworkResult, returns None if not a Structure
    pub fn extract_structure(self) -> Option<Structure> {
        match self {
            NetworkResult::Structure(value) => Some(value),
            _ => None,
        }
    }

    /// Extracts an optional Structure value from the NetworkResult.
    /// Returns Some(None) if NetworkResult::None (no input connected),
    /// Some(Some(structure)) if NetworkResult::Structure(...),
    /// None otherwise.
    pub fn extract_optional_structure(self) -> Option<Option<Structure>> {
        match self {
            NetworkResult::None => Some(None),
            NetworkResult::Structure(value) => Some(Some(value)),
            _ => None,
        }
    }

    /// Converts this NetworkResult to the specified target data type
    /// Returns self if the types already match, otherwise performs conversion
    ///
    /// # Parameters
    /// * `source_type` - The data type of this NetworkResult
    /// * `target_type` - The desired target data type
    pub fn convert_to(self, source_type: &DataType, target_type: &DataType) -> NetworkResult {
        // If types already match, return self
        if DataType::can_be_converted_to(source_type, target_type) && source_type == target_type {
            return self;
        }

        // If conversion is possible and both types are functions, return self unmodified
        // Function values don't need runtime conversion - partial evaluation happens at type level
        if DataType::can_be_converted_to(source_type, target_type) {
            if let (DataType::Function(_), DataType::Function(_)) = (source_type, target_type) {
                return self;
            }
        }

        // Handle Error and None cases - they cannot be converted
        match &self {
            NetworkResult::Error(_) | NetworkResult::None => return self,
            _ => {}
        }

        // Check if we can convert T to [T] (single element to array)
        if let DataType::Array(target_element_type) = target_type {
            if DataType::can_be_converted_to(source_type, target_element_type) {
                // Convert the single element to the target element type, then wrap in array
                let converted_element = self.convert_to(source_type, target_element_type);
                return NetworkResult::Array(vec![converted_element]);
            }
        }

        // Handle array to array conversion (element-wise conversion)
        if let (DataType::Array(source_element_type), DataType::Array(target_element_type)) =
            (source_type, target_type)
        {
            if let NetworkResult::Array(elements) = self {
                let converted_elements: Vec<NetworkResult> = elements
                    .into_iter()
                    .map(|element| element.convert_to(source_element_type, target_element_type))
                    .collect();
                return NetworkResult::Array(converted_elements);
            }
        }

        // Perform basic type conversions
        match (self, target_type) {
            // Bool -> Int
            (NetworkResult::Bool(value), DataType::Int) => {
                NetworkResult::Int(if value { 1 } else { 0 })
            }

            // Int -> Bool (0 = false, non-zero = true)
            (NetworkResult::Int(value), DataType::Bool) => NetworkResult::Bool(value != 0),

            // Int -> Float
            (NetworkResult::Int(value), DataType::Float) => NetworkResult::Float(value as f64),

            // Float -> Int (rounded)
            (NetworkResult::Float(value), DataType::Int) => {
                NetworkResult::Int(value.round() as i32)
            }

            // IVec2 -> Vec2
            (NetworkResult::IVec2(vec), DataType::Vec2) => {
                NetworkResult::Vec2(DVec2::new(vec.x as f64, vec.y as f64))
            }

            // Vec2 -> IVec2 (rounded)
            (NetworkResult::Vec2(vec), DataType::IVec2) => {
                NetworkResult::IVec2(IVec2::new(vec.x.round() as i32, vec.y.round() as i32))
            }

            // IVec3 -> Vec3
            (NetworkResult::IVec3(vec), DataType::Vec3) => {
                NetworkResult::Vec3(DVec3::new(vec.x as f64, vec.y as f64, vec.z as f64))
            }

            // Vec3 -> IVec3 (rounded)
            (NetworkResult::Vec3(vec), DataType::IVec3) => NetworkResult::IVec3(IVec3::new(
                vec.x.round() as i32,
                vec.y.round() as i32,
                vec.z.round() as i32,
            )),

            // LatticeVecs -> DrawingPlane (backward compatibility for old .cnnd files)
            // Creates a standard XY plane (001 Miller index) at the origin
            (NetworkResult::LatticeVecs(unit_cell), DataType::DrawingPlane) => {
                match DrawingPlane::new(
                    unit_cell,
                    IVec3::new(0, 0, 1), // XY plane (001 Miller index)
                    IVec3::new(0, 0, 0), // Center at origin
                    0,                   // No shift
                    1,                   // Subdivision = 1
                ) {
                    Ok(drawing_plane) => NetworkResult::DrawingPlane(drawing_plane),
                    Err(err_msg) => NetworkResult::Error(format!(
                        "Failed to convert LatticeVecs to DrawingPlane: {}",
                        err_msg
                    )),
                }
            }

            (original, _target) => {
                /*
                NetworkResult::Error(format!(
                  "Cannot convert {:?} to {:?}",
                  source_type,
                  target
                ))
                */
                original
            } /*
              we could return a runtime error here, but for technical reasons None types are converted
              to any value in runtime (due to the Value node), so we just return self for now.
              */
        }
    }

    /// Returns a user-readable string representation for all variants.
    /// For complex variants like Geometry2D, Blueprint, Atomic, and Error, returns the variant name.
    pub fn to_display_string(&self) -> String {
        match self {
            NetworkResult::None => "None".to_string(),
            NetworkResult::Bool(value) => value.to_string(),
            NetworkResult::String(value) => value.to_string(),
            NetworkResult::Int(value) => value.to_string(),
            NetworkResult::Float(value) => format!("{:.6}", value),
            NetworkResult::Vec2(vec) => format!("({:.6}, {:.6})", vec.x, vec.y),
            NetworkResult::Vec3(vec) => format!("({:.6}, {:.6}, {:.6})", vec.x, vec.y, vec.z),
            NetworkResult::IVec2(vec) => format!("({}, {})", vec.x, vec.y),
            NetworkResult::IVec3(vec) => format!("({}, {}, {})", vec.x, vec.y, vec.z),
            NetworkResult::Array(elements) => {
                let element_strings: Vec<String> = elements
                    .iter()
                    .map(|element| element.to_display_string())
                    .collect();
                format!("[{}]", element_strings.join(", "))
            }
            NetworkResult::Function(closure) => format!(
                "network: {} node: {}",
                closure.node_network_name, closure.node_id
            ),
            NetworkResult::LatticeVecs(unit_cell) => {
                format!(
                    "LatticeVecs:\n  a: ({:.6}, {:.6}, {:.6})\n  b: ({:.6}, {:.6}, {:.6})\n  c: ({:.6}, {:.6}, {:.6})",
                    unit_cell.a.x,
                    unit_cell.a.y,
                    unit_cell.a.z,
                    unit_cell.b.x,
                    unit_cell.b.y,
                    unit_cell.b.z,
                    unit_cell.c.x,
                    unit_cell.c.y,
                    unit_cell.c.z
                )
            }
            NetworkResult::DrawingPlane(drawing_plane) => {
                format!(
                    "DrawingPlane: miller_index=({}, {}, {}), center=({}, {}, {}), shift={}, subdivision={}",
                    drawing_plane.miller_index.x,
                    drawing_plane.miller_index.y,
                    drawing_plane.miller_index.z,
                    drawing_plane.center.x,
                    drawing_plane.center.y,
                    drawing_plane.center.z,
                    drawing_plane.shift,
                    drawing_plane.subdivision
                )
            }
            NetworkResult::Geometry2D(_) => "Geometry2D".to_string(),
            NetworkResult::Blueprint(_) => "Blueprint".to_string(),
            NetworkResult::Crystal(c) => format_atomic_display_string(&c.atoms),
            NetworkResult::Molecule(m) => format_atomic_display_string(&m.atoms),
            NetworkResult::Motif(motif) => motif.to_text_format(),
            NetworkResult::Structure(structure) => {
                let uc = &structure.lattice_vecs;
                format!(
                    "Structure:\n  lattice_vecs: a=({:.6}, {:.6}, {:.6}) b=({:.6}, {:.6}, {:.6}) c=({:.6}, {:.6}, {:.6})\n  motif_offset: ({:.6}, {:.6}, {:.6})",
                    uc.a.x,
                    uc.a.y,
                    uc.a.z,
                    uc.b.x,
                    uc.b.y,
                    uc.b.z,
                    uc.c.x,
                    uc.c.y,
                    uc.c.z,
                    structure.motif_offset.x,
                    structure.motif_offset.y,
                    structure.motif_offset.z,
                )
            }
            NetworkResult::Error(_) => "Error".to_string(),
        }
    }

    /// Returns a detailed string representation including full contents for complex types.
    /// For Blueprint/Geometry2D, shows unit cell/drawing plane, frame transform, and geo tree.
    /// For Atomic/Motif, shows counts plus first 10 atoms/sites/bonds.
    /// For other variants, delegates to to_display_string().
    pub fn to_detailed_string(&self) -> String {
        match self {
            NetworkResult::Blueprint(geometry) => {
                format!("Blueprint:\n{}", geometry.to_detailed_string())
            }
            NetworkResult::Geometry2D(geometry) => {
                format!("Geometry2D:\n{}", geometry.to_detailed_string())
            }
            NetworkResult::Crystal(c) => {
                format!("Crystal:\n{}", c.atoms.to_detailed_string())
            }
            NetworkResult::Molecule(m) => {
                format!("Molecule:\n{}", m.atoms.to_detailed_string())
            }
            NetworkResult::Motif(motif) => {
                format!("Motif:\n{}", motif.to_detailed_string())
            }
            NetworkResult::Structure(structure) => {
                format!(
                    "Structure:\n  lattice_vecs:\n    a: ({:.6}, {:.6}, {:.6})\n    b: ({:.6}, {:.6}, {:.6})\n    c: ({:.6}, {:.6}, {:.6})\n  motif_offset: ({:.6}, {:.6}, {:.6})\n  motif:\n{}",
                    structure.lattice_vecs.a.x,
                    structure.lattice_vecs.a.y,
                    structure.lattice_vecs.a.z,
                    structure.lattice_vecs.b.x,
                    structure.lattice_vecs.b.y,
                    structure.lattice_vecs.b.z,
                    structure.lattice_vecs.c.x,
                    structure.lattice_vecs.c.y,
                    structure.lattice_vecs.c.z,
                    structure.motif_offset.x,
                    structure.motif_offset.y,
                    structure.motif_offset.z,
                    structure.motif.to_detailed_string(),
                )
            }
            NetworkResult::Error(msg) => {
                format!("Error: {}", msg)
            }
            _ => self.to_display_string(),
        }
    }

    /// Parse a NetworkResult from a string value based on expected DataType.
    /// Used for CLI parameter parsing.
    pub fn from_string(value_str: &str, data_type: &DataType) -> Result<Self, String> {
        match data_type {
            DataType::Bool => match value_str.to_lowercase().as_str() {
                "true" => Ok(NetworkResult::Bool(true)),
                "false" => Ok(NetworkResult::Bool(false)),
                _ => Err(format!(
                    "Invalid bool value '{}'. Expected 'true' or 'false'",
                    value_str
                )),
            },

            DataType::Int => value_str
                .parse::<i32>()
                .map(NetworkResult::Int)
                .map_err(|_| format!("Invalid int value '{}'", value_str)),

            DataType::Float => value_str
                .parse::<f64>()
                .map(NetworkResult::Float)
                .map_err(|_| format!("Invalid float value '{}'", value_str)),

            DataType::String => Ok(NetworkResult::String(value_str.to_string())),

            DataType::Vec2 => {
                let parts: Vec<&str> = value_str.split(',').map(|s| s.trim()).collect();
                if parts.len() != 2 {
                    return Err(format!(
                        "Vec2 requires 2 comma-separated values, got '{}'",
                        value_str
                    ));
                }
                let x = parts[0]
                    .parse::<f64>()
                    .map_err(|_| format!("Invalid Vec2 x component: '{}'", parts[0]))?;
                let y = parts[1]
                    .parse::<f64>()
                    .map_err(|_| format!("Invalid Vec2 y component: '{}'", parts[1]))?;
                Ok(NetworkResult::Vec2(DVec2::new(x, y)))
            }

            DataType::Vec3 => {
                let parts: Vec<&str> = value_str.split(',').map(|s| s.trim()).collect();
                if parts.len() != 3 {
                    return Err(format!(
                        "Vec3 requires 3 comma-separated values, got '{}'",
                        value_str
                    ));
                }
                let x = parts[0]
                    .parse::<f64>()
                    .map_err(|_| format!("Invalid Vec3 x component: '{}'", parts[0]))?;
                let y = parts[1]
                    .parse::<f64>()
                    .map_err(|_| format!("Invalid Vec3 y component: '{}'", parts[1]))?;
                let z = parts[2]
                    .parse::<f64>()
                    .map_err(|_| format!("Invalid Vec3 z component: '{}'", parts[2]))?;
                Ok(NetworkResult::Vec3(DVec3::new(x, y, z)))
            }

            DataType::IVec2 => {
                let parts: Vec<&str> = value_str.split(',').map(|s| s.trim()).collect();
                if parts.len() != 2 {
                    return Err(format!(
                        "IVec2 requires 2 comma-separated values, got '{}'",
                        value_str
                    ));
                }
                let x = parts[0]
                    .parse::<i32>()
                    .map_err(|_| format!("Invalid IVec2 x component: '{}'", parts[0]))?;
                let y = parts[1]
                    .parse::<i32>()
                    .map_err(|_| format!("Invalid IVec2 y component: '{}'", parts[1]))?;
                Ok(NetworkResult::IVec2(IVec2::new(x, y)))
            }

            DataType::IVec3 => {
                let parts: Vec<&str> = value_str.split(',').map(|s| s.trim()).collect();
                if parts.len() != 3 {
                    return Err(format!(
                        "IVec3 requires 3 comma-separated values, got '{}'",
                        value_str
                    ));
                }
                let x = parts[0]
                    .parse::<i32>()
                    .map_err(|_| format!("Invalid IVec3 x component: '{}'", parts[0]))?;
                let y = parts[1]
                    .parse::<i32>()
                    .map_err(|_| format!("Invalid IVec3 y component: '{}'", parts[1]))?;
                let z = parts[2]
                    .parse::<i32>()
                    .map_err(|_| format!("Invalid IVec3 z component: '{}'", parts[2]))?;
                Ok(NetworkResult::IVec3(IVec3::new(x, y, z)))
            }

            _ => Err(format!("Unsupported CLI parameter type: {}", data_type)),
        }
    }
}

fn format_atomic_display_string(atomic: &AtomicStructure) -> String {
    let num_atoms = atomic.get_num_of_atoms();
    let num_bonds = atomic.get_num_of_bonds();

    // Molecular formula: count atoms by element, sorted by atomic number
    let mut element_counts: BTreeMap<i16, usize> = BTreeMap::new();
    let mut bond_type_counts = [0usize; 8]; // indexed by bond order constant
    for atom in atomic.atoms_values() {
        *element_counts.entry(atom.atomic_number).or_insert(0) += 1;
        for bond in &atom.bonds {
            let order = bond.bond_order();
            if (order as usize) < bond_type_counts.len() {
                bond_type_counts[order as usize] += 1;
            }
        }
    }
    // Each bond is stored in both atoms, so divide by 2
    for count in &mut bond_type_counts {
        *count /= 2;
    }

    // Build molecular formula string (C, H first if present, then rest alphabetically)
    let mut formula_parts: Vec<String> = Vec::new();
    let mut remaining: BTreeMap<&str, usize> = BTreeMap::new();
    // Collect symbols, prioritize C and H (Hill system)
    let mut c_count = 0usize;
    let mut h_count = 0usize;
    for (&atomic_number, &count) in &element_counts {
        if let Some(info) = ATOM_INFO.get(&(atomic_number as i32)) {
            match info.symbol.as_str() {
                "C" => c_count = count,
                "H" => h_count = count,
                _ => {
                    remaining.insert(&info.symbol, count);
                }
            }
        }
    }
    if c_count > 0 {
        if c_count == 1 {
            formula_parts.push("C".to_string());
        } else {
            formula_parts.push(format!("C{}", c_count));
        }
    }
    if h_count > 0 {
        if h_count == 1 {
            formula_parts.push("H".to_string());
        } else {
            formula_parts.push(format!("H{}", h_count));
        }
    }
    for (symbol, count) in &remaining {
        if *count == 1 {
            formula_parts.push(symbol.to_string());
        } else {
            formula_parts.push(format!("{}{}", symbol, count));
        }
    }
    let formula = if formula_parts.is_empty() {
        String::new()
    } else {
        formula_parts.join("")
    };

    // Bond type breakdown
    let bond_labels: &[(u8, &str)] = &[
        (BOND_SINGLE, "single"),
        (BOND_DOUBLE, "double"),
        (BOND_TRIPLE, "triple"),
        (BOND_QUADRUPLE, "quadruple"),
        (BOND_AROMATIC, "aromatic"),
        (BOND_DATIVE, "dative"),
        (BOND_METALLIC, "metallic"),
    ];
    let bond_parts: Vec<String> = bond_labels
        .iter()
        .filter(|(order, _)| bond_type_counts[*order as usize] > 0)
        .map(|(order, label)| format!("{} {}", bond_type_counts[*order as usize], label))
        .collect();

    // Bounding box
    let mut min_pos = DVec3::new(f64::MAX, f64::MAX, f64::MAX);
    let mut max_pos = DVec3::new(f64::MIN, f64::MIN, f64::MIN);
    for atom in atomic.atoms_values() {
        min_pos = min_pos.min(atom.position);
        max_pos = max_pos.max(atom.position);
    }

    let mut lines = Vec::new();
    if atomic.is_diff() {
        lines.push("diff".to_string());
    }
    lines.push(format!("{} atoms, {} bonds", num_atoms, num_bonds));
    if !formula.is_empty() {
        lines.push(formula);
    }
    if !bond_parts.is_empty() {
        lines.push(format!("bonds: {}", bond_parts.join(", ")));
    }
    if num_atoms > 0 {
        let size = max_pos - min_pos;
        lines.push(format!(
            "bbox: {:.1} x {:.1} x {:.1} A",
            size.x, size.y, size.z
        ));
    }
    lines.join("\n")
}

/// Creates a consistent error message for missing input in node evaluation
///
/// # Arguments
/// * `input_name` - The name of the missing input (e.g., 'molecule', 'shape')
///
/// # Returns
/// * `NetworkResult::Error` with a formatted error message
pub fn input_missing_error(input_name: &str) -> NetworkResult {
    NetworkResult::Error(format!("{} input is missing", input_name))
}

pub fn error_in_input(input_name: &str) -> NetworkResult {
    NetworkResult::Error(format!("error in {} input", input_name))
}

pub fn runtime_type_error_in_input(input_param_index: usize) -> NetworkResult {
    NetworkResult::Error(format!(
        "runtime type error in the {} indexed input",
        input_param_index
    ))
}

pub fn unit_cell_mismatch_error() -> NetworkResult {
    NetworkResult::Error("Unit cell mismatch.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_cell_exact_equality() {
        let uc1 = UnitCellStruct {
            a: DVec3::new(1.0, 0.0, 0.0),
            b: DVec3::new(0.0, 1.0, 0.0),
            c: DVec3::new(0.0, 0.0, 1.0),
            cell_length_a: 1.0,
            cell_length_b: 1.0,
            cell_length_c: 1.0,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        };
        let uc2 = UnitCellStruct {
            a: DVec3::new(1.0, 0.0, 0.0),
            b: DVec3::new(0.0, 1.0, 0.0),
            c: DVec3::new(0.0, 0.0, 1.0),
            cell_length_a: 1.0,
            cell_length_b: 1.0,
            cell_length_c: 1.0,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        };

        assert!(uc1.is_approximately_equal(&uc2));
    }

    #[test]
    fn test_unit_cell_approximate_equality() {
        let uc1 = UnitCellStruct {
            a: DVec3::new(1.0, 0.0, 0.0),
            b: DVec3::new(0.0, 1.0, 0.0),
            c: DVec3::new(0.0, 0.0, 1.0),
            cell_length_a: 1.0,
            cell_length_b: 1.0,
            cell_length_c: 1.0,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        };
        let uc2 = UnitCellStruct {
            a: DVec3::new(1.000001, 0.0, 0.0),
            b: DVec3::new(0.0, 0.999999, 0.0),
            c: DVec3::new(0.0, 0.0, 1.000001),
            cell_length_a: 1.000001,
            cell_length_b: 0.999999,
            cell_length_c: 1.000001,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        };

        // Small differences (< 1e-5) should be considered equal
        assert!(uc1.is_approximately_equal(&uc2));
    }

    #[test]
    fn test_unit_cell_significant_difference() {
        let uc1 = UnitCellStruct {
            a: DVec3::new(1.0, 0.0, 0.0),
            b: DVec3::new(0.0, 1.0, 0.0),
            c: DVec3::new(0.0, 0.0, 1.0),
            cell_length_a: 1.0,
            cell_length_b: 1.0,
            cell_length_c: 1.0,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        };
        let uc2 = UnitCellStruct {
            a: DVec3::new(1.0001, 0.0, 0.0), // Difference > 1e-5
            b: DVec3::new(0.0, 1.0, 0.0),
            c: DVec3::new(0.0, 0.0, 1.0),
            cell_length_a: 1.0001,
            cell_length_b: 1.0,
            cell_length_c: 1.0,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        };

        // Significant differences (> 1e-5) should not be considered equal
        assert!(!uc1.is_approximately_equal(&uc2));
    }

    #[test]
    fn test_cubic_diamond_compatibility() {
        let uc1 = UnitCellStruct::cubic_diamond();
        let uc2 = UnitCellStruct::cubic_diamond();

        assert!(uc1.is_approximately_equal(&uc2));
    }
}
