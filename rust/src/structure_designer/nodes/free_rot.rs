use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure_diff::extract_diff;
use crate::display::gadget::{Gadget, GadgetPickContext};
use crate::geo_tree::GeoNode;
use crate::renderer::mesh::{Material, Mesh};
use crate::renderer::tessellator::tessellator::{
    Tessellatable, TessellationOutput, tessellate_cone, tessellate_cylinder, tessellate_sphere,
};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, MoleculeData, NetworkResult, runtime_type_error_in_input,
    worsen_alignment_with_reason,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::hit_test_utils::get_closest_point_on_first_ray;
use crate::util::serialization_utils::dvec3_serializer;
use crate::util::transform::Transform;
use glam::f64::DQuat;
use glam::f64::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct FreeRotEvalCache {
    pub pivot_point: DVec3,
    pub rot_axis: DVec3,
}

/// Wraps an extracted (or empty) diff as the `Molecule` value for the node's
/// `diff` output pin (issue #295, `doc/design_diff_outputs_for_atom_ops.md` §2).
/// A `Blueprint` input has no atoms, so it yields an empty diff (§2.3).
fn diff_pin(mut diff: AtomicStructure) -> NetworkResult {
    diff.decorator_mut().show_anchor_arrows = true;
    NetworkResult::Molecule(MoleculeData {
        atoms: diff,
        geo_tree_root: None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeRotData {
    /// Rotation angle in **degrees** (converted to radians at the eval math
    /// boundary). Renamed from the radian-era `angle` so stale snippets fail
    /// loudly rather than being silently reinterpreted (issue #384).
    pub angle_degrees: f64,
    #[serde(with = "dvec3_serializer")]
    pub rot_axis: DVec3,
    #[serde(with = "dvec3_serializer")]
    pub pivot_point: DVec3,
}

impl NodeData for FreeRotData {
    fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let cache = eval_cache.downcast_ref::<FreeRotEvalCache>()?;

        Some(Box::new(FreeRotGadget::new(
            self.angle_degrees.to_radians(),
            cache.rot_axis,
            cache.pivot_point,
        )))
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext,
    ) -> EvalOutput {
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = input_val {
            // Propagate the error on both pins (result + diff) so diff consumers
            // don't silently see `None` on pin 1 (§2).
            return EvalOutput::multi(vec![input_val.clone(), input_val]);
        }

        // The value flowing through pin 1 is now in degrees (issue #384);
        // convert to radians at the single math boundary below.
        let angle_degrees = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.angle_degrees,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::multi(vec![error.clone(), error]),
        };

        let rot_axis = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.rot_axis,
            NetworkResult::extract_vec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::multi(vec![error.clone(), error]),
        };

        let pivot_point = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            self.pivot_point,
            NetworkResult::extract_vec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::multi(vec![error.clone(), error]),
        };

        let normalized_axis = rot_axis.normalize_or_zero();
        if normalized_axis == DVec3::ZERO {
            // Degenerate axis: the mutation is skipped, but we must still yield
            // two pins — the input unchanged on pin 0 and an empty diff on pin 1
            // (§3 Phase 3, never `EvalOutput::single`).
            return match input_val {
                NetworkResult::Blueprint(_) | NetworkResult::Molecule(_) => {
                    EvalOutput::multi(vec![input_val, diff_pin(AtomicStructure::new_diff())])
                }
                _ => {
                    let err = runtime_type_error_in_input(0);
                    EvalOutput::multi(vec![err.clone(), err])
                }
            };
        }

        if network_stack.len() == 1 {
            context.selected_node_eval_cache = Some(Box::new(FreeRotEvalCache {
                pivot_point,
                rot_axis: normalized_axis,
            }));
        }

        let rotation_quat = DQuat::from_axis_angle(normalized_axis, angle_degrees.to_radians());
        let tr = Transform::new_rotation_around_point(pivot_point, rotation_quat);

        match input_val {
            NetworkResult::Blueprint(shape) => {
                let mut alignment = shape.alignment;
                let mut alignment_reason = shape.alignment_reason;
                worsen_alignment_with_reason(
                    &mut alignment,
                    &mut alignment_reason,
                    Alignment::LatticeUnaligned,
                    || {
                        format!(
                            "free_rot rotates the cutter by {:.2}° in world space (off-lattice)",
                            angle_degrees
                        )
                    },
                );
                EvalOutput::multi(vec![
                    NetworkResult::Blueprint(BlueprintData {
                        structure: shape.structure,
                        geo_tree_root: GeoNode::transform(tr, Box::new(shape.geo_tree_root)),
                        alignment,
                        alignment_reason,
                    }),
                    // Blueprint has no atoms → empty diff (§2.3).
                    diff_pin(AtomicStructure::new_diff()),
                ])
            }
            NetworkResult::Molecule(mol) => {
                let mut atoms = mol.atoms;
                // Snapshot before the in-place transform; atom ids are stable, so
                // the diff is an exact id-keyed comparison (§1.5).
                let before = atoms.clone();
                atoms.transform(&DQuat::IDENTITY, &(-pivot_point));
                atoms.transform(&rotation_quat, &DVec3::ZERO);
                atoms.transform(&DQuat::IDENTITY, &pivot_point);
                let diff = extract_diff(&before, &atoms, 0.0);
                let new_geo = mol
                    .geo_tree_root
                    .map(|gt| GeoNode::transform(tr, Box::new(gt)));
                EvalOutput::multi(vec![
                    NetworkResult::Molecule(MoleculeData {
                        atoms,
                        geo_tree_root: new_geo,
                    }),
                    diff_pin(diff),
                ])
            }
            _ => {
                let err = runtime_type_error_in_input(0);
                EvalOutput::multi(vec![err.clone(), err])
            }
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "angle_degrees".to_string(),
                TextValue::Float(self.angle_degrees),
            ),
            ("rot_axis".to_string(), TextValue::Vec3(self.rot_axis)),
            ("pivot_point".to_string(), TextValue::Vec3(self.pivot_point)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        // Reject the radian-era key explicitly so stale snippets fail loudly
        // instead of no-oping invisibly (issue #384). The wiring path uses the
        // pin name `angle`, which is unchanged — this only affects a literal
        // `angle: <number>` property.
        if props.contains_key("angle") {
            return Err("angle was renamed to angle_degrees and is now in degrees".to_string());
        }
        if let Some(v) = props.get("angle_degrees") {
            self.angle_degrees = v
                .as_float()
                .ok_or_else(|| "angle_degrees must be a float".to_string())?;
        }
        if let Some(v) = props.get("rot_axis") {
            self.rot_axis = v
                .as_vec3()
                .ok_or_else(|| "rot_axis must be a Vec3".to_string())?;
        }
        if let Some(v) = props.get("pivot_point") {
            self.pivot_point = v
                .as_vec3()
                .ok_or_else(|| "pivot_point must be a Vec3".to_string())?;
        }
        Ok(())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        let show_angle = !connected_input_pins.contains("angle");
        let show_axis = !connected_input_pins.contains("rot_axis");

        if self.angle_degrees == 0.0 {
            return None;
        }

        let mut parts = Vec::new();

        if show_angle {
            parts.push(format!("{:.1}°", self.angle_degrees));
        }

        if show_axis {
            let axis_name = match (self.rot_axis.x, self.rot_axis.y, self.rot_axis.z) {
                (x, y, z) if (x - 1.0).abs() < 0.001 && y.abs() < 0.001 && z.abs() < 0.001 => "X",
                (x, y, z) if x.abs() < 0.001 && (y - 1.0).abs() < 0.001 && z.abs() < 0.001 => "Y",
                (x, y, z) if x.abs() < 0.001 && y.abs() < 0.001 && (z - 1.0).abs() < 0.001 => "Z",
                (x, y, z) if (x + 1.0).abs() < 0.001 && y.abs() < 0.001 && z.abs() < 0.001 => "-X",
                (x, y, z) if x.abs() < 0.001 && (y + 1.0).abs() < 0.001 && z.abs() < 0.001 => "-Y",
                (x, y, z) if x.abs() < 0.001 && y.abs() < 0.001 && (z + 1.0).abs() < 0.001 => "-Z",
                _ => return Some(format!("{:.1}° custom", self.angle_degrees)),
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
        m.insert("input".to_string(), (true, None));
        // The pin name (`angle`) diverges from the property name
        // (`angle_degrees`) after issue #384, so the text-format introspection
        // fallback would wrongly mark this optional pin as required. Precedent:
        // `extrude`'s `dir` ↔ `extrude_direction` split.
        m.insert(
            "angle".to_string(),
            (
                false,
                Some("0 (degrees; stored as angle_degrees)".to_string()),
            ),
        );
        m
    }
}

#[derive(Clone)]
pub struct FreeRotGadget {
    pub angle: f64,
    pub rot_axis: DVec3,
    pub pivot_point: DVec3,
    pub dragging: bool,
    pub drag_start_angle: f64,
    pub drag_start_offset: f64,
}

const AXIS_LENGTH: f64 = 15.0;
const CYLINDER_RADIUS: f64 = 0.2;
const HIT_RADIUS: f64 = 1.5;
const ROTATION_SENSITIVITY: f64 = 0.3;

impl Tessellatable for FreeRotGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;

        let half_length = AXIS_LENGTH * 0.5;
        let top_center = self.pivot_point + self.rot_axis * half_length;
        let bottom_center = self.pivot_point - self.rot_axis * half_length;

        let yellow_material = Material::new(&glam::f32::Vec3::new(1.0, 1.0, 0.0), 0.4, 0.8);

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

        let arrow_tip = top_center + self.rot_axis * 0.5;
        tessellate_cone(
            output_mesh,
            &arrow_tip,
            &top_center,
            CYLINDER_RADIUS * 3.0,
            16,
            &yellow_material,
            true,
        );

        let red_material = Material::new(&glam::f32::Vec3::new(1.0, 0.0, 0.0), 0.4, 0.0);

        tessellate_sphere(output_mesh, &self.pivot_point, 0.4, 12, 12, &red_material);

        let base_perp_dir = if self.rot_axis.dot(DVec3::X).abs() < 0.9 {
            self.rot_axis.cross(DVec3::X).normalize()
        } else {
            self.rot_axis.cross(DVec3::Y).normalize()
        };

        let rotation_quat = DQuat::from_axis_angle(self.rot_axis, self.angle);
        let rotated_perp_dir = rotation_quat * base_perp_dir;

        let flag_length = 3.0;
        let flag_offset_along_axis = 2.0;

        let flag_base = self.pivot_point + self.rot_axis * flag_offset_along_axis;
        let flag_end = flag_base + rotated_perp_dir * flag_length;

        let flag_material = Material::new(&glam::f32::Vec3::new(0.0, 1.0, 1.0), 0.4, 0.8);

        tessellate_cylinder(
            output_mesh,
            &flag_base,
            &flag_end,
            CYLINDER_RADIUS * 1.5,
            12,
            &flag_material,
            true,
            Some(&flag_material),
            Some(&flag_material),
        );

        tessellate_sphere(output_mesh, &flag_end, 0.3, 8, 8, &flag_material);
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for FreeRotGadget {
    fn hit_test(
        &self,
        ray_origin: DVec3,
        ray_direction: DVec3,
        _pick_ctx: &GadgetPickContext,
    ) -> Option<i32> {
        if cylinder_ray_intersection(
            ray_origin,
            ray_direction,
            self.pivot_point,
            self.rot_axis,
            AXIS_LENGTH,
            HIT_RADIUS,
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

impl NodeNetworkGadget for FreeRotGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(d) = data.as_any_mut().downcast_mut::<FreeRotData>() {
            // The gadget's `angle` is radians (its rotation/tessellation math is
            // radian-native); convert to degrees at this data boundary (#384).
            d.angle_degrees = self.angle.to_degrees();
        }
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl FreeRotGadget {
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

    fn get_axis_offset(&self, ray_origin: DVec3, ray_direction: DVec3) -> f64 {
        get_closest_point_on_first_ray(
            &self.pivot_point,
            &self.rot_axis,
            &ray_origin,
            &ray_direction,
        )
    }
}

fn cylinder_ray_intersection(
    ray_origin: DVec3,
    ray_direction: DVec3,
    cylinder_center: DVec3,
    cylinder_axis: DVec3,
    cylinder_length: f64,
    hit_radius: f64,
) -> bool {
    let half_length = cylinder_length * 0.5;
    let top = cylinder_center + cylinder_axis * half_length;
    let bottom = cylinder_center - cylinder_axis * half_length;

    let d1 = top - bottom;
    let d2 = ray_direction;
    let w = bottom - ray_origin;

    let a = d1.dot(d1);
    let b = d1.dot(d2);
    let c = d2.dot(d2);
    let d = d1.dot(w);
    let e = d2.dot(w);

    let denom = a * c - b * b;
    if denom.abs() < 1e-10 {
        return false;
    }

    let s = (b * e - c * d) / denom;
    let t = (a * e - b * d) / denom;

    if !(0.0..=1.0).contains(&s) {
        return false;
    }

    if t < 0.0 {
        return false;
    }

    let point_on_axis = bottom + d1 * s;
    let point_on_ray = ray_origin + ray_direction * t;
    let distance = (point_on_axis - point_on_ray).length();

    distance <= hit_radius
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "free_rot".to_string(),
        description: "Rotates an unanchored object (Blueprint or Molecule) around an axis in world space.
The angle is in degrees.
For a Blueprint, only the geometry (the cutter) rotates; the structure stays fixed. This can drift the cutter off-lattice.
For a Molecule, atoms and geometry rotate together.
Crystal inputs are rejected (exit_structure first, or use structure_rot to stay in lattice space).
The `diff` output pin captures the atom motion only (a Molecule diff applicable via apply_diff); geometry motion is not represented in the diff. A Blueprint input yields an empty diff.".to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "input".to_string(),
                data_type: DataType::HasFreeLinOps,
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
        output_pins: vec![
            OutputPinDefinition::same_as_input("result", "input"),
            OutputPinDefinition::fixed("diff", DataType::Molecule),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(FreeRotData {
                angle_degrees: 0.0,
                rot_axis: DVec3::new(0.0, 0.0, 1.0),
                pivot_point: DVec3::ZERO,
            })
        },
        node_data_saver: generic_node_data_saver::<FreeRotData>,
        node_data_loader: generic_node_data_loader::<FreeRotData>,
    }
}
