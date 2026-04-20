use std::any::Any;
use std::collections::{HashMap, HashSet};

use crate::api::structure_designer::structure_designer_preferences::GeometryVisualization;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualizationPreferences;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::display::csg_to_poly_mesh::convert_csg_mesh_to_poly_mesh;
use crate::display::csg_to_poly_mesh::convert_csg_sketch_to_poly_mesh;
use crate::geo_tree::GeoNode;
use crate::geo_tree::csg_cache::CsgConversionCache;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::implicit_eval::surface_splatting_2d::generate_2d_point_cloud;
use crate::structure_designer::implicit_eval::surface_splatting_3d::generate_point_cloud;
use crate::structure_designer::node_data::EvalOutput;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::node_network::NodeDisplayType;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::facet_shell::FacetShellData;
use crate::structure_designer::structure_designer_scene::{
    DisplayedPinOutput, NodeOutput, NodeSceneData,
};

use super::network_result::Closure;
use super::network_result::input_missing_error;
use super::network_result::{Alignment, BlueprintData, GeometrySummary2D};
use crate::crystolecule::structure::Structure;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;

#[derive(Clone)]
pub struct NetworkStackElement<'a> {
    pub node_network: &'a NodeNetwork,
    pub node_id: u64,
}

impl<'a> NetworkStackElement<'a> {
    pub fn get_top_node(network_stack: &[NetworkStackElement<'a>], node_id: u64) -> &'a Node {
        network_stack
            .last()
            .unwrap()
            .node_network
            .nodes
            .get(&node_id)
            .unwrap()
    }

    pub fn is_node_selected_in_root_network(
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
    ) -> bool {
        network_stack
            .first()
            .unwrap()
            .node_network
            .is_node_selected(node_id)
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct NodeInvocationId {
    root_network_name: String,
    node_id_stack: Vec<u64>,
}

pub struct NetworkEvaluationContext {
    pub node_errors: HashMap<u64, String>,
    pub node_output_strings: HashMap<u64, Vec<String>>,
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
    pub top_level_parameters: HashMap<String, NetworkResult>,
    /// Whether to use spatial grid cutoff for vdW interactions during minimization.
    pub use_vdw_cutoff: bool,
}

impl Default for NetworkEvaluationContext {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkEvaluationContext {
    pub fn new() -> Self {
        Self {
            node_errors: HashMap::new(),
            node_output_strings: HashMap::new(),
            selected_node_eval_cache: None,
            top_level_parameters: HashMap::new(),
            use_vdw_cutoff: false,
        }
    }
}

pub struct NetworkEvaluator {
    csg_conversion_cache: CsgConversionCache,
}

/*
 * Node network evaluator.
 * The node network evaluator is able to generate displayable representation for a node in a node network.
 * It delegates node related evaluation to functions in node specific modules.
 */
impl Default for NetworkEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkEvaluator {
    pub fn new() -> Self {
        Self {
            csg_conversion_cache: CsgConversionCache::with_defaults(),
        }
    }

    /// Clear the CSG conversion cache
    pub fn clear_csg_cache(&mut self) {
        self.csg_conversion_cache.clear();
    }

    /// Get cache statistics
    pub fn get_csg_cache_stats(&self) -> crate::geo_tree::csg_cache::CacheStats {
        self.csg_conversion_cache.stats()
    }

    // Creates the Scene that will be displayed for the given node by the Renderer, and is retained
    // for interaction purposes
    #[allow(clippy::too_many_arguments)]
    pub fn generate_scene(
        &mut self,
        network_name: &str,
        node_id: u64,
        _display_type: NodeDisplayType, //TODO: use display_type
        registry: &NodeTypeRegistry,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
        top_level_parameters: Option<HashMap<String, NetworkResult>>,
        use_vdw_cutoff: bool,
    ) -> NodeSceneData {
        //let _timer = Timer::new("generate_scene");

        let network = match registry.node_networks.get(network_name) {
            Some(network) => network,
            None => return NodeSceneData::new(NodeOutput::None),
        };

        // Do not evaluate invalid networks
        if !network.valid {
            return NodeSceneData::new(NodeOutput::None);
        }

        let mut context = NetworkEvaluationContext::new();
        context.use_vdw_cutoff = use_vdw_cutoff;
        if let Some(params) = top_level_parameters {
            context.top_level_parameters = params;
        }

        // We assign the root node network zero node id. It is not used in the evaluation.
        let network_stack = vec![NetworkStackElement {
            node_network: network,
            node_id: 0,
        }];

        let node = match network.nodes.get(&node_id) {
            Some(node) => node,
            None => return NodeSceneData::new(NodeOutput::None),
        };

        let from_selected_node = network_stack
            .last()
            .unwrap()
            .node_network
            .is_node_selected(node_id);

        // Evaluate all outputs once (avoids redundant evaluation for multi-output nodes)
        let eval_output = {
            //let _timer = Timer::new("evaluate inside generate_scene");
            self.evaluate_all_outputs(
                &network_stack,
                node_id,
                registry,
                from_selected_node,
                &mut context,
            )
        };

        // Get the unit cell: prefer explicit override (e.g. motif_edit), else extract from primary
        let unit_cell = eval_output
            .unit_cell_override
            .clone()
            .or_else(|| eval_output.primary().get_unit_cell());

        // Convert primary result (pin 0) to NodeOutput for backward compat.
        // Use display override if present (e.g., motif_edit shows Atomic in viewport
        // while the wire carries Motif).
        let node_type = registry.get_node_type_for_node(node).unwrap();
        let (display_result_0, display_type_0) =
            if let Some(dr) = eval_output.display_results.get(&0) {
                let dt = dr
                    .infer_data_type()
                    .unwrap_or_else(|| node_type.output_type().clone());
                (dr.clone(), dt)
            } else {
                // Infer from the concrete result first so that pins declared
                // as `SameAsInput(..)` (for which `output_type()` falls
                // through to `DataType::None`) still map to the right
                // NodeOutput variant. Fall back to the declared type when the
                // result is None/Error/etc.
                let result = eval_output.get(0);
                let dt = result
                    .infer_data_type()
                    .unwrap_or_else(|| node_type.output_type().clone());
                (result, dt)
            };
        let (output, geo_tree) = self.convert_result_to_node_output(
            display_result_0,
            &display_type_0,
            from_selected_node,
            &network_stack,
            node_id,
            registry,
            &mut context,
            geometry_visualization_preferences,
        );

        // Build pin_outputs for ALL output pins (not just displayed ones).
        // This makes NodeSceneData cache-safe: pin display can be toggled
        // without re-evaluation. displayed_outputs() filters at render time.
        let pin_count = node_type.output_pin_count();
        let mut pin_outputs = Vec::with_capacity(pin_count);
        for pin_index_usize in 0..pin_count {
            let pin_index = pin_index_usize as i32;
            if pin_index == 0 {
                // Pin 0's actual data lives in NodeSceneData.output / .geo_tree.
                // displayed_outputs() resolves pin 0 from those fields.
                pin_outputs.push(DisplayedPinOutput {
                    pin_index: 0,
                    output: NodeOutput::None,
                    geo_tree: None,
                });
                continue;
            }
            let (pin_result, pin_data_type) =
                if let Some(dr) = eval_output.display_results.get(&pin_index_usize) {
                    let dt = dr
                        .infer_data_type()
                        .unwrap_or_else(|| node_type.get_output_pin_type(pin_index));
                    (dr.clone(), dt)
                } else {
                    let result = eval_output.get(pin_index);
                    let dt = result
                        .infer_data_type()
                        .unwrap_or_else(|| node_type.get_output_pin_type(pin_index));
                    (result, dt)
                };
            let (pin_output, pin_geo_tree) = self.convert_result_to_node_output(
                pin_result,
                &pin_data_type,
                from_selected_node,
                &network_stack,
                node_id,
                registry,
                &mut context,
                geometry_visualization_preferences,
            );
            pin_outputs.push(DisplayedPinOutput {
                pin_index,
                output: pin_output,
                geo_tree: pin_geo_tree,
            });
        }

        // Get current displayed pins from the network
        let displayed_pins = network
            .get_displayed_pins(node_id)
            .cloned()
            .unwrap_or_else(|| HashSet::from([0]));

        // Show unit cell wireframe when eval explicitly provided a unit cell
        // (motif_edit sets unit_cell_override; other nodes don't)
        let show_unit_cell_wireframe = eval_output.unit_cell_override.is_some();

        // Build NodeSceneData
        NodeSceneData {
            output,
            geo_tree,
            pin_outputs,
            displayed_pins,
            node_errors: context.node_errors.clone(),
            node_output_strings: context.node_output_strings.clone(),
            unit_cell,
            show_unit_cell_wireframe,
            selected_node_eval_cache: context.selected_node_eval_cache,
        }
    }

    /// Converts a geometry shell (`GeoNode`) retained on a Crystal/Molecule
    /// into a renderable `NodeOutput` using the user's current visualization
    /// method and sharpness settings. Returns `None` if conversion yields no
    /// mesh (e.g. empty CSG result).
    fn build_atomic_shell_output(
        &mut self,
        geo_tree: &GeoNode,
        context: &mut NetworkEvaluationContext,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
    ) -> Option<NodeOutput> {
        match geometry_visualization_preferences.geometry_visualization {
            GeometryVisualization::SurfaceSplatting => {
                let point_cloud =
                    generate_point_cloud(geo_tree, context, geometry_visualization_preferences);
                Some(NodeOutput::SurfacePointCloud(point_cloud))
            }
            GeometryVisualization::ExplicitMesh => {
                let csg_mesh = geo_tree.to_csg_mesh_cached(Some(&mut self.csg_conversion_cache))?;
                let mut poly_mesh = convert_csg_mesh_to_poly_mesh(&csg_mesh, false, false);
                poly_mesh.detect_sharp_edges(
                    geometry_visualization_preferences.sharpness_angle_threshold_degree,
                    true,
                );
                Some(NodeOutput::PolyMesh(poly_mesh))
            }
        }
    }

    fn generate_explicit_mesh_output<'a>(
        &mut self,
        result: NetworkResult,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        _registry: &NodeTypeRegistry,
        _context: &mut NetworkEvaluationContext,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
    ) -> (NodeOutput, Option<GeoNode>) {
        //let _timer = Timer::new("generate_explicit_mesh_output");
        let from_selected_node = network_stack
            .last()
            .unwrap()
            .node_network
            .is_node_selected(node_id);

        let poly_mesh = match &result {
            NetworkResult::Blueprint(geometry_summary) => {
                if let Some(csg_mesh) = geometry_summary
                    .geo_tree_root
                    .to_csg_mesh_cached(Some(&mut self.csg_conversion_cache))
                {
                    let node = network_stack
                        .last()
                        .unwrap()
                        .node_network
                        .nodes
                        .get(&node_id)
                        .unwrap();
                    let is_half_space = node.node_type_name == "half_space";
                    let mut poly_mesh =
                        convert_csg_mesh_to_poly_mesh(&csg_mesh, is_half_space, is_half_space);
                    poly_mesh.detect_sharp_edges(
                        geometry_visualization_preferences.sharpness_angle_threshold_degree,
                        true,
                    );
                    // Highlight faces if the last node is facet_shell and it's selected
                    if node.node_type_name == "facet_shell" && from_selected_node {
                        // Downcast the node data to FacetShellData
                        if let Some(facet_shell_data) =
                            node.data.as_any_ref().downcast_ref::<FacetShellData>()
                        {
                            // Call the highlight method
                            facet_shell_data.highlight_selected_facets(&mut poly_mesh);
                        }
                    }
                    Some(poly_mesh)
                } else {
                    None
                }
            }
            NetworkResult::Geometry2D(geometry_summary_2d) => {
                if let Some(csg_sketch) = geometry_summary_2d
                    .geo_tree_root
                    .to_csg_sketch_cached(Some(&mut self.csg_conversion_cache))
                {
                    let mut poly_mesh = convert_csg_sketch_to_poly_mesh(
                        csg_sketch,
                        !geometry_visualization_preferences.wireframe_geometry,
                        &geometry_summary_2d.drawing_plane,
                    );
                    poly_mesh.detect_sharp_edges(
                        geometry_visualization_preferences.sharpness_angle_threshold_degree,
                        true,
                    );
                    Some(poly_mesh)
                } else {
                    None
                }
            }
            _ => None,
        };

        // Extract geo_tree_root from the result based on its type
        let geo_tree = match result {
            NetworkResult::Blueprint(geometry_summary) => Some(geometry_summary.geo_tree_root),
            NetworkResult::Geometry2D(geometry_summary_2d) => {
                Some(geometry_summary_2d.geo_tree_root)
            }
            _ => None,
        };

        // Return output and geo_tree
        let output = if let Some(mesh) = poly_mesh {
            NodeOutput::PolyMesh(mesh)
        } else {
            NodeOutput::None
        };

        (output, geo_tree)
    }

    /// Converts a NetworkResult to a NodeOutput based on the data type and visualization preferences.
    #[allow(clippy::too_many_arguments)]
    fn convert_result_to_node_output<'a>(
        &mut self,
        result: NetworkResult,
        data_type: &DataType,
        from_selected_node: bool,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
    ) -> (NodeOutput, Option<GeoNode>) {
        if *data_type == DataType::DrawingPlane {
            if let NetworkResult::DrawingPlane(drawing_plane) = result {
                (NodeOutput::DrawingPlane(drawing_plane), None)
            } else {
                (NodeOutput::None, None)
            }
        } else if *data_type == DataType::Geometry2D {
            if geometry_visualization_preferences.geometry_visualization
                == GeometryVisualization::SurfaceSplatting
            {
                if let NetworkResult::Geometry2D(geometry_summary_2d) = result {
                    let point_cloud = generate_2d_point_cloud(
                        &geometry_summary_2d.geo_tree_root,
                        context,
                        geometry_visualization_preferences,
                    );
                    (
                        NodeOutput::SurfacePointCloud2D(point_cloud),
                        Some(geometry_summary_2d.geo_tree_root),
                    )
                } else {
                    (NodeOutput::None, None)
                }
            } else if geometry_visualization_preferences.geometry_visualization
                == GeometryVisualization::ExplicitMesh
            {
                self.generate_explicit_mesh_output(
                    result,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            } else {
                (NodeOutput::None, None)
            }
        } else if *data_type == DataType::Blueprint {
            if geometry_visualization_preferences.geometry_visualization
                == GeometryVisualization::SurfaceSplatting
            {
                if let NetworkResult::Blueprint(geometry_summary) = result {
                    let point_cloud = generate_point_cloud(
                        &geometry_summary.geo_tree_root,
                        context,
                        geometry_visualization_preferences,
                    );
                    (
                        NodeOutput::SurfacePointCloud(point_cloud),
                        Some(geometry_summary.geo_tree_root),
                    )
                } else {
                    (NodeOutput::None, None)
                }
            } else if geometry_visualization_preferences.geometry_visualization
                == GeometryVisualization::ExplicitMesh
            {
                self.generate_explicit_mesh_output(
                    result,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            } else {
                (NodeOutput::None, None)
            }
        } else if matches!(
            data_type,
            DataType::Atomic | DataType::Crystal | DataType::Molecule
        ) {
            // Accept both the abstract `Atomic` (still declared by not-yet-migrated
            // nodes as Fixed(Atomic)) and the concrete `Crystal`/`Molecule` pin
            // types. In all three cases the NetworkResult carries a
            // Crystal(..) or Molecule(..) variant from which we extract the
            // inner AtomicStructure for display.
            let (atomic_structure, shell_geo_tree) = match result {
                NetworkResult::Crystal(c) => (Some(c.atoms), c.geo_tree_root),
                NetworkResult::Molecule(m) => (Some(m.atoms), m.geo_tree_root),
                _ => (None, None),
            };
            if let Some(mut atomic_structure) = atomic_structure {
                atomic_structure.decorator_mut().from_selected_node = from_selected_node;
                let shell_output =
                    if geometry_visualization_preferences.show_geometry_shell_for_atomic {
                        shell_geo_tree.and_then(|geo_tree| {
                            self.build_atomic_shell_output(
                                &geo_tree,
                                context,
                                geometry_visualization_preferences,
                            )
                        })
                    } else {
                        None
                    };
                (
                    NodeOutput::Atomic(atomic_structure, shell_output.map(Box::new)),
                    None,
                )
            } else {
                (NodeOutput::None, None)
            }
        } else if let DataType::Array(inner_type) = data_type {
            if let NetworkResult::Array(elements) = result {
                self.convert_array_to_node_output(
                    elements,
                    inner_type,
                    from_selected_node,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            } else {
                (NodeOutput::None, None)
            }
        } else {
            (NodeOutput::None, None)
        }
    }

    /// Merges an array of results into a single displayable output.
    ///
    /// For `Array<Blueprint>`: creates a CSG union of all shapes (like the `union` node).
    /// For `Array<Geometry2D>`: creates a 2D CSG union (like the `union_2d` node).
    /// For `Array<Atomic>`: merges all atomic structures (like the `atom_union` node).
    /// Other element types or empty arrays return `NodeOutput::None`.
    #[allow(clippy::too_many_arguments)]
    fn convert_array_to_node_output<'a>(
        &mut self,
        elements: Vec<NetworkResult>,
        inner_type: &DataType,
        from_selected_node: bool,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
    ) -> (NodeOutput, Option<GeoNode>) {
        if elements.is_empty() {
            return (NodeOutput::None, None);
        }

        match inner_type {
            DataType::Blueprint => {
                let mut shapes: Vec<GeoNode> = Vec::new();
                let mut first_lattice_vecs = None;
                let mut alignment = Alignment::Aligned;
                for element in elements {
                    if let NetworkResult::Blueprint(geo) = element {
                        if first_lattice_vecs.is_none() {
                            first_lattice_vecs = Some(geo.structure.lattice_vecs.clone());
                        }
                        alignment.worsen_to(geo.alignment);
                        shapes.push(geo.geo_tree_root);
                    }
                }
                if shapes.is_empty() {
                    return (NodeOutput::None, None);
                }
                let merged = NetworkResult::Blueprint(BlueprintData {
                    structure: Structure::from_lattice_vecs(first_lattice_vecs.unwrap()),
                    geo_tree_root: GeoNode::union_3d(shapes),
                    alignment,
                });
                self.convert_result_to_node_output(
                    merged,
                    &DataType::Blueprint,
                    from_selected_node,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            }
            DataType::Geometry2D => {
                let mut shapes: Vec<GeoNode> = Vec::new();
                let mut frame_translation = DVec2::ZERO;
                let mut first_drawing_plane = None;
                for element in elements {
                    if let NetworkResult::Geometry2D(geo) = element {
                        if first_drawing_plane.is_none() {
                            first_drawing_plane = Some(geo.drawing_plane.clone());
                        }
                        frame_translation += geo.frame_transform.translation;
                        shapes.push(geo.geo_tree_root);
                    }
                }
                if shapes.is_empty() {
                    return (NodeOutput::None, None);
                }
                let count = shapes.len() as f64;
                frame_translation /= count;
                let merged = NetworkResult::Geometry2D(GeometrySummary2D {
                    drawing_plane: first_drawing_plane.unwrap(),
                    frame_transform: Transform2D::new(frame_translation, 0.0),
                    geo_tree_root: GeoNode::union_2d(shapes),
                });
                self.convert_result_to_node_output(
                    merged,
                    &DataType::Geometry2D,
                    from_selected_node,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            }
            DataType::Atomic | DataType::Crystal | DataType::Molecule => {
                // Same handling for abstract `Atomic` arrays (not-yet-migrated
                // nodes) and concrete `Crystal`/`Molecule` arrays — extract the
                // inner AtomicStructure from each Crystal(..)/Molecule(..)
                // variant and union them for display.
                let mut structures: Vec<AtomicStructure> = Vec::new();
                for element in elements {
                    if let Some(structure) = element.extract_atomic() {
                        structures.push(structure);
                    }
                }
                if structures.is_empty() {
                    return (NodeOutput::None, None);
                }
                let mut result = structures.remove(0);
                for other in &structures {
                    result.add_atomic_structure(other);
                }
                result.decorator_mut().from_selected_node = from_selected_node;
                (NodeOutput::Atomic(result, None), None)
            }
            _ => (NodeOutput::None, None),
        }
    }

    /// Helper method for the common pattern: get value from node data, or override with input pin
    /// Returns the input pin value if connected, otherwise returns the default value
    /// If the input pin evaluation results in an error, returns that error
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::result_large_err)]
    pub fn evaluate_or_default<'a, T>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        parameter_index: usize,
        default_value: T,
        extractor: impl FnOnce(NetworkResult) -> Option<T>,
    ) -> Result<T, NetworkResult> {
        let result = self.evaluate_arg(network_stack, node_id, registry, context, parameter_index);

        if let NetworkResult::None = result {
            return Ok(default_value);
        }

        // Check for error first
        if result.is_error() {
            return Err(result);
        }

        // Try to extract the value
        if let Some(value) = extractor(result) {
            Ok(value)
        } else {
            Ok(default_value)
        }
    }

    /// Helper method for the common pattern: get value from required input pin
    /// Returns the input pin value if connected, otherwise returns the missing input error
    /// If the input pin evaluation results in an error, returns that error
    #[allow(clippy::result_large_err)]
    pub fn evaluate_required<'a, T>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        parameter_index: usize,
        extractor: impl FnOnce(NetworkResult) -> Option<T>,
    ) -> Result<T, NetworkResult> {
        let result =
            self.evaluate_arg_required(network_stack, node_id, registry, context, parameter_index);

        // Check for error first
        if result.is_error() {
            return Err(result);
        }

        // Try to extract the value
        if let Some(value) = extractor(result.clone()) {
            Ok(value)
        } else {
            Err(result)
        }
    }

    // Evaluates an argument of a node.
    // Can return an Error NetworkResult, or a valid NetworkResult.
    // If the atgument is not connected that is an error.
    // If the return value is not an Error, it is guaranteed to be converted to the
    // type of the parameter.
    pub fn evaluate_arg_required<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        parameter_index: usize,
    ) -> NetworkResult {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let input_name = registry.get_parameter_name(node, parameter_index);
        let result = self.evaluate_arg(network_stack, node_id, registry, context, parameter_index);
        if let NetworkResult::None = result {
            input_missing_error(&input_name)
        } else {
            result
        }
    }

    // Evaluates an argument of a node.
    // Can return a NetworkResult::None, NetworkResult::Error, or a valid NetworkResult.
    // Returns NetworkResult::None if the input was not connected.
    // If the return value is not an Error or None, it is guaranteed to be converted to the
    // type of the parameter.
    pub fn evaluate_arg<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        parameter_index: usize,
    ) -> NetworkResult {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let input_name = registry.get_parameter_name(node, parameter_index);

        // Get the expected input type for this parameter
        let expected_type = registry.get_node_param_data_type(node, parameter_index);

        if expected_type.is_array() {
            let input_output_pins = &node.arguments[parameter_index].argument_output_pins;

            if input_output_pins.is_empty() {
                return NetworkResult::None; // Nothing is connected
            }

            let mut merged_items = Vec::new();

            // Sort by node ID to ensure deterministic evaluation order
            // (HashMap iteration order is non-deterministic)
            let mut sorted_pins: Vec<_> = input_output_pins.iter().collect();
            sorted_pins.sort_by_key(|&(&node_id, _)| node_id);

            for (&input_node_id, &input_node_output_pin_index) in sorted_pins {
                let result = self.evaluate(
                    network_stack,
                    input_node_id,
                    input_node_output_pin_index,
                    registry,
                    false,
                    context,
                );

                if let NetworkResult::Error(_) = result {
                    return error_in_input(&input_name);
                }

                let input_node = NetworkStackElement::get_top_node(network_stack, input_node_id);
                let input_node_output_type = registry
                    .get_node_type_for_node(input_node)
                    .unwrap()
                    .output_type()
                    .clone();

                // convert_to handles conversion to array types, so we can convert directly.
                // The result is guaranteed to be an array, containing one or more elements.
                let converted_result =
                    result.convert_to(&input_node_output_type, &expected_type.clone());

                if let NetworkResult::Array(array_data) = converted_result {
                    merged_items.extend(array_data);
                } else {
                    // This should not happen based on the logic of convert_to, but we handle it just in case.
                    return error_in_input(&input_name);
                }
            }

            NetworkResult::Array(merged_items)
        } else {
            // single argument evaluation
            if let Some((input_node_id, input_node_output_pin_index)) =
                node.arguments[parameter_index].get_node_id_and_pin()
            {
                let result = self.evaluate(
                    network_stack,
                    input_node_id,
                    input_node_output_pin_index,
                    registry,
                    false,
                    context,
                );
                if let NetworkResult::Error(_error) = result {
                    return error_in_input(&input_name);
                }

                let input_node = NetworkStackElement::get_top_node(network_stack, input_node_id);
                let input_node_type = registry.get_node_type_for_node(input_node);
                let input_node_output_type = input_node_type
                    .unwrap()
                    .get_output_pin_type(input_node_output_pin_index);

                // Convert the result to the expected type

                result.convert_to(&input_node_output_type, &expected_type)
            } else {
                NetworkResult::None // Nothing is connected
            }
        }
    }

    /// Evaluate a node and return all output pin results.
    /// Used by generate_scene() to avoid redundant evaluation when
    /// displaying multiple output pins of the same node.
    pub fn evaluate_all_outputs<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);

        let eval_output = if registry
            .built_in_node_types
            .contains_key(&node.node_type_name)
        {
            node.data
                .eval(self, network_stack, node_id, registry, decorate, context)
        } else if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
            // custom node — evaluate return node, pass through all outputs
            if !child_network.valid {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "{} is invalid",
                    node.node_type_name
                )));
            }
            let mut child_network_stack = network_stack.to_vec();
            child_network_stack.push(NetworkStackElement {
                node_network: child_network,
                node_id,
            });
            if child_network.return_node_id.is_none() {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "{} has no return node",
                    node.node_type_name
                )));
            }
            let eval_output = self.evaluate_all_outputs(
                &child_network_stack,
                child_network.return_node_id.unwrap(),
                registry,
                false,
                context,
            );
            // Wrap errors with the custom network name for better diagnostics
            let child_display_results = eval_output.display_results;
            let results: Vec<NetworkResult> = eval_output
                .results
                .into_iter()
                .map(|r| {
                    if let NetworkResult::Error(_) = &r {
                        NetworkResult::Error(format!("Error in {}", node.node_type_name))
                    } else {
                        r
                    }
                })
                .collect();
            let mut output = EvalOutput::multi(results);
            output.display_results = child_display_results;
            output
        } else {
            EvalOutput::single(NetworkResult::Error(format!(
                "Unknown node type: {}",
                node.node_type_name
            )))
        };

        // Runtime guard: if a node produced a value whose inferred data type
        // is abstract, that is a bug in a polymorphic node's `eval` (it failed
        // to re-wrap its result in a concrete variant). Replace such values
        // with a NetworkResult::Error so downstream state is not corrupted.
        // In debug builds this also asserts — should be unreachable in a
        // valid, well-implemented graph.
        let mut eval_output = eval_output;
        for (pin_idx, result) in eval_output.results.iter_mut().enumerate() {
            if let Some(t) = result.infer_data_type() {
                if t.is_abstract() {
                    debug_assert!(
                        false,
                        "node {} pin {} produced value with abstract type {:?}",
                        node_id, pin_idx, t
                    );
                    *result = NetworkResult::Error(format!(
                        "node produced value with abstract type {:?} on pin {}",
                        t, pin_idx
                    ));
                }
            }
        }

        // Record error from primary (pin 0) result
        let primary = eval_output.primary();
        if let NetworkResult::Error(error_message) = primary {
            context.node_errors.insert(node_id, error_message.clone());
        }
        // Record per-pin display strings
        let pin_strings: Vec<String> = eval_output
            .results
            .iter()
            .map(|r| r.to_display_string())
            .collect();
        context.node_output_strings.insert(node_id, pin_strings);

        eval_output
    }

    // Evaluates the specified node (calculates the NetworkResult on its output pin).
    pub fn evaluate<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        output_pin_index: i32,
        registry: &NodeTypeRegistry,
        decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
        let node = network_stack
            .last()
            .unwrap()
            .node_network
            .nodes
            .get(&node_id)
            .unwrap();

        let result = if output_pin_index == (-1) {
            let node_type = registry.get_node_type_for_node(node);
            let num_of_params = node_type.unwrap().parameters.len();
            let mut captured_argument_values: Vec<NetworkResult> = Vec::new();

            for i in 0..num_of_params {
                let result = self.evaluate_arg(network_stack, node_id, registry, context, i);
                captured_argument_values.push(result);
            }

            NetworkResult::Function(Closure {
                node_network_name: network_stack
                    .last()
                    .unwrap()
                    .node_network
                    .node_type
                    .name
                    .clone(),
                node_id,
                captured_argument_values,
            })
        } else {
            let node = NetworkStackElement::get_top_node(network_stack, node_id);
            if registry
                .built_in_node_types
                .contains_key(&node.node_type_name)
            {
                let eval_output =
                    node.data
                        .eval(self, network_stack, node_id, registry, decorate, context);
                // Record all pin strings now, since eval() already computed them all.
                // This prevents partial overwrites when get_all_node_output_strings()
                // aggregates across multiple generate_scene() contexts.
                let pin_strings: Vec<String> = eval_output
                    .results
                    .iter()
                    .map(|r| r.to_display_string())
                    .collect();
                context.node_output_strings.insert(node_id, pin_strings);
                eval_output.get(output_pin_index)
            } else if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
                // custom node — pass through the requested output pin index to the return node
                if !child_network.valid {
                    return NetworkResult::Error(format!("{} is invalid", node.node_type_name));
                }
                let mut child_network_stack = network_stack.to_vec();
                child_network_stack.push(NetworkStackElement {
                    node_network: child_network,
                    node_id,
                });
                if child_network.return_node_id.is_none() {
                    return NetworkResult::Error(format!(
                        "{} has no return node",
                        node.node_type_name
                    ));
                }
                let result = self.evaluate(
                    &child_network_stack,
                    child_network.return_node_id.unwrap(),
                    output_pin_index,
                    registry,
                    false,
                    context,
                );
                if let NetworkResult::Error(_error) = &result {
                    NetworkResult::Error(format!("Error in {}", node.node_type_name))
                } else {
                    result
                }
            } else {
                NetworkResult::Error(format!("Unknown node type: {}", node.node_type_name))
            }
        };

        // Check for error and store it in the context
        if let NetworkResult::Error(error_message) = &result {
            context.node_errors.insert(node_id, error_message.clone());
        }

        // Record per-pin display string (single-pin evaluation overwrites)
        let display_string = result.to_display_string();
        let pin_index = if output_pin_index < 0 {
            0
        } else {
            output_pin_index as usize
        };
        let entry = context
            .node_output_strings
            .entry(node_id)
            .or_insert_with(Vec::new);
        // Grow the vec if needed
        if entry.len() <= pin_index {
            entry.resize(pin_index + 1, String::new());
        }
        entry[pin_index] = display_string;

        result
    }
}
