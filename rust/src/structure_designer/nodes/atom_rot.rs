use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::collections::HashSet;
use crate::util::serialization_utils::dvec3_serializer;
use crate::structure_designer::text_format::TextValue;
use crate::renderer::mesh::{Mesh, Material};
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput, tessellate_cylinder, tessellate_cone, tessellate_sphere};
use crate::display::gadget::Gadget;
use glam::f64::DQuat;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::util::hit_test_utils::get_closest_point_on_first_ray;

/// Evaluation cache for atom_rot node.
/// Stores the evaluated pivot point and rotation axis for gadget creation.
#[derive(Debug, Clone)]
pub struct AtomRotEvalCache {
    pub pivot_point: DVec3,
    pub rot_axis: DVec3,  // Normalized
}

/// Data structure for atom_rot node.
/// Rotates an atomic structure around an axis in world space by a specified angle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomRotData {
    pub angle: f64,  // Rotation angle in radians
    #[serde(with = "dvec3_serializer")]
    pub rot_axis: DVec3,  // Rotation axis direction (will be normalized)
    #[serde(with = "dvec3_serializer")]
    pub pivot_point: DVec3,  // Point around which rotation occurs (in angstroms)
}

impl NodeData for AtomRotData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let atom_rot_cache = eval_cache.downcast_ref::<AtomRotEvalCache>()?;

        Some(Box::new(AtomRotGadget::new(
            self.angle,
            atom_rot_cache.rot_axis,
            atom_rot_cache.pivot_point,
        )))
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &Vec<NetworkStackElement<'a>>,
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext
    ) -> NetworkResult {
        // 1. Get input atomic structure
        let input_val = network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = input_val {
            return input_val;
        }

        if let NetworkResult::Atomic(atomic_structure) = input_val {
            // 2. Get angle (from pin or property)
            let angle = match network_evaluator.evaluate_or_default(
                network_stack, node_id, registry, context, 1,
                self.angle,
                NetworkResult::extract_float
            ) {
                Ok(value) => value,
                Err(error) => return error,
            };

            // 3. Get rotation axis (from pin or property)
            let rot_axis = match network_evaluator.evaluate_or_default(
                network_stack, node_id, registry, context, 2,
                self.rot_axis,
                NetworkResult::extract_vec3
            ) {
                Ok(value) => value,
                Err(error) => return error,
            };

            // 4. Get pivot point (from pin or property)
            let pivot_point = match network_evaluator.evaluate_or_default(
                network_stack, node_id, registry, context, 3,
                self.pivot_point,
                NetworkResult::extract_vec3
            ) {
                Ok(value) => value,
                Err(error) => return error,
            };

            // 5. Normalize the rotation axis
            let normalized_axis = rot_axis.normalize_or_zero();
            if normalized_axis == DVec3::ZERO {
                // Invalid axis - return input unchanged
                return NetworkResult::Atomic(atomic_structure);
            }

            // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
            if network_stack.len() == 1 {
                let eval_cache = AtomRotEvalCache {
                    pivot_point,
                    rot_axis: normalized_axis,
                };
                context.selected_node_eval_cache = Some(Box::new(eval_cache));
            }

            // 6. Create rotation quaternion
            let rotation_quat = DQuat::from_axis_angle(normalized_axis, angle);

            // 7. Apply rotation around pivot point directly to atoms (NO frame transform manipulation)
            // This is: translate to origin, rotate, translate back
            let mut result = atomic_structure.clone();

            // For each atom: new_pos = pivot + rotation * (old_pos - pivot)
            // Which is equivalent to: translate by -pivot, rotate, translate by +pivot
            result.transform(&DQuat::IDENTITY, &(-pivot_point));  // Move pivot to origin
            result.transform(&rotation_quat, &DVec3::ZERO);       // Rotate around origin
            result.transform(&DQuat::IDENTITY, &pivot_point);     // Move back

            return NetworkResult::Atomic(result);
        }

        NetworkResult::None
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("angle".to_string(), TextValue::Float(self.angle)),
            ("rot_axis".to_string(), TextValue::Vec3(self.rot_axis)),
            ("pivot_point".to_string(), TextValue::Vec3(self.pivot_point)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("angle") {
            self.angle = v.as_float().ok_or_else(|| "angle must be a float".to_string())?;
        }
        if let Some(v) = props.get("rot_axis") {
            self.rot_axis = v.as_vec3().ok_or_else(|| "rot_axis must be a Vec3".to_string())?;
        }
        if let Some(v) = props.get("pivot_point") {
            self.pivot_point = v.as_vec3().ok_or_else(|| "pivot_point must be a Vec3".to_string())?;
        }
        Ok(())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        let show_angle = !connected_input_pins.contains("angle");
        let show_axis = !connected_input_pins.contains("rot_axis");

        if self.angle == 0.0 {
            return None;  // No rotation
        }

        let mut parts = Vec::new();

        if show_angle {
            let degrees = self.angle.to_degrees();
            parts.push(format!("{:.1}°", degrees));
        }

        if show_axis {
            // Show simplified axis name if it's a standard axis
            let axis_name = match (self.rot_axis.x, self.rot_axis.y, self.rot_axis.z) {
                (x, y, z) if (x - 1.0).abs() < 0.001 && y.abs() < 0.001 && z.abs() < 0.001 => "X",
                (x, y, z) if x.abs() < 0.001 && (y - 1.0).abs() < 0.001 && z.abs() < 0.001 => "Y",
                (x, y, z) if x.abs() < 0.001 && y.abs() < 0.001 && (z - 1.0).abs() < 0.001 => "Z",
                (x, y, z) if (x + 1.0).abs() < 0.001 && y.abs() < 0.001 && z.abs() < 0.001 => "-X",
                (x, y, z) if x.abs() < 0.001 && (y + 1.0).abs() < 0.001 && z.abs() < 0.001 => "-Y",
                (x, y, z) if x.abs() < 0.001 && y.abs() < 0.001 && (z + 1.0).abs() < 0.001 => "-Z",
                _ => return Some(format!("{:.1}° custom", self.angle.to_degrees())),
            };
            parts.push(axis_name.to_string());
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m
    }
}

/// Gadget for atom_rot node that displays the rotation axis as an arrow.
/// The gadget shows a yellow cylinder with an arrow head at the positive end,
/// and a red sphere at the pivot point.
#[derive(Clone)]
pub struct AtomRotGadget {
    pub angle: f64,
    pub rot_axis: DVec3,  // Normalized
    pub pivot_point: DVec3,
    pub dragging: bool,
    pub drag_start_angle: f64,
    pub drag_start_offset: f64,  // Offset along axis at drag start
}

const AXIS_LENGTH: f64 = 15.0;  // Length of the rotation axis visualization in angstroms
const CYLINDER_RADIUS: f64 = 0.2;  // Match AXIS_CYLINDER_RADIUS in xyz_gadget_utils
const HIT_RADIUS: f64 = 1.5;  // Hit detection radius (larger for easier clicking)
const ROTATION_SENSITIVITY: f64 = 0.3;  // Radians per unit offset delta

impl Tessellatable for AtomRotGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;

        // Calculate axis endpoints
        let half_length = AXIS_LENGTH * 0.5;
        let top_center = self.pivot_point + self.rot_axis * half_length;
        let bottom_center = self.pivot_point - self.rot_axis * half_length;

        let yellow_material = Material::new(
            &glam::f32::Vec3::new(1.0, 1.0, 0.0),
            0.4,
            0.8
        );

        // Cylinder for the axis
        tessellate_cylinder(
            output_mesh,
            &top_center,
            &bottom_center,
            CYLINDER_RADIUS,
            16,
            &yellow_material,
            true,
            Some(&yellow_material),
            Some(&yellow_material),
        );

        // Arrow head at top
        let arrow_tip = top_center + self.rot_axis * 0.5;
        tessellate_cone(
            output_mesh,
            &arrow_tip,
            &top_center,
            CYLINDER_RADIUS * 3.0,
            16,
            &yellow_material,
            true,  // include base
        );

        // Red sphere at pivot point
        let red_material = Material::new(
            &glam::f32::Vec3::new(1.0, 0.0, 0.0),
            0.4,
            0.0
        );

        tessellate_sphere(
            output_mesh,
            &self.pivot_point,
            0.4,
            12,
            12,
            &red_material,
        );

        // === Perpendicular flag that rotates with self.angle ===
        // This provides visual feedback during dragging

        // Create a reference direction perpendicular to the rotation axis
        let base_perp_dir = if self.rot_axis.dot(DVec3::X).abs() < 0.9 {
            self.rot_axis.cross(DVec3::X).normalize()
        } else {
            self.rot_axis.cross(DVec3::Y).normalize()
        };

        // Rotate the perpendicular direction by the current angle
        let rotation_quat = DQuat::from_axis_angle(self.rot_axis, self.angle);
        let rotated_perp_dir = rotation_quat * base_perp_dir;

        // Flag parameters
        let flag_length = 3.0;  // Length of the flag
        let flag_offset_along_axis = 2.0;  // Offset from pivot along the axis

        // Position the flag slightly above the pivot point
        let flag_base = self.pivot_point + self.rot_axis * flag_offset_along_axis;
        let flag_end = flag_base + rotated_perp_dir * flag_length;

        // Use a cyan/teal color for the flag to distinguish from the yellow axis
        let flag_material = Material::new(
            &glam::f32::Vec3::new(0.0, 1.0, 1.0),  // Cyan
            0.4,
            0.8
        );

        // Draw the flag cylinder
        tessellate_cylinder(
            output_mesh,
            &flag_base,
            &flag_end,
            CYLINDER_RADIUS * 1.5,  // Slightly thicker than axis
            12,
            &flag_material,
            true,
            Some(&flag_material),
            Some(&flag_material),
        );

        // Small sphere at the flag tip for better visibility
        tessellate_sphere(
            output_mesh,
            &flag_end,
            0.3,
            8,
            8,
            &flag_material,
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for AtomRotGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        if cylinder_ray_intersection(
            ray_origin, ray_direction,
            self.pivot_point, self.rot_axis,
            AXIS_LENGTH, HIT_RADIUS
        ) {
            return Some(0);
        }
        None
    }

    fn start_drag(&mut self, _handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        self.dragging = true;
        self.drag_start_angle = self.angle;
        self.drag_start_offset = self.get_axis_offset(ray_origin, ray_direction);
    }

    fn drag(&mut self, _handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        let current_offset = self.get_axis_offset(ray_origin, ray_direction);
        let offset_delta = current_offset - self.drag_start_offset;
        self.angle = self.drag_start_angle + offset_delta * ROTATION_SENSITIVITY;
    }

    fn end_drag(&mut self) {
        self.dragging = false;
    }
}

impl NodeNetworkGadget for AtomRotGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(atom_rot_data) = data.as_any_mut().downcast_mut::<AtomRotData>() {
            atom_rot_data.angle = self.angle;
        }
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl AtomRotGadget {
    pub fn new(angle: f64, rot_axis: DVec3, pivot_point: DVec3) -> Self {
        Self {
            angle,
            rot_axis,
            pivot_point,
            dragging: false,
            drag_start_angle: 0.0,
            drag_start_offset: 0.0,
        }
    }

    /// Get the current drag offset along the rotation axis
    fn get_axis_offset(&self, ray_origin: DVec3, ray_direction: DVec3) -> f64 {
        get_closest_point_on_first_ray(
            &self.pivot_point,
            &self.rot_axis,
            &ray_origin,
            &ray_direction
        )
    }
}

/// Test if a ray intersects with a cylinder along an axis
fn cylinder_ray_intersection(
    ray_origin: DVec3,
    ray_direction: DVec3,
    cylinder_center: DVec3,
    cylinder_axis: DVec3,
    cylinder_length: f64,
    hit_radius: f64,
) -> bool {
    // Cylinder segment endpoints
    let half_length = cylinder_length * 0.5;
    let top = cylinder_center + cylinder_axis * half_length;
    let bottom = cylinder_center - cylinder_axis * half_length;

    // Find closest points between cylinder axis line and ray line
    // Line 1 (cylinder): P1 + s * d1 where P1 = bottom, d1 = (top - bottom)
    // Line 2 (ray): P2 + t * d2 where P2 = ray_origin, d2 = ray_direction
    let d1 = top - bottom;
    let d2 = ray_direction;
    let w = bottom - ray_origin;  // P1 - P2 (correct sign for formula)

    let a = d1.dot(d1);  // |d1|²
    let b = d1.dot(d2);  // d1 · d2
    let c = d2.dot(d2);  // |d2|²
    let d = d1.dot(w);   // d1 · w
    let e = d2.dot(w);   // d2 · w

    let denom = a * c - b * b;
    if denom.abs() < 1e-10 {
        return false;  // Lines are parallel
    }

    // Closest point parameters
    let s = (b * e - c * d) / denom;  // Parameter along cylinder axis [0,1]
    let t = (a * e - b * d) / denom;  // Parameter along ray [0,∞)

    // Check if the closest point is within the cylinder bounds
    if s < 0.0 || s > 1.0 {
        return false;
    }

    // Check if the intersection is in front of the camera (t >= 0)
    if t < 0.0 {
        return false;
    }

    // Calculate the distance between the closest points
    let point_on_axis = bottom + d1 * s;
    let point_on_ray = ray_origin + ray_direction * t;
    let distance = (point_on_axis - point_on_ray).length();

    distance <= hit_radius
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "atom_rot".to_string(),
        description: "Rotates an atomic structure around an axis in world space.
The rotation is performed around the specified axis direction, centered at the pivot point.
The axis is always interpreted in the fixed Cartesian coordinate system (world space).
The rotation angle is in radians in text format (e.g., 1.5708 for 90°) and degrees in the UI.".to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::Atomic,
            },
            Parameter {
                id: None,
                name: "angle".to_string(),
                data_type: DataType::Float,
            },
            Parameter {
                id: None,
                name: "rot_axis".to_string(),
                data_type: DataType::Vec3,
            },
            Parameter {
                id: None,
                name: "pivot_point".to_string(),
                data_type: DataType::Vec3,
            },
        ],
        output_type: DataType::Atomic,
        public: true,
        node_data_creator: || Box::new(AtomRotData {
            angle: 0.0,
            rot_axis: DVec3::new(0.0, 0.0, 1.0),  // Default: Z-axis
            pivot_point: DVec3::ZERO,
        }),
        node_data_saver: generic_node_data_saver::<AtomRotData>,
        node_data_loader: generic_node_data_loader::<AtomRotData>,
    }
}
