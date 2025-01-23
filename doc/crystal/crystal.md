# Diamond crystal based editing workflow

This document is about how to enable users to create atomically precise diamond crystal based objects.

For simplicity we will only support cubic diamond in the beginning.

## Crystal representation vs. atomic representation

Besides the atoms and bonds there will be a possibility to design small diamond crystals in the software. These are separate representations, which we will discuss later but for now it can be thought of as a space with a special polygonal boundary where the polygons have restrictions on vertex coordinates and face orientations.

When a crystal is 'ready', it can be converted to an atomic representation with the **build crystal** command. After this point it can be edited atom-by-atom as any regular atomic model.

## Crystal representation invariants

The following must be always true for a crystal representation:

- All the  faces of the crystal must be on a plane with a restricted Miller index. The supported Miller indices are in the form of (ABC) where A<=2, B<=2, C<=2. These planes also need to fit on the lattice grid (going through grid points according to their Miller index). It can be easily seen that the operations we will discuss below do not change theses plane properties.
- Vertex coordinates are a more complicated issue. When possible we restrict the vertices on the polygonal boundary of a crystal on a position with integer lattice coordinates. For example this will be true for initially generates shapes vertices. This invariant cannot be consistently maintained when doing unrestricted CSG diff operations though, because planes can sometimes cross each other resulting in a non-integer coordinate vertex. So this will probably not be an invariant, and need not necessarily be an invariant. Optionally though we can experiment with a strict CSG mode, where we only allow a CSG if it results in only integer-coordinate vertices. Thoughtful designers may want to maintain this invariant as well.

## Basic operations

All the operations must guarantee that the crystal representation invariants defined above remain satisfied after the operation.

The following are basic operations that can be implemented with a simple a CSG tree representation and simple UX:

- A crystal can be created from thin air. A dozen simple crystal geometries should be created, like cube, cuboid, tetrahedron, hexagonal prism etc...
- Crystal geometries can be imported as a simple .OBJ file. (3d geometry file). This is supported so that users can create advanced geometries in polygonal editors which would be difficult to model in an early atomCAD version for some reason. The drawback is that the user must satisfy the constraints in the third party editor, which might be difficult in complex cases.  

- A crystal can be translated with with a translation vector with integer lattice coordinates.

- It should be possible to rotate a crystal by 90 degree along any axis direction.

- It is possible to cut away material along a plane. The plane center must fit on a lattice point and the MiIller index of the plane should be constrained according to the crystal constraints. Cutting away material with a plane is good for designing convex crystals.

- It is possible to do CSG (constructive solid geometry) operations on 2 crystals and create one resulting crystal. The two sub possibilities are:

  - A diff B.
  - A union B

  With the CSG operations it is possible to design concave crystals.

## Advanced operations

These operations need a more advanced polygon boundary representation and a more sophisticated UX:

- Draw a constrained 2d profile on the XZ plane in the editor and extrude it.
- Draw a constrained 2d profile on an existing polygon face on a crystal and extrude it perpendicularly. Please note that on any polygonal face, due to the low Miller index there are grid points regularly, but not as densely as on an axis aligned plane. During drawing only these grid points are allowed as vertices to satisfy crystal invariants. For the same reason we allow each drawn edge only after checking whether the resulting perpendicular extruded plane will still have low-enough Miller index.
- Other advanced operations which require a polygonal boundary are possible, but for each one we need to be sure to restrict them in a way that the defined crystal invariants remain satisfied.

## Development roadmap

Basic operations could be supported relatively early on. To support the basic operations we do not need the complex polygon representation needed to edit polygonal objects. We can just store the CSG tree for the crystals and we can display them on the screen as atomic representations (even though they cannot be edited atom by atom until the user decides to 'build' them) or can be converted to a triangle mesh, which is still much simpler than a fully featured polygonal representation for editing purposes.

### How to build the atomic representation

As discussed we can build the atomic representation for display purposes after each edit operation. The algorithm for evaluating the CSG tree is the following:

- Evaluate the CSG tree for each cubic unit cell of the crystal.
- If the cell evaluates to false, nothing is produced for the cell.
- If the cell evaluates to true, generate each atom in the corresponding diamond cubic cell.
- If the cell evaluates to partially true and partially false then do the eval for each atom separately in the cube and with a parametrizable threshold err on the side of adding an atom rather than not adding it. This is the only part of the algorithm which probably needs to be experimented with. As the cutter plane Miller indices are limited, there are a fix number of combinations, for which deciding which atoms to include can be even decided offline, and can be built into the source code.
- Obviously make sure to add a bond between 2 atoms if a bond exists between them in a diamond crystal.
- Add passivation hydrogens to the surface if specified. 

