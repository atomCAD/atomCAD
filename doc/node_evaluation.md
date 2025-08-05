As the structure designer evolved node network evaluation also evolved.
Some complexity had to be introduced to support real-time editing operations
and different geometry visualizations.

The key Rust structs are the following:

- StructureDesignerScene
- StructureDesigner
- NetworkEvaluationContext
- NetworkEvaluator
- ImplicitEvaluator

StructureDesigner and StructureDesignerScene are the higher level structs which rely
on the lower level evaluation related structs: NetworkEvaluationContext, NetworkEvaluator and ImplicitEvaluator.

Let's review all of them, let's go from top top bottom

## StructureDesignerScene

StructureDesignerScene is a struct that holds the scene to be rendered in the structure designer.

Let's look at all its data members:

    pub struct StructureDesignerScene {
      pub atomic_structures: Vec<AtomicStructure>,
      pub surface_point_clouds: Vec<SurfacePointCloud>,
      pub surface_point_cloud_2ds: Vec<SurfacePointCloud2D>,
      pub poly_meshes: Vec<PolyMesh>,

      pub tessellatable: Option<Box<dyn Tessellatable>>,

      pub node_errors: HashMap<u64, String>,
      pub selected_node_eval_cache: Option<Box<dyn Any>>,
    }

As you can see it contains anything that can ber rendered in the editor: atomic structurees, polymeshes, surface point clouds, etc...
The tessellable member contains anything that implements the Tessellable interface, it is typically used to
include the active editor gadget.

The node_errors member is used to store any error messages that may have occurred during the evaluation of the displayed nodes.

The selected_node_eval_cache member is the least intuitive one. It is needed for the editor functionality: the selected node can store here anything during its evaluation so that it can be accessed by the editor typically to set up the gadget for the selected node. Usually gadgets only need to access the node data, but sometimes they need calculated values that are only available after the evaluation of the node.

## StructureDesigner

StructureDesigner has a public refresh method:

    // Generates the scene to be rendered according to the displayed nodes of the active node network
    pub fn refresh(&mut self, lightweight: bool)

This is a void method. It generates the scene using NetworkEvaluator::generate_scene and
stores the result in StructureDesigner::last_generated_structure_designer_scene.

## NetworkEvaluator

It contains the public generate_Scene method used by structure_designer to generate the scene based on one node.
This is the signature of the method:

  pub fn generate_scene(
    &self,
    network_name: &str,
    node_id: u64,
    _display_type: NodeDisplayType, //TODO: use display_type
    registry: &NodeTypeRegistry,
    geometry_visualization_preferences: &GeometryVisualizationPreferences,
  ) -> StructureDesignerScene

The way the scene is generated heavily depends on the geometry_visualization_preferences parameter and whether the output
type of the node is atomic structure or geometry.
Scene generation can involve direct evaluation of nodes and implicit node evaluation.
Direct evaluation of a node always needs to be called but implicit evaluation is only called for geometry nodes and even for them only if either the geometry visualization preferences are set to point cloud an atomic structure need to sample from the geometry (this is the case in case of the geo_to_atom node).

Direct evaluation for a node needs to be called even if implicit evaluation is used for the node because
in some cases implicit evaluation want to use its result as a precalculated value.
The reason is that implicit evaluation is called per sample point so it is extremely performance intensive.
Direct evaluation also needs to be called for every node to calculate the frame_transform for that node and also to calculate the selected_node_eval_cache if the node is the selected node.

## EvaluationContext

EvaluationContext is a struct that is passed to the evaluate method of a node evaluator.
It contains 'input members' which parametrize the evaluation and also contains 'output members' to collect some results
during the evaluation. It is passed down recursively to every function which is involved in the node evaluation.



 
