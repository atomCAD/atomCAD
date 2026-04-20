use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::drawing_plane::DrawingPlane;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::display::poly_mesh::PolyMesh;
use crate::display::surface_point_cloud::SurfacePointCloud;
use crate::display::surface_point_cloud::SurfacePointCloud2D;
use crate::geo_tree::GeoNode;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::structure_designer::evaluator::network_result::Alignment;
use crate::util::memory_bounded_lru_cache::MemoryBoundedLruCache;
use crate::util::memory_size_estimator::MemorySizeEstimator;
use std::any::Any;
use std::collections::{HashMap, HashSet};

/// Output of a single displayed output pin.
pub struct DisplayedPinOutput {
    pub pin_index: i32,
    pub output: NodeOutput,
    pub geo_tree: Option<GeoNode>,
    /// Alignment of this pin's value, if it carries one (Blueprint/Crystal).
    /// None for types that have no alignment (Molecule, primitives, etc.).
    pub alignment: Option<Alignment>,
}

/// The explicit geometric/data output of a single node evaluation
/// This represents the tessellatable/renderable output
pub enum NodeOutput {
    /// Atomic structure (from atomic nodes). The optional boxed `NodeOutput`
    /// is a pre-computed geometry shell (`PolyMesh` or `SurfacePointCloud`)
    /// for Crystal/Molecule phases — produced by the evaluator when the
    /// source payload retained a geo_tree and the shell-display preference
    /// is on.
    Atomic(AtomicStructure, Option<Box<NodeOutput>>),

    /// 3D surface point cloud (from geometry visualization)
    SurfacePointCloud(SurfacePointCloud),

    /// 2D surface point cloud (from 2D geometry visualization)
    SurfacePointCloud2D(SurfacePointCloud2D),

    /// Explicit polygon mesh (from geometry conversion)
    PolyMesh(PolyMesh),

    /// Drawing plane (from drawing plane nodes)
    DrawingPlane(DrawingPlane),

    /// No explicit output (for nodes that don't produce displayable results)
    None,
}

/// Represents the complete output of evaluating a single displayed node,
/// including its primary output and all metadata from the evaluation chain
pub struct NodeSceneData {
    /// The primary renderable output of this node (backward-compatible: always pin 0)
    pub output: NodeOutput,

    /// The CSG geometry tree for the primary output (if this is a geometry node)
    pub geo_tree: Option<GeoNode>,

    /// Converted outputs for ALL output pins of this node (not just displayed ones).
    /// Pin 0 uses a `NodeOutput::None` marker — its actual data is in `output`/`geo_tree`.
    /// This is populated at evaluation time and remains valid across pin display toggles.
    pub pin_outputs: Vec<DisplayedPinOutput>,

    /// Which output pins are currently displayed. Updated by pin display toggles
    /// without re-evaluation. `displayed_outputs()` uses this to filter `pin_outputs`.
    pub displayed_pins: HashSet<i32>,

    /// Errors collected during evaluation of this node and its dependencies
    /// Maps node_id -> error_message for all nodes in the evaluation chain
    pub node_errors: HashMap<u64, String>,

    /// Output strings collected during evaluation of this node and its dependencies
    /// Maps node_id -> per-pin output strings for all nodes in the evaluation chain
    pub node_output_strings: HashMap<u64, Vec<String>>,

    /// Unit cell associated with this node's output (if applicable)
    pub unit_cell: Option<UnitCellStruct>,

    /// Whether to render a unit cell wireframe for this node (motif_edit only)
    pub show_unit_cell_wireframe: bool,

    /// Eval cache for this node (used for gadget creation if this is the selected node)
    /// Contains node-specific data needed to reconstruct gadgets across refresh cycles
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
}

impl NodeSceneData {
    pub fn new(output: NodeOutput) -> Self {
        Self {
            output,
            geo_tree: None,
            pin_outputs: Vec::new(),
            displayed_pins: HashSet::from([0]),
            node_errors: HashMap::new(),
            node_output_strings: HashMap::new(),
            unit_cell: None,
            show_unit_cell_wireframe: false,
            selected_node_eval_cache: None,
        }
    }

    /// Get the interactive pin index (lowest-indexed displayed pin).
    pub fn interactive_pin_index(&self) -> Option<i32> {
        self.displayed_pins.iter().copied().min()
    }

    /// Get the output for the interactive pin (lowest-indexed displayed pin).
    pub fn interactive_output(&self) -> Option<&NodeOutput> {
        let interactive_idx = self.interactive_pin_index()?;
        if interactive_idx == 0 {
            Some(&self.output)
        } else {
            self.pin_outputs
                .iter()
                .find(|p| p.pin_index == interactive_idx)
                .map(|p| &p.output)
        }
    }

    /// Iterate over displayed pin outputs only, resolving pin 0 from the main
    /// `output`/`geo_tree` fields. Each item is `(pin_index, &NodeOutput, Option<&GeoNode>)`.
    ///
    /// When `pin_outputs` is empty (e.g. nodes constructed without multi-output info),
    /// falls back to yielding the main `output`/`geo_tree` as pin 0.
    pub fn displayed_outputs(&self) -> impl Iterator<Item = (i32, &NodeOutput, Option<&GeoNode>)> {
        let use_fallback = self.pin_outputs.is_empty();
        let fallback = use_fallback.then_some((0, &self.output, self.geo_tree.as_ref()));
        let displayed_pins = &self.displayed_pins;
        fallback.into_iter().chain(
            self.pin_outputs
                .iter()
                .filter(move |p| displayed_pins.contains(&p.pin_index))
                .map(move |p| {
                    if p.pin_index == 0 {
                        (0, &self.output, self.geo_tree.as_ref())
                    } else {
                        (p.pin_index, &p.output, p.geo_tree.as_ref())
                    }
                }),
        )
    }
}

// StructureDesignerScene is a struct that holds the scene to be rendered in the structure designer.
pub struct StructureDesignerScene {
    /// Per-node scene data, keyed by node ID (for visible nodes)
    /// Each entry contains the node's output, geo_tree (if applicable), and evaluation metadata
    pub node_data: HashMap<u64, NodeSceneData>,

    /// LRU cache for invisible node scene data
    /// Retains recently invisible nodes to enable ultra-fast visibility restoration
    /// Memory-bounded to prevent excessive memory usage
    invisible_node_cache: MemoryBoundedLruCache<u64, NodeSceneData>,

    /// Gadget for the currently selected node (if any)
    /// Created after evaluation, not part of node evaluation output
    pub tessellatable: Option<Box<dyn Tessellatable>>,

    /// Unit cell from the selected node (used for background rendering)
    /// Overrides individual node unit cells for global scene context
    pub unit_cell: Option<UnitCellStruct>,

    /// The active node ID - used by the tessellator to render active geometry
    /// with a distinct color
    pub active_node_id: Option<u64>,
}

impl Default for StructureDesignerScene {
    fn default() -> Self {
        Self::new()
    }
}

impl StructureDesignerScene {
    /// Default cache size: 256 MB for invisible nodes
    const DEFAULT_INVISIBLE_CACHE_SIZE_BYTES: usize = 256 * 1024 * 1024;

    pub fn new() -> Self {
        Self {
            node_data: HashMap::new(),
            invisible_node_cache: MemoryBoundedLruCache::new(
                Self::DEFAULT_INVISIBLE_CACHE_SIZE_BYTES,
                |node_data: &NodeSceneData| node_data.estimate_memory_bytes(),
            ),
            tessellatable: None,
            unit_cell: None,
            active_node_id: None,
        }
    }

    /// Helper to get all errors from all nodes
    pub fn get_all_node_errors(&self) -> HashMap<u64, String> {
        let mut all_errors = HashMap::new();
        for node_data in self.node_data.values() {
            all_errors.extend(node_data.node_errors.clone());
        }
        all_errors
    }

    /// Helper to get all output strings from all nodes
    pub fn get_all_node_output_strings(&self) -> HashMap<u64, Vec<String>> {
        let mut all_strings = HashMap::new();
        for node_data in self.node_data.values() {
            all_strings.extend(node_data.node_output_strings.clone());
        }
        all_strings
    }

    // Cache management methods for invisible nodes

    /// Moves a node from visible (node_data) to invisible (cache)
    /// Returns true if the node was found and moved, false otherwise
    pub fn move_to_cache(&mut self, node_id: u64) -> bool {
        if let Some(node_data) = self.node_data.remove(&node_id) {
            self.invisible_node_cache.insert(node_id, node_data);
            true
        } else {
            false
        }
    }

    /// Restores a node from invisible cache to visible node_data
    /// Returns true if the node was found in cache and restored, false otherwise
    pub fn restore_from_cache(&mut self, node_id: u64) -> bool {
        if let Some(node_data) = self.invisible_node_cache.pop(&node_id) {
            self.node_data.insert(node_id, node_data);
            true
        } else {
            false
        }
    }

    /// Updates the `displayed_pins` on a node in the invisible cache (if present).
    pub fn update_cached_displayed_pins(&mut self, node_id: u64, displayed_pins: HashSet<i32>) {
        if let Some(node_data) = self.invisible_node_cache.get_mut(&node_id) {
            node_data.displayed_pins = displayed_pins;
        }
    }

    /// Invalidates (removes) cached nodes that are affected by data changes
    /// This ensures stale cached data is not restored when nodes become visible again
    ///
    /// # Arguments
    /// * `node_ids` - Set of node IDs to invalidate from the cache
    pub fn invalidate_cached_nodes(&mut self, node_ids: &HashSet<u64>) {
        for &node_id in node_ids {
            self.invisible_node_cache.pop(&node_id);
        }
    }

    /// Returns the number of nodes currently in the invisible cache
    pub fn cached_node_count(&self) -> usize {
        self.invisible_node_cache.len()
    }

    /// Returns the current memory usage of the invisible cache in bytes
    pub fn cached_memory_bytes(&self) -> usize {
        self.invisible_node_cache.current_memory_bytes()
    }

    /// Gets the eval cache for a specific node (typically the selected node)
    /// Returns None if the node doesn't exist or has no eval cache
    pub fn get_node_eval_cache(&self, node_id: u64) -> Option<&Box<dyn Any>> {
        self.node_data
            .get(&node_id)?
            .selected_node_eval_cache
            .as_ref()
    }
}

// Memory size estimation implementations

impl MemorySizeEstimator for NodeOutput {
    fn estimate_memory_bytes(&self) -> usize {
        let base_size = std::mem::size_of::<NodeOutput>();

        let variant_size = match self {
            NodeOutput::Atomic(atomic_structure, shell) => {
                atomic_structure.estimate_memory_bytes()
                    + shell
                        .as_ref()
                        .map(|s| s.estimate_memory_bytes())
                        .unwrap_or(0)
            }
            NodeOutput::SurfacePointCloud(point_cloud) => point_cloud.estimate_memory_bytes(),
            NodeOutput::SurfacePointCloud2D(point_cloud_2d) => {
                point_cloud_2d.estimate_memory_bytes()
            }
            NodeOutput::PolyMesh(poly_mesh) => poly_mesh.estimate_memory_bytes(),
            NodeOutput::DrawingPlane(_drawing_plane) => std::mem::size_of::<DrawingPlane>(),
            NodeOutput::None => 0,
        };

        base_size + variant_size
    }
}

impl MemorySizeEstimator for NodeSceneData {
    fn estimate_memory_bytes(&self) -> usize {
        let base_size = std::mem::size_of::<NodeSceneData>();

        // Estimate output
        let output_size = self.output.estimate_memory_bytes();

        // Estimate geo_tree (if present)
        let geo_tree_size = self
            .geo_tree
            .as_ref()
            .map(|tree| tree.estimate_memory_bytes())
            .unwrap_or(0);

        // Estimate node_errors HashMap
        let node_errors_size = self
            .node_errors
            .values()
            .map(|value| {
                std::mem::size_of::<u64>() + std::mem::size_of::<String>() + value.capacity()
            })
            .sum::<usize>();

        // Estimate node_output_strings HashMap
        let node_output_strings_size = self
            .node_output_strings
            .values()
            .map(|strings| {
                std::mem::size_of::<u64>()
                    + std::mem::size_of::<Vec<String>>()
                    + strings
                        .iter()
                        .map(|s| std::mem::size_of::<String>() + s.capacity())
                        .sum::<usize>()
            })
            .sum::<usize>();

        // Estimate unit_cell (if present)
        // UnitCellStruct is a simple struct with a few f64 fields, estimate conservatively
        let unit_cell_size = if self.unit_cell.is_some() {
            std::mem::size_of::<UnitCellStruct>()
        } else {
            0
        };

        // selected_node_eval_cache is a Box<dyn Any> - we can't know its size
        // Estimate conservatively as the size of the Box pointer
        let eval_cache_size = if self.selected_node_eval_cache.is_some() {
            std::mem::size_of::<Box<dyn Any>>()
        } else {
            0
        };

        // Estimate pin_outputs Vec
        let pin_outputs_size: usize = self
            .pin_outputs
            .iter()
            .map(|p| {
                p.output.estimate_memory_bytes()
                    + p.geo_tree
                        .as_ref()
                        .map(|t| t.estimate_memory_bytes())
                        .unwrap_or(0)
                    + std::mem::size_of::<DisplayedPinOutput>()
            })
            .sum();

        base_size
            + output_size
            + geo_tree_size
            + pin_outputs_size
            + node_errors_size
            + node_output_strings_size
            + unit_cell_size
            + eval_cache_size
    }
}
