use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure_diff::extract_diff;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::display::gadget::{Gadget, GadgetPickContext};
use crate::geo_tree::GeoNode;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator,
};
use crate::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, MoleculeData, NetworkResult,
    runtime_type_error_in_input, worsen_alignment_with_reason,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::utils::xyz_gadget_utils;
use crate::util::mat_utils::unit_ivec3;
use crate::util::serialization_utils::{ivec3_or_int_serializer, ivec3_serializer};
use crate::util::transform::Transform;
use glam::DQuat;
use glam::f64::DVec3;
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StructureMoveEvalCache {
    pub unit_cell: UnitCellStruct,
    /// The per-axis subdivision actually used by `eval` — the wired
    /// `subdiv_xyz` pin's value when connected, else the wired `subdivision`
    /// pin splatted, else the stored field. The gadget must read this rather
    /// than `StructureMoveData::lattice_subdivision`, or a wired subdivision
    /// makes the gizmo travel `lattice_subdivision`× further than the object
    /// it moves (issue #411).
    pub lattice_subdivision: IVec3,
}

/// Renders a subdivision for user-facing strings: the scalar form (`"2"`) when
/// uniform, the vector form (`"(2, 4, 1)"`) otherwise.
fn format_subdivision(sub: &IVec3) -> String {
    if sub.x == sub.y && sub.y == sub.z {
        format!("{}", sub.x)
    } else {
        format!("({}, {}, {})", sub.x, sub.y, sub.z)
    }
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
pub struct StructureMoveData {
    #[serde(with = "ivec3_serializer")]
    pub translation: IVec3,
    /// Per-axis subdivision. Old files stored a single uniform i32; the
    /// serializer accepts both forms (a scalar splats across all three axes).
    #[serde(
        default = "default_lattice_subdivision",
        with = "ivec3_or_int_serializer"
    )]
    pub lattice_subdivision: IVec3,
}

fn default_lattice_subdivision() -> IVec3 {
    IVec3::ONE
}

impl NodeData for StructureMoveData {
    fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let cache = eval_cache.downcast_ref::<StructureMoveEvalCache>()?;
        let gadget = StructureMoveGadget::new(
            self.translation,
            cache.lattice_subdivision,
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
            // Propagate the error on both pins (result + diff) so diff consumers
            // don't silently see `None` on pin 1 (§2).
            return EvalOutput::multi(vec![input_val.clone(), input_val]);
        }

        let translation = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.translation,
            NetworkResult::extract_ivec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::multi(vec![error.clone(), error]),
        };

        // Per-axis `subdiv_xyz` (pin 3) wins over the uniform `subdivision`
        // (pin 2, splatted); both fall back to the stored per-axis field.
        let subdiv_xyz_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 3);
        if subdiv_xyz_val.is_error() {
            return EvalOutput::multi(vec![subdiv_xyz_val.clone(), subdiv_xyz_val]);
        }
        let lattice_subdivision = match subdiv_xyz_val.extract_ivec3() {
            Some(value) => value,
            None => match network_evaluator.evaluate_or_default(
                network_stack,
                node_id,
                registry,
                context,
                2,
                self.lattice_subdivision,
                |result| result.extract_int().map(IVec3::splat),
            ) {
                Ok(value) => value,
                Err(error) => return EvalOutput::multi(vec![error.clone(), error]),
            },
        }
        .max(IVec3::ONE);

        let subdivided_translation = translation.as_dvec3() / lattice_subdivision.as_dvec3();

        // Translation is lattice-safe iff each component is divisible by its
        // axis's subdivision.
        let divisible = translation.x.rem_euclid(lattice_subdivision.x) == 0
            && translation.y.rem_euclid(lattice_subdivision.y) == 0
            && translation.z.rem_euclid(lattice_subdivision.z) == 0;

        match input_val {
            NetworkResult::Blueprint(shape) => {
                let unit_cell = shape.structure.lattice_vecs.clone();
                let real_translation = unit_cell.dvec3_lattice_to_real(&subdivided_translation);

                if network_stack.len() == 1 {
                    context.selected_node_eval_cache = Some(Box::new(StructureMoveEvalCache {
                        unit_cell: unit_cell.clone(),
                        lattice_subdivision,
                    }));
                }

                let mut alignment = shape.alignment;
                let mut alignment_reason = shape.alignment_reason;
                if !divisible {
                    worsen_alignment_with_reason(
                        &mut alignment,
                        &mut alignment_reason,
                        Alignment::LatticeUnaligned,
                        || {
                            format!(
                                "structure_move by fractional translation ({}, {}, {})/{}",
                                translation.x,
                                translation.y,
                                translation.z,
                                format_subdivision(&lattice_subdivision)
                            )
                        },
                    );
                }

                EvalOutput::multi(vec![
                    NetworkResult::Blueprint(BlueprintData {
                        structure: shape.structure.clone(),
                        geo_tree_root: GeoNode::transform(
                            Transform::new(real_translation, DQuat::IDENTITY),
                            Box::new(shape.geo_tree_root),
                        ),
                        alignment,
                        alignment_reason,
                    }),
                    // Blueprint has no atoms → empty diff (§2.3).
                    diff_pin(AtomicStructure::new_diff()),
                ])
            }
            NetworkResult::Crystal(crystal) => {
                let unit_cell = crystal.structure.lattice_vecs.clone();
                let real_translation = unit_cell.dvec3_lattice_to_real(&subdivided_translation);

                if network_stack.len() == 1 {
                    context.selected_node_eval_cache = Some(Box::new(StructureMoveEvalCache {
                        unit_cell: unit_cell.clone(),
                        lattice_subdivision,
                    }));
                }

                let mut atoms = crystal.atoms;
                // Snapshot before the in-place transform; atom ids are stable, so
                // the diff is an exact id-keyed comparison (§1.5).
                let before = atoms.clone();
                atoms.transform(&DQuat::IDENTITY, &real_translation);
                let diff = extract_diff(&before, &atoms, 0.0);

                let new_geo_tree_root = crystal.geo_tree_root.map(|gt| {
                    GeoNode::transform(
                        Transform::new(real_translation, DQuat::IDENTITY),
                        Box::new(gt),
                    )
                });

                let mut alignment = crystal.alignment;
                let mut alignment_reason = crystal.alignment_reason;
                if !divisible {
                    worsen_alignment_with_reason(
                        &mut alignment,
                        &mut alignment_reason,
                        Alignment::LatticeUnaligned,
                        || {
                            format!(
                                "structure_move by fractional translation ({}, {}, {})/{}",
                                translation.x,
                                translation.y,
                                translation.z,
                                format_subdivision(&lattice_subdivision)
                            )
                        },
                    );
                }

                EvalOutput::multi(vec![
                    NetworkResult::Crystal(CrystalData {
                        structure: crystal.structure,
                        atoms,
                        geo_tree_root: new_geo_tree_root,
                        alignment,
                        alignment_reason,
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

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let show_translation = !connected_input_pins.contains("translation");
        let show_subdivision = !connected_input_pins.contains("subdivision")
            && !connected_input_pins.contains("subdiv_xyz")
            && self.lattice_subdivision != IVec3::ONE;

        match (show_translation, show_subdivision) {
            (true, true) => Some(format!(
                "t: ({},{},{}), sub: {}",
                self.translation.x,
                self.translation.y,
                self.translation.z,
                format_subdivision(&self.lattice_subdivision)
            )),
            (true, false) => Some(format!(
                "t: ({},{},{})",
                self.translation.x, self.translation.y, self.translation.z
            )),
            (false, true) => Some(format!(
                "sub: {}",
                format_subdivision(&self.lattice_subdivision)
            )),
            (false, false) => None,
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "translation".to_string(),
                TextValue::IVec3(self.translation),
            ),
            (
                "subdivision".to_string(),
                if self.lattice_subdivision.x == self.lattice_subdivision.y
                    && self.lattice_subdivision.y == self.lattice_subdivision.z
                {
                    TextValue::Int(self.lattice_subdivision.x)
                } else {
                    TextValue::IVec3(self.lattice_subdivision)
                },
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("translation") {
            self.translation = v
                .as_ivec3()
                .ok_or_else(|| "translation must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("subdivision") {
            self.lattice_subdivision = if let Some(i) = v.as_int() {
                IVec3::splat(i)
            } else if let Some(vec) = v.as_ivec3() {
                vec
            } else {
                return Err("subdivision must be an integer or an IVec3".to_string());
            };
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("input".to_string(), (true, None));
        // No matching text property (the stored `subdivision` property backs
        // pin 2), so without this entry the introspection would mark the pin
        // required.
        m.insert(
            "subdiv_xyz".to_string(),
            (
                false,
                Some("overrides `subdivision` per axis when connected".to_string()),
            ),
        );
        m
    }
}

#[derive(Clone)]
pub struct StructureMoveGadget {
    pub translation: IVec3,
    pub lattice_subdivision: IVec3,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
    pub start_drag_translation: IVec3,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for StructureMoveGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;
        xyz_gadget_utils::tessellate_xyz_gadget(
            output_mesh,
            &self.unit_cell,
            DQuat::IDENTITY,
            &self.get_real_position(),
            false,
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for StructureMoveGadget {
    fn hit_test(
        &self,
        ray_origin: DVec3,
        ray_direction: DVec3,
        pick_ctx: &GadgetPickContext,
    ) -> Option<i32> {
        xyz_gadget_utils::xyz_gadget_hit_test(
            &self.unit_cell,
            DQuat::IDENTITY,
            &self.get_real_position(),
            &ray_origin,
            &ray_direction,
            false,
            pick_ctx,
        )
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        self.dragged_handle_index = Some(handle_index);
        self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
            &self.unit_cell,
            DQuat::IDENTITY,
            &self.get_real_position(),
            handle_index,
            &ray_origin,
            &ray_direction,
        );
        self.start_drag_translation = self.translation;
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
            &self.unit_cell,
            DQuat::IDENTITY,
            &self.get_real_position(),
            handle_index,
            &ray_origin,
            &ray_direction,
        );
        let offset_delta = current_offset - self.start_drag_offset;
        if self.apply_drag_offset(handle_index, offset_delta) {
            self.start_drag(handle_index, ray_origin, ray_direction);
        }
    }

    fn end_drag(&mut self) {
        self.dragged_handle_index = None;
    }
}

impl NodeNetworkGadget for StructureMoveGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(d) = data.as_any_mut().downcast_mut::<StructureMoveData>() {
            d.translation = self.translation;
        }
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl StructureMoveGadget {
    pub fn new(translation: IVec3, lattice_subdivision: IVec3, unit_cell: &UnitCellStruct) -> Self {
        Self {
            translation,
            lattice_subdivision,
            dragged_handle_index: None,
            start_drag_offset: 0.0,
            start_drag_translation: translation,
            unit_cell: unit_cell.clone(),
        }
    }

    fn get_real_position(&self) -> DVec3 {
        let subdivided_pos = self.translation.as_dvec3() / self.lattice_subdivision.as_dvec3();
        self.unit_cell.dvec3_lattice_to_real(&subdivided_pos)
    }

    fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
        let axis_basis_vector = self.unit_cell.get_basis_vector(axis_index);
        let axis_subdivision = self.lattice_subdivision[axis_index as usize] as f64;
        let rounded_delta = (offset_delta / axis_basis_vector.length() * axis_subdivision).round();

        if rounded_delta == 0.0 {
            return false;
        }

        self.translation =
            self.start_drag_translation + unit_ivec3(axis_index) * (rounded_delta as i32);

        true
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "structure_move".to_string(),
        description:
            "Moves a structure-bound object (Blueprint or Crystal) in discrete lattice space.
For a Blueprint, only the geometry (the cutter) moves; latent atoms stay anchored to the structure.
For a Crystal, atoms and geometry move together rigidly within the structure.
Molecule inputs are rejected (use free_move for free-space translation).
The translation is measured in units of 1/subdivision of a lattice cell. The `subdivision` pin applies one subdivision to all three axes; the `subdiv_xyz` pin sets it per axis and overrides `subdivision` when connected (both override the stored value).
The `diff` output pin captures the atom motion only (a Molecule diff applicable via apply_diff); geometry/structure motion is not represented in the diff. A Blueprint input yields an empty diff."
                .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry3D,
        parameters: vec![
            Parameter {
                id: None,
                name: "input".to_string(),
                data_type: DataType::HasStructure,
            },
            Parameter {
                id: None,
                name: "translation".to_string(),
                data_type: DataType::IVec3,
            },
            Parameter {
                id: None,
                name: "subdivision".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "subdiv_xyz".to_string(),
                data_type: DataType::IVec3,
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
            Box::new(StructureMoveData {
                translation: IVec3::new(0, 0, 0),
                lattice_subdivision: IVec3::ONE,
            })
        },
        node_data_saver: generic_node_data_saver::<StructureMoveData>,
        node_data_loader: generic_node_data_loader::<StructureMoveData>,
    }
}
