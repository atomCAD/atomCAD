use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::unit_cell_symmetries::{RotationalSymmetry, analyze_unit_cell_symmetries};
use crate::display::gadget::Gadget;
use crate::geo_tree::GeoNode;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator,
};
use crate::structure_designer::evaluator::network_result::{
    BlueprintData, CrystalData, NetworkResult, runtime_type_error_in_input,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::serialization_utils::ivec3_serializer;
use crate::util::transform::Transform;
use glam::DQuat;
use glam::f64::DVec3;
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StructureRotEvalCache {
    pub unit_cell: UnitCellStruct,
    pub pivot_point: IVec3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureRotData {
    pub axis_index: Option<i32>,
    pub step: i32,
    #[serde(with = "ivec3_serializer")]
    pub pivot_point: IVec3,
}

impl NodeData for StructureRotData {
    fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let cache = eval_cache.downcast_ref::<StructureRotEvalCache>()?;

        let gadget = StructureRotGadget::new(
            self.axis_index,
            self.step,
            cache.pivot_point,
            &cache.unit_cell,
        );
        Some(Box::new(gadget))
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
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = input_val {
            return EvalOutput::single(input_val);
        }

        let axis_index = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.axis_index,
            NetworkResult::extract_optional_int,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let step = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.step,
            NetworkResult::extract_int,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let pivot_point = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            self.pivot_point,
            NetworkResult::extract_ivec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let unit_cell = match &input_val {
            NetworkResult::Blueprint(bp) => bp.structure.lattice_vecs.clone(),
            NetworkResult::Crystal(c) => c.structure.lattice_vecs.clone(),
            _ => return EvalOutput::single(runtime_type_error_in_input(0)),
        };

        let symmetry_axes = analyze_unit_cell_symmetries(&unit_cell);
        let real_rotation_quat = compute_rotation_quat(axis_index, step, &symmetry_axes);
        let pivot_real = unit_cell.ivec3_lattice_to_real(&pivot_point);
        let tr = Transform::new_rotation_around_point(pivot_real, real_rotation_quat);

        if network_stack.len() == 1 {
            context.selected_node_eval_cache = Some(Box::new(StructureRotEvalCache {
                unit_cell: unit_cell.clone(),
                pivot_point,
            }));
        }

        match input_val {
            NetworkResult::Blueprint(shape) => {
                EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
                    structure: shape.structure.clone(),
                    geo_tree_root: GeoNode::transform(tr, Box::new(shape.geo_tree_root)),
                }))
            }
            NetworkResult::Crystal(crystal) => {
                let mut atoms = crystal.atoms;
                let neg_pivot = DVec3::new(-pivot_real.x, -pivot_real.y, -pivot_real.z);
                atoms.transform(&DQuat::IDENTITY, &neg_pivot);
                atoms.transform(&real_rotation_quat, &DVec3::ZERO);
                atoms.transform(&DQuat::IDENTITY, &pivot_real);

                let new_geo_tree_root = crystal
                    .geo_tree_root
                    .map(|gt| GeoNode::transform(tr, Box::new(gt)));

                EvalOutput::single(NetworkResult::Crystal(CrystalData {
                    structure: crystal.structure,
                    atoms,
                    geo_tree_root: new_geo_tree_root,
                }))
            }
            _ => EvalOutput::single(runtime_type_error_in_input(0)),
        }
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let show_axis_index = !connected_input_pins.contains("axis_index");
        let show_step = !connected_input_pins.contains("step");

        let mut parts = Vec::new();
        let has_rotation = self.axis_index.is_some() && self.step != 0;

        if has_rotation && show_axis_index {
            if let Some(axis_idx) = self.axis_index {
                parts.push(format!("axis: {}", axis_idx));
            }
        }

        if has_rotation && show_step {
            parts.push(format!("step: {}", self.step));
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = vec![
            ("step".to_string(), TextValue::Int(self.step)),
            (
                "pivot_point".to_string(),
                TextValue::IVec3(self.pivot_point),
            ),
        ];
        if let Some(axis_idx) = self.axis_index {
            props.insert(0, ("axis_index".to_string(), TextValue::Int(axis_idx)));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("axis_index") {
            self.axis_index = Some(
                v.as_int()
                    .ok_or_else(|| "axis_index must be an integer".to_string())?,
            );
        }
        if let Some(v) = props.get("step") {
            self.step = v
                .as_int()
                .ok_or_else(|| "step must be an integer".to_string())?;
        }
        if let Some(v) = props.get("pivot_point") {
            self.pivot_point = v
                .as_ivec3()
                .ok_or_else(|| "pivot_point must be an IVec3".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("input".to_string(), (true, None));
        m
    }
}

#[derive(Clone)]
pub struct StructureRotGadget {
    pub axis_index: Option<i32>,
    pub step: i32,
    pub pivot_point: IVec3,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for StructureRotGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;
        if let Some(axis_idx) = self.axis_index {
            let symmetry_axes = analyze_unit_cell_symmetries(&self.unit_cell);

            if !symmetry_axes.is_empty() {
                let safe_axis_index = ((axis_idx % symmetry_axes.len() as i32)
                    + symmetry_axes.len() as i32)
                    % symmetry_axes.len() as i32;
                let selected_symmetry = &symmetry_axes[safe_axis_index as usize];

                let normalized_axis = selected_symmetry.axis;
                let cylinder_length = 30.0;
                let cylinder_radius = 0.1;

                let pivot_real = self.unit_cell.ivec3_lattice_to_real(&self.pivot_point);

                let half_length = cylinder_length * 0.5;
                let top_center = pivot_real + normalized_axis * half_length;
                let bottom_center = pivot_real - normalized_axis * half_length;

                let yellow_material = crate::renderer::mesh::Material::new(
                    &glam::f32::Vec3::new(1.0, 1.0, 0.0),
                    0.4,
                    0.8,
                );

                crate::renderer::tessellator::tessellator::tessellate_cylinder(
                    output_mesh,
                    &top_center,
                    &bottom_center,
                    cylinder_radius,
                    16,
                    &yellow_material,
                    true,
                    Some(&yellow_material),
                    Some(&yellow_material),
                );

                let red_material = crate::renderer::mesh::Material::new(
                    &glam::f32::Vec3::new(1.0, 0.0, 0.0),
                    0.4,
                    0.0,
                );

                crate::renderer::tessellator::tessellator::tessellate_sphere(
                    output_mesh,
                    &pivot_real,
                    0.4,
                    12,
                    12,
                    &red_material,
                );
            }
        }
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for StructureRotGadget {
    fn hit_test(&self, _ray_origin: DVec3, _ray_direction: DVec3) -> Option<i32> {
        None
    }

    fn start_drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {}

    fn drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {}

    fn end_drag(&mut self) {}
}

impl NodeNetworkGadget for StructureRotGadget {
    fn sync_data(&self, _data: &mut dyn NodeData) {}

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl StructureRotGadget {
    pub fn new(
        axis_index: Option<i32>,
        step: i32,
        pivot_point: IVec3,
        unit_cell: &UnitCellStruct,
    ) -> Self {
        Self {
            axis_index,
            step,
            pivot_point,
            unit_cell: unit_cell.clone(),
        }
    }
}

fn compute_rotation_quat(
    axis_index: Option<i32>,
    step: i32,
    symmetry_axes: &[RotationalSymmetry],
) -> DQuat {
    if axis_index.is_none() || step == 0 || symmetry_axes.is_empty() {
        DQuat::IDENTITY
    } else {
        let axis_idx = axis_index.unwrap();

        let safe_axis_index = ((axis_idx % symmetry_axes.len() as i32)
            + symmetry_axes.len() as i32)
            % symmetry_axes.len() as i32;
        let selected_symmetry = &symmetry_axes[safe_axis_index as usize];

        let safe_step = ((step % selected_symmetry.n_fold as i32)
            + selected_symmetry.n_fold as i32)
            % selected_symmetry.n_fold as i32;

        if safe_step == 0 {
            DQuat::IDENTITY
        } else {
            let angle_per_step = selected_symmetry.smallest_angle_radians();
            let total_angle = angle_per_step * safe_step as f64;
            DQuat::from_axis_angle(selected_symmetry.axis, total_angle)
        }
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "structure_rot".to_string(),
        description: "Rotates a structure-bound object (Blueprint or Crystal) in lattice space.
Only rotations that are symmetries of the input's structure are allowed.
For a Blueprint, only the geometry (the cutter) rotates.
For a Crystal, atoms and geometry rotate together.
Molecule inputs are rejected (use free_rot for free-space rotation)."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry3D,
        parameters: vec![
            Parameter {
                id: None,
                name: "input".to_string(),
                data_type: DataType::StructureBound,
            },
            Parameter {
                id: None,
                name: "axis_index".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "step".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "pivot_point".to_string(),
                data_type: DataType::IVec3,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("input"),
        public: true,
        node_data_creator: || {
            Box::new(StructureRotData {
                axis_index: None,
                step: 0,
                pivot_point: IVec3::new(0, 0, 0),
            })
        },
        node_data_saver: generic_node_data_saver::<StructureRotData>,
        node_data_loader: generic_node_data_loader::<StructureRotData>,
    }
}
