In this document we analyze the dataflow in atomCAD structure designer as of September 4, 2025:
The structure of data coming out from the node network, in what format it is cached for interaction,
and how the data is used to generate the scene to be rendered.

The codebase evolved significantly since the original design and we need to see how the original concepts hold up and
what needs to be changed.

## Original concept and current state

The original concept was that the node network evaluation produces a 'scene' object and the renderer knows how to render a scene object.

As multiple nodes can be visible at the same time, the scene object supports merging, multiple scene objects can be merged into one.

It quickly bacame obvious that the last displayed scene needs to be cached for interaction purposes so StructureDesigner has the following member:

  `pub last_generated_structure_designer_scene: StructureDesignerScene`

The scene representation is rather arbitrary: it is a collection of atomic structures, surface point clouds, meshes:
sometimes high level data sometimes lower level data structures closer to the renderer.
Usually it is the data that is needed for interaction.

Currently I am in the middle of the refactor to create the geo_tree intermediate rperesentation for geoemtries:
Should it be part of the StructureDesignerScene? The answer is yes, as it is needed for being able to interact with the geometry:
needed for tracing rays into it using implicit eval.

This means that the philosophy of the StructureDesignerScene should be: contain everything needed for interaction
possibly multiple layers of representations if needed.

Representations that are only needed for rendering can be pushed into the renderer though.

This means that for example 'point cloud' might not be needed in the scene, as geo_tree is there.

This would then make the Renderer even bigger though: that is a different problem: we lack modularisation inside the renderer.

Current action items:

Urgent: put geo_tree into StructureDesignerScene
Not urgent: push lower level geometry representations into the Renderer.
Even more Later: modularise the renderer more




