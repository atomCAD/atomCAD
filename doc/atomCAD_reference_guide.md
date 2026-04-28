# atomCAD Reference Guide

*A comprehensive guide to atomCAD, including a complete reference for all built-in nodes.*

## Introduction

atomCAD is a CAD application for Atomically Precise Manufacturing (APM).
With atomCAD you can design arbitrary covalently bonded atomic structures that are constrained to a crystal lattice and suitable for physical (and, in the future, manufacturable) workflows.

![](./atomCAD_images/nut-bolt-example0-1.png)

If you are new to atomCAD check out the [atomCAD Basic Tutorial](./atomCAD_basic_tutorial.md) and the [atomCAD Part Design Tutorial](./part_design_tutorial.md) first.

Basic features:
- **Arbitrary unit cells.** Any unit cell defined by the lattice parameters `(a, b, c, α, β, γ)` is supported. The implied crystal system (*cubic, tetragonal, orthorhombic, hexagonal, trigonal, monoclinic, triclinic*) and its symmetries are automatically determined.
- **Lattice-constrained geometry.** Geometries are created relative to the unit cell lattice, and operations on those geometries preserve lattice constraints. This makes it easier to design atomic crystal structures that are aligned, physically accurate, and manufacturable.
- **User-definable motifs.** Geometries can be filled with motifs to create atomic structures. Motifs are user-definable: any covalently bonded crystal structure can be specified. The default motif is cubic diamond.
- **Parametric, composable designs.** atomCAD designs are parametric and composed as visual node networks, enabling non-destructive editing. Custom node types can be created by defining subnetworks. The node network includes functional-programming elements and is Turing-complete.
- **Surface reconstructions.** Currently only (100) 2×1 dimer reconstruction is supported (for cubic diamond) but more reconstructions will be supported in the future.
- **Direct Editing Mode.** A streamlined entry point for beginners that hides node-network complexity, letting users build atomic structures immediately.
- **Energy minimization.** UFF force-field energy minimization is available both as a standalone node and integrated into the atom editor for interactive structure relaxation.
- **Hydrogen passivation & depassivation.** Automatically add or remove hydrogen atoms to satisfy valence requirements, available as both one-click actions in the atom editor and as standalone nodes.

Planned features include:

- Dynamics simulation support and access to more accurate (server-side) energy minimization methods
- Atomically Precise Manufacturing (APM) integration
- A streaming level-of-detail system to support larger structures that currently do not fit in memory

We’d love to hear about your use case: what are you using — or planning to use — atomCAD for?

## Contents

- [Direct Editing Mode](./reference_guide/direct_editing.md) — the simplified beginner mode and the atom editor.
- [Parts of the UI](./reference_guide/ui.md) — viewport, panels, menu bar, preferences.
- [Node Networks](./reference_guide/node_networks.md) — core concepts: data types, subnetworks, functional programming.
- [Nodes reference](#nodes-reference) — built-in node categories.
- [Headless Mode (CLI)](./reference_guide/headless_cli.md)
- [Using with Claude Code](./reference_guide/claude_code.md)

## Nodes reference

We categorize nodes by their functionality and/or output pin data type. There are the following categories of nodes:

- [Annotation nodes](./reference_guide/nodes/annotation.md)
- [Math and programming nodes](./reference_guide/nodes/math_programming.md)
- [2D Geometry nodes](./reference_guide/nodes/geometry_2d.md)
- [3D Geometry nodes](./reference_guide/nodes/geometry_3d.md)
- [Atomic structure nodes](./reference_guide/nodes/atomic.md)
- [Other nodes](./reference_guide/nodes/other.md)

You create 2D geometry to eventually use the **extrude** node to create a 3D `Blueprint` from it. You create a `Blueprint` to eventually use the **atom_fill** node to materialize an atomic structure from it.
