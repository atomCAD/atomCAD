use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::api::structure_designer::structure_designer_preferences::BackgroundPreferences;
use crate::crystolecule::drawing_plane::DrawingPlane;
use crate::crystolecule::structure::Structure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::display::gadget::Gadget;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::utils::half_space_utils;
use crate::structure_designer::utils::half_space_utils::get_dragged_shift;
use crate::util::serialization_utils::{ivec3_serializer, option_ivec3_serializer};
use glam::f64::DVec3;
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingPlaneData {
    pub max_miller_index: i32,
    /// Miller plane index `(h k l)`, now optional. Old files carry a bare
    /// `[h,k,l]` array which `option_ivec3_serializer` deserializes to `Some(..)`;
    /// an absent field (`#[serde(default)]`) or explicit `null` (case D) yields
    /// `None`.
    #[serde(with = "option_ivec3_serializer", default)]
    pub miller_index: Option<IVec3>,
    #[serde(with = "ivec3_serializer")]
    pub center: IVec3,
    pub shift: i32,
    #[serde(default = "default_subdivision")]
    pub subdivision: i32,
    /// First in-plane lattice direction `[u v w]`. `None` = unset.
    #[serde(with = "option_ivec3_serializer", default)]
    pub u_axis: Option<IVec3>,
    /// Second in-plane lattice direction `[u v w]`. `None` = unset.
    #[serde(with = "option_ivec3_serializer", default)]
    pub v_axis: Option<IVec3>,
}

fn default_subdivision() -> i32 {
    1
}

impl NodeData for DrawingPlaneData {
    fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let drawing_plane_cache = eval_cache.downcast_ref::<DrawingPlaneEvalCache>()?;

        // Drive the gadget from the *resolved* orientation so it reflects the
        // concrete plane (derived `m` in case D, auto-picked second axis in
        // case B). Miller-index dragging is disabled when the stored `m` is
        // `None` (case D) — the index is derived from `u`/`v`, not editable.
        Some(Box::new(DrawingPlaneGadget::new(
            self.max_miller_index,
            &drawing_plane_cache.resolved_miller,
            self.center,
            self.shift,
            self.subdivision,
            &drawing_plane_cache.unit_cell,
            &structure_designer.preferences.background_preferences,
            self.miller_index.is_some(),
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
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let structure = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            Structure::diamond(),
            NetworkResult::extract_structure,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };
        let unit_cell = structure.lattice_vecs.clone();

        // Resolve the three orientation inputs to `Option<IVec3>` with the
        // standard three-state rule (wired pin > stored field > unset).
        let miller_index = match resolve_optional_ivec3(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.miller_index,
            "m_index",
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let u = match resolve_optional_ivec3(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            5,
            self.u_axis,
            "u",
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let v = match resolve_optional_ivec3(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            6,
            self.v_axis,
            "v",
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let center = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.center,
            NetworkResult::extract_ivec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let shift = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            self.shift,
            NetworkResult::extract_int,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let subdivision = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            4,
            self.subdivision,
            NetworkResult::extract_int,
        ) {
            Ok(value) => value.max(1), // Ensure minimum value of 1
            Err(error) => return EvalOutput::single(error),
        };

        // Create DrawingPlane via the explicit-orientation spec (handles cases A–D).
        let drawing_plane = match DrawingPlane::from_spec(
            unit_cell,
            miller_index,
            u,
            v,
            center,
            shift,
            subdivision,
        ) {
            Ok(plane) => plane,
            Err(error_msg) => return EvalOutput::single(NetworkResult::Error(error_msg)),
        };

        // Store evaluation cache for root-level evaluations (used for gadget creation
        // when this node is selected). Only store for direct evaluations of visible
        // nodes, not for upstream dependency calculations. The cache carries the
        // *resolved* orientation so the gadget/editor reflect the effective plane —
        // important for case D (derived `m`) and case B (auto-picked second axis).
        if network_stack.len() == 1 {
            let eval_cache = DrawingPlaneEvalCache {
                unit_cell: drawing_plane.unit_cell.clone(),
                resolved_miller: drawing_plane.miller_index,
                resolved_u: drawing_plane.u_axis,
                resolved_v: drawing_plane.v_axis,
            };
            context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }

        EvalOutput::single(NetworkResult::DrawingPlane(drawing_plane))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let center_connected = connected_input_pins.contains("center");
        let m_index_connected = connected_input_pins.contains("m_index");
        let shift_connected = connected_input_pins.contains("shift");
        let subdivision_connected = connected_input_pins.contains("subdivision");
        let u_connected = connected_input_pins.contains("u");
        let v_connected = connected_input_pins.contains("v");

        let mut parts = Vec::new();

        if !center_connected {
            parts.push(format!(
                "c: ({},{},{})",
                self.center.x, self.center.y, self.center.z
            ));
        }

        if !m_index_connected {
            // Show the Miller index when set, or a `derived` marker when `None`
            // (case D — the index is computed from `u`/`v`).
            match self.miller_index {
                Some(m) => parts.push(format!("m: ({},{},{})", m.x, m.y, m.z)),
                None => parts.push("m: derived".to_string()),
            }
        }

        if !u_connected && let Some(u) = self.u_axis {
            parts.push(format!("u: [{},{},{}]", u.x, u.y, u.z));
        }

        if !v_connected && let Some(v) = self.v_axis {
            parts.push(format!("v: [{},{},{}]", v.x, v.y, v.z));
        }

        if !shift_connected {
            parts.push(format!("s: {}", self.shift));
        }

        if !subdivision_connected && self.subdivision != 1 {
            parts.push(format!("sub: {}", self.subdivision));
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        // `m_index`/`u`/`v` are optional: present ⇒ `Some`, absent ⇒ `None`.
        // Emitting them only when set lets the text format express the
        // unset/derived state (case D `m: null`) by simple omission.
        let mut props = vec![(
            "max_miller_index".to_string(),
            TextValue::Int(self.max_miller_index),
        )];
        if let Some(m) = self.miller_index {
            props.push(("m_index".to_string(), TextValue::IVec3(m)));
        }
        props.push(("center".to_string(), TextValue::IVec3(self.center)));
        props.push(("shift".to_string(), TextValue::Int(self.shift)));
        props.push(("subdivision".to_string(), TextValue::Int(self.subdivision)));
        if let Some(u) = self.u_axis {
            props.push(("u".to_string(), TextValue::IVec3(u)));
        }
        if let Some(v) = self.v_axis {
            props.push(("v".to_string(), TextValue::IVec3(v)));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("max_miller_index") {
            self.max_miller_index = v
                .as_int()
                .ok_or_else(|| "max_miller_index must be an integer".to_string())?;
        }
        // Optional orientation triple: present ⇒ `Some`, absent ⇒ `None`. Replace-mode
        // text edits rebuild the node, so an omitted property naturally unsets the field.
        self.miller_index = read_optional_ivec3(props, "m_index")?;
        self.u_axis = read_optional_ivec3(props, "u")?;
        self.v_axis = read_optional_ivec3(props, "v")?;
        if let Some(v) = props.get("center") {
            self.center = v
                .as_ivec3()
                .ok_or_else(|| "center must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("shift") {
            self.shift = v
                .as_int()
                .ok_or_else(|| "shift must be an integer".to_string())?;
        }
        if let Some(v) = props.get("subdivision") {
            self.subdivision = v
                .as_int()
                .ok_or_else(|| "subdivision must be an integer".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert(
            "structure".to_string(),
            (false, Some("diamond".to_string())),
        );
        // The three orientation inputs are all optional (resolved via the
        // wired-pin > stored-field > unset rule).
        m.insert("m_index".to_string(), (false, Some("(0,0,1)".to_string())));
        m.insert("u".to_string(), (false, None));
        m.insert("v".to_string(), (false, None));
        m
    }
}

#[derive(Debug, Clone)]
pub struct DrawingPlaneEvalCache {
    pub unit_cell: UnitCellStruct,
    /// Resolved Miller index (derived from `u`/`v` in case D).
    pub resolved_miller: IVec3,
    /// Resolved first in-plane axis (auto-picked in case A).
    pub resolved_u: IVec3,
    /// Resolved second in-plane axis (auto-picked in cases A/B).
    pub resolved_v: IVec3,
}

#[derive(Clone)]
pub struct DrawingPlaneGadget {
    pub max_miller_index: i32,
    pub miller_index: IVec3,
    pub center: IVec3,
    pub dragged_shift: f64, // this is rounded into 'shift'
    pub shift: i32,
    pub subdivision: i32,
    pub dragged_handle_index: Option<i32>,
    pub possible_miller_indices: HashSet<IVec3>,
    pub unit_cell: UnitCellStruct,
    pub background_preferences: BackgroundPreferences,
    /// Whether the Miller index can be edited by dragging the central handle.
    /// `false` in case D, where `m` is derived from `u`/`v` (not directly editable).
    pub miller_editable: bool,
}

impl Tessellatable for DrawingPlaneGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;
        let center_pos = self.unit_cell.ivec3_lattice_to_real(&self.center);

        half_space_utils::tessellate_center_sphere(output_mesh, &center_pos);

        half_space_utils::tessellate_shift_drag_handle(
            output_mesh,
            &self.center,
            &self.miller_index,
            self.dragged_shift,
            &self.unit_cell,
            self.subdivision,
        );

        // Tessellate miller index discs only if we're dragging the central sphere
        // (handle index 0) and the Miller index is editable (not derived).
        if self.dragged_handle_index == Some(0) && self.miller_editable {
            half_space_utils::tessellate_miller_indices_discs(
                output_mesh,
                &center_pos,
                &self.miller_index,
                &self.possible_miller_indices,
                self.max_miller_index,
                &self.unit_cell,
            );
        }
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for DrawingPlaneGadget {
    // Returns the index of the handle that was hit, or None if no handle was hit
    // handle 0: miller index handle (central red sphere)
    // handle 1: shift drag handle (blue cylinder)
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        // Test central sphere
        if let Some(_t) = half_space_utils::hit_test_center_sphere(
            &self.unit_cell,
            &self.center,
            &ray_origin,
            &ray_direction,
        ) {
            return Some(0); // Central sphere hit
        }

        // Test shift handle cylinder
        if let Some(_t) = half_space_utils::hit_test_shift_handle(
            &self.unit_cell,
            &self.center,
            &self.miller_index,
            self.shift as f64,
            &ray_origin,
            &ray_direction,
            self.subdivision,
        ) {
            return Some(1); // Shift handle hit
        }

        None // No handle was hit
    }

    fn start_drag(&mut self, handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {
        self.dragged_handle_index = Some(handle_index);
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        // Calculate center position in world space
        let center_pos = self.unit_cell.ivec3_lattice_to_real(&self.center);

        if handle_index == 0 {
            // Handle index already stored in dragged_handle_index during start_drag

            // Miller index is derived (case D) — not draggable.
            if !self.miller_editable {
                return;
            }

            // Check if any miller index disc is hit
            if let Some(new_miller_index) = half_space_utils::hit_test_miller_indices_discs(
                &self.unit_cell,
                &center_pos,
                &self.possible_miller_indices,
                self.max_miller_index,
                ray_origin,
                ray_direction,
            ) {
                // Set the miller index to the hit disc's miller index
                self.miller_index = new_miller_index;
            }
        } else if handle_index == 1 {
            // Handle dragging the shift handle
            // We need to determine the new shift value based on where the mouse ray is closest to the normal ray
            self.dragged_shift = get_dragged_shift(
                &self.unit_cell,
                &self.miller_index,
                &self.center,
                &ray_origin,
                &ray_direction,
                half_space_utils::SHIFT_HANDLE_ACCESSIBILITY_OFFSET,
                self.subdivision,
            );
            self.shift = self.dragged_shift.round() as i32;
        }
    }

    fn end_drag(&mut self) {
        // Clear the dragged handle index to stop displaying the grid and conditional miller index discs
        self.dragged_handle_index = None;
    }
}

impl NodeNetworkGadget for DrawingPlaneGadget {
    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }

    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(drawing_plane_data) = data.as_any_mut().downcast_mut::<DrawingPlaneData>() {
            // Only write the Miller index back when it is directly editable
            // (case D derives it from `u`/`v`, leaving the stored field `None`).
            if self.miller_editable {
                drawing_plane_data.miller_index = Some(self.miller_index);
            }
            drawing_plane_data.center = self.center;
            drawing_plane_data.shift = self.shift;
        }
    }
}

impl DrawingPlaneGadget {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_miller_index: i32,
        miller_index: &IVec3,
        center: IVec3,
        shift: i32,
        subdivision: i32,
        unit_cell: &UnitCellStruct,
        background_preferences: &BackgroundPreferences,
        miller_editable: bool,
    ) -> Self {
        Self {
            max_miller_index,
            miller_index: *miller_index,
            center,
            dragged_shift: shift as f64,
            shift,
            subdivision,
            dragged_handle_index: None,
            possible_miller_indices: half_space_utils::generate_possible_miller_indices(
                max_miller_index,
            ),
            unit_cell: unit_cell.clone(),
            background_preferences: background_preferences.clone(),
            miller_editable,
        }
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "drawing_plane".to_string(),
      description: "Defines a 2D drawing plane on a crystallographic plane with Miller indices. Use this to specify where 2D shapes are placed before extrusion.".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
          id: None,
          name: "structure".to_string(),
          data_type: DataType::Structure,
        },
        Parameter {
          id: None,
          name: "m_index".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          id: None,
          name: "center".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          id: None,
          name: "shift".to_string(),
          data_type: DataType::Int,
        },
        Parameter {
          id: None,
          name: "subdivision".to_string(),
          data_type: DataType::Int,
        },
        // New optional in-plane direction pins (appended at indices 5/6 so all
        // existing pin indices are preserved).
        Parameter {
          id: None,
          name: "u".to_string(),
          data_type: DataType::IVec3,
        },
        Parameter {
          id: None,
          name: "v".to_string(),
          data_type: DataType::IVec3,
        },
      ],
      output_pins: OutputPinDefinition::single(DataType::DrawingPlane),
      zone_input_pins: vec![],
      zone_output_pins: vec![],
      public: true,
      node_data_creator: || Box::new(DrawingPlaneData {
        max_miller_index: 1,
        miller_index: Some(IVec3::new(0, 0, 1)), // Default normal along z-axis (001 plane)
        center: IVec3::new(0, 0, 0),
        shift: 0,
        subdivision: 1,
        u_axis: None,
        v_axis: None,
      }),
      node_data_saver: generic_node_data_saver::<DrawingPlaneData>,
      node_data_loader: generic_node_data_loader::<DrawingPlaneData>,
    }
}

/// Resolves an orientation input pin to `Option<IVec3>` using the three-state
/// rule: wired pin → `Some(value)`; pin disconnected → the stored field;
/// a wired pin that evaluates to an error propagates that error.
#[allow(clippy::result_large_err)]
#[allow(clippy::too_many_arguments)]
fn resolve_optional_ivec3(
    evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'_>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    parameter_index: usize,
    stored: Option<IVec3>,
    pin_name: &str,
) -> Result<Option<IVec3>, NetworkResult> {
    let result = evaluator.evaluate_arg(network_stack, node_id, registry, context, parameter_index);
    match result {
        NetworkResult::None => Ok(stored),
        r if r.is_error() => Err(r),
        r => r
            .extract_ivec3()
            .map(Some)
            .ok_or_else(|| NetworkResult::Error(format!("{} must be an IVec3", pin_name))),
    }
}

/// Reads an optional `IVec3` text property: present ⇒ `Some`, absent ⇒ `None`.
/// A present-but-wrong-typed value is a hard error.
fn read_optional_ivec3(
    props: &HashMap<String, TextValue>,
    key: &str,
) -> Result<Option<IVec3>, String> {
    match props.get(key) {
        Some(v) => Ok(Some(
            v.as_ivec3()
                .ok_or_else(|| format!("{} must be an IVec3", key))?,
        )),
        None => Ok(None),
    }
}
