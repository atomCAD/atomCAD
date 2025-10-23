Note on 2025-10-23: This document is not up to date. We keep this as it contains
important ideas for the future development of the renderer in atomCAD.

# Renderer

- Only atoms, bonds and editor 'gizmos' need to be rendered, so the renderer do not need to be general purpose

- Must support multiple views (like balls and stick vs. balls only)
- Must Handle huge models in the long run (not in the short run though, so we can start with something simple)

## Requirements

The task is to render a huge number of atoms and bonds while sometimes

they even change due to editing, but most of the time only the camera moves.



Solution(s) need to be found that satisfy all the different, sometimes almost contradictory requirements to a sufficient degree.



- Rendered vertex count should not be too high (=> Imposters?, LODs?)
- Draw call count should not be too high
- It would be nice to be able to frustum cull parts of the scene
- Streaming differences to the GPU when editing should not be too slow
- Occlusion culling would be nice

### Kernel dependency

The renderer need the following from the kernel:

- Read only access to the whole model to be able to re-fill GPU buffers based on the model
- Except for the Naive approach we need a report from the kernel frame-by frame about which atoms and bonds changed.

## Flexibility

It is important that the renderer is very well separated from other parts of the application (Kernel and UI) and so we can experiment with multiple rendering solutions without introducing dependencies in the other parts of the application.

Generally the renderer should support multiple approaches and multiple options in one approach. I would like to support everything that is discussed below. 

## Support both triangle meshes and imposters

Supporting triangle meshes is low-effort so it will be supported as a baseline. It is useful because:

- We can have a rendering solution in the early phases in the application development
- Good to have a baseline to detect bugs in other approaches
- Easy to support new kind of views quickly.

Rendering atoms using imposters is a huge win in terms of vertex count though, so it will be eventually also supported.

Imposter-based rendering works the following way: For each atom and bond we use one quad (two triangles fromed from 4 vertices).

#### Imposter based atom rendering

For atoms all the vertices have originally the same 3D coordinate (the center of the atom), but each vertex knows which vertex it is from the 4, and so can calculate its offseted screen position in the vertex shader to become one corner of the viewer facing quad.

The pixel shader should calculate not just proper color, but also proper depth and alpha. Alpha can be used for pixels not on the sphere and for antialiasing the edges.

#### Imposter based bond rendering

Bonds are represented as cylinders. Only the curved part needs to be rendered, the top and bottom is never seen. This can be done on a quad that is the cross-section of the cylinder facing the viewer. The corner positions can be finalized in the vertex shader, the original 3D coordinates for these vertices are the bottom center (for 2 vertices) and the top center (2 vertices).

#### Order independent Imposter based rendering?

Where I am uncertain is whether the above approach for handling edge antialiasing with alpha can be good-enough qulity without a sorted back to front rendering. Without sorting we can render with one draw-call from a vertex buffer. Ordered rendering potentially needs a less efficient solution.

## Support partitioning and LODs

Without partitioning we always just render one big vertex buffer. (it can be triangles meshes or imposter quads, but the whole model is always just one big vertex buffer, and one draw call is enough.)

We can support partitioning: We separate the scene into parts using some sort of space partitioning. I think a simple regular 3D grid would work well because the spatial details in our case are quite evenly distributed. In case of triangle meshes we tessellate multiple LODS for each part. In case of imposters multiple LODs is not needed. 

Partitioning is useful because:

- In case of triangle meshes we render the proper LOD for each part based on its distance.
- We can frustrum cull parts easily.
- Now that we have parts, when editing, it is enough to re-tessellate only the parts that changed 

Fortunately there are lots of things that are simpler in our case than in general renderers. One of the most important is that we do not have cracks when two neighboring parts are displayed in different LOD levels. It will be simple to implement dynamic LOD transitioning too, but will not implement it for the MVP.

