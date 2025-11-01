# atomCAD

atomCAD is a CAD application for Atomically Precise Manufacturing (APM).
With atomCAD you can design arbitrary covalently bonded atomic structures that are constrained to a crystal lattice and suitable for physical (and, in the future, manufacturable) workflows.

![](./doc/atomCAD_images/nut-bolt-example0-1.png)

Basic features:
- **Arbitrary unit cells.** Any unit cell defined by the lattice parameters `(a, b, c, α, β, γ)` is supported. The implied crystal system (*cubic, tetragonal, orthorhombic, hexagonal, trigonal, monoclinic, triclinic*) and its symmetries are automatically determined.
- **Lattice-constrained geometry.** Geometries are created relative to the unit cell lattice, and operations on those geometries preserve lattice constraints. This makes it easier to design atomic crystal structures that are aligned, physically accurate, and manufacturable.
- **User-definable motifs.** Geometries can be filled with motifs to create atomic structures. Motifs are user-definable: any covalently bonded crystal structure can be specified. The default motif is cubic diamond.
- **Parametric, composable designs.** atomCAD designs are parametric and composed as visual node networks, enabling non-destructive editing. Custom node types can be created by defining subnetworks. The node network includes functional-programming primitives for complex programmatic designs.

Planned features include:

- Surface reconstruction support
- Defect editing and placement tools
- Atomically Precise Manufacturing (APM) integration
- A streaming level-of-detail system to support larger structures that currently do not fit in memory
- Geometry optimization and dynamics simulation support

We’d love to hear about your use case: what are you using — or planning to use — atomCAD for?

For more details see [atomCAD User Guide & Reference](./doc/atomCAD_user_guide.md).

Interested in contributing? See our [developer documentation](./doc/for_developers.md) to get started.
