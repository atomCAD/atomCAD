use crate::common::atomic_structure::AtomicStructure;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::common::surface_point_cloud::SurfacePointCloud2D;
use crate::renderer::tessellator::tessellator::Tessellatable;
use std::collections::HashMap;
use std::any::Any;
use crate::common::poly_mesh::PolyMesh;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

/// The explicit geometric/data output of a single node evaluation
/// This represents the tessellatable/renderable output
pub enum NodeOutput {
    /// Atomic structure (from atomic nodes)
    Atomic(AtomicStructure),
    
    /// 3D surface point cloud (from geometry visualization)
    SurfacePointCloud(SurfacePointCloud),
    
    /// 2D surface point cloud (from 2D geometry visualization)
    SurfacePointCloud2D(SurfacePointCloud2D),
    
    /// Explicit polygon mesh (from geometry conversion)
    PolyMesh(PolyMesh),
    
    /// No explicit output (for nodes that don't produce displayable results)
    None,
}

/// Represents the complete output of evaluating a single displayed node,
/// including its primary output and all metadata from the evaluation chain
pub struct NodeSceneData {
    /// The explicit renderable output of this node (atomic structure, mesh, point cloud, etc.)
    pub output: NodeOutput,
    
    /// The CSG geometry tree (if this is a geometry node)
    /// This can coexist with explicit output (e.g., a PolyMesh derived from this geo_tree)
    /// The geo_tree is kept for potential future operations or alternative visualizations
    pub geo_tree: Option<GeoNode>,
    
    /// Errors collected during evaluation of this node and its dependencies
    /// Maps node_id -> error_message for all nodes in the evaluation chain
    pub node_errors: HashMap<u64, String>,
    
    /// Output strings collected during evaluation of this node and its dependencies
    /// Maps node_id -> output_string for all nodes in the evaluation chain
    pub node_output_strings: HashMap<u64, String>,
    
    /// Unit cell associated with this node's output (if applicable)
    pub unit_cell: Option<UnitCellStruct>,
    
    /// Eval cache for this node (used for gadget creation if this is the selected node)
    /// Contains node-specific data needed to reconstruct gadgets across refresh cycles
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
}

impl NodeSceneData {
    pub fn new(output: NodeOutput) -> Self {
        Self {
            output,
            geo_tree: None,
            node_errors: HashMap::new(),
            node_output_strings: HashMap::new(),
            unit_cell: None,
            selected_node_eval_cache: None,
        }
    }
}

// StructureDesignerScene is a struct that holds the scene to be rendered in the structure designer.
pub struct StructureDesignerScene {
    /// Per-node scene data, keyed by node ID
    /// Each entry contains the node's output, geo_tree (if applicable), and evaluation metadata
    pub node_data: HashMap<u64, NodeSceneData>,
    
    /// Gadget for the currently selected node (if any)
    /// Created after evaluation, not part of node evaluation output
    pub tessellatable: Option<Box<dyn Tessellatable>>,

    /// Eval cache for the selected node (used to create gadgets)
    /// Stored separately because gadgets need it across refresh cycles
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
    
    /// Unit cell from the selected node (used for background rendering)
    /// Overrides individual node unit cells for global scene context
    pub unit_cell: Option<UnitCellStruct>,
}

impl StructureDesignerScene {
    pub fn new() -> Self {
        Self {
            node_data: HashMap::new(),
            tessellatable: None,
            selected_node_eval_cache: None,
            unit_cell: None,
        }
    }
    
    /// Helper to get all errors from all nodes
    pub fn get_all_node_errors(&self) -> HashMap<u64, String> {
        let mut all_errors = HashMap::new();
        for (_, node_data) in &self.node_data {
            all_errors.extend(node_data.node_errors.clone());
        }
        all_errors
    }
    
    /// Helper to get all output strings from all nodes
    pub fn get_all_node_output_strings(&self) -> HashMap<u64, String> {
        let mut all_strings = HashMap::new();
        for (_, node_data) in &self.node_data {
            all_strings.extend(node_data.node_output_strings.clone());
        }
        all_strings
    }
}
