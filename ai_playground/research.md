# AI Playground - Diamond Nanostructure Design Research

## Project Intent

This is an **experimental design project** exploring what can be built with atomCAD using cubic diamond lattices. The goal is to design small nanoscale components (single-digit nanometers) that could have practical relevance for **Atomically Precise Manufacturing (APM)**.

This project serves multiple purposes:
1. **Discover viable component designs** - What shapes and structures work well at the nanoscale?
2. **Build a library of reusable nodes** - Create parameterized custom nodes that can be composed
3. **Learn atomCAD's capabilities and limits** - Document what works, what doesn't, and workarounds
4. **Iterate on designs** - Create v1, v2, v3... of components as we learn better approaches

---

## Crystal-First Design Philosophy

### Core Principle: Work WITH the Lattice, Not Against It

Diamond has a discrete crystalline structure. Designs that respect crystallographic planes and symmetries will produce cleaner, more stable atomic structures than those that fight the lattice.

### Preferred Building Blocks (in order of preference)

1. **`half_space`** - The fundamental primitive. Intersecting half-spaces creates faceted polyhedra that align perfectly with crystal planes. Miller indices define crystallographically meaningful cuts.

2. **`polygon` + `extrude`** - For prismatic shapes. Define precise 2D cross-sections and extrude along lattice directions. Gives exact control over faceted geometry.

3. **`cuboid`** - Axis-aligned boxes. Natural fit for cubic lattice when aligned with <100> directions.

4. **`facet_shell`** - Creates polyhedral shells bounded by crystal facets. Powerful for complex crystallographic shapes.

### Use Sparingly

- **`sphere`** - Spheres are approximations in a discrete lattice. The "surface" will be jagged at the atomic level. Use only when isotropy is truly needed.

- **`circle`** - Same issue as sphere. Prefer `polygon` with many sides if you need approximate roundness, or better yet, use crystallographically-aligned shapes.

### Why This Matters

- **Crystal planes have specific surface energies** - (111) diamond surfaces are different from (100) surfaces
- **Symmetry operations preserve structure** - Designs using lattice symmetries are more likely to be physically stable
- **Cleaner atom_fill results** - Faceted shapes aligned to the lattice produce fewer "stray" atoms and cleaner passivation

### Cubic Diamond Symmetries to Leverage

Cubic diamond has **Oh symmetry** (48 operations). Key symmetries:
- **4-fold axes** along <100> directions (cube faces)
- **3-fold axes** along <111> directions (cube diagonals)
- **2-fold axes** along <110> directions (cube edges)
- **Mirror planes** - {100}, {110}, and {111} families

Design components that exploit these symmetries for:
- Balanced/symmetric parts (rotors, connectors)
- Interlocking pieces that mate along symmetry planes
- Surfaces with predictable atomic arrangements

---

## Rules for AI Agent Sessions

### Document Maintenance
**Keep this file concise!** AI agents have limited context windows.
- Session logs: 3-5 bullet points max per session, focus on outcomes not process
- Periodically archive old session logs to `session_archive.md` if this file grows too long
- Prefer tables and lists over prose
- Delete obsolete information rather than accumulating it

### Before Starting Work
1. **Read this file first** - Understand current state and what's been tried
2. **Check the Session Log** below for recent progress
3. The user will load in ai_playground/project.cnnd: **Review existing node networks in it** to understand what's been built

### During Work
1. **Use the atomcad skill** - All node network creation should go through `atomcad-cli`
2. **Create custom nodes** - Prefer building reusable parameterized subnetworks over one-off designs
3. **Think crystallographically** - Use half_space, polygon+extrude, respect lattice symmetries
4. **Take screenshots** - Visual documentation helps future sessions understand designs
5. **Document learnings** - Add notes about what worked/didn't work

### After Work
1. **Update this file** - Add a session log entry summarizing what was done
2. **Update component status** - Mark components as designed/tested/refined
3. **Note any issues or ideas** - Help the next session continue productively
4. The user will save the progress into project.cnnd.

---

## Viable Components to Build

### Tier 1: Simple (Low Complexity)
Faceted shapes using half_space intersections or simple extrusions.

| Component | Description | Status | Notes |
|-----------|-------------|--------|-------|
| **Octahedron** | 8-faced shape from {111} planes | Not started | intersect 8 half_spaces, natural diamond form |
| **Truncated Cube** | Cube with chamfered corners | Not started | cuboid + half_spaces cutting corners |
| **Hexagonal Prism** | 6-sided column | Not started | polygon(6) + extrude |
| **Wedge** | Triangular prism | Not started | polygon(3) + extrude |
| **L-Bracket** | Corner structural element | Not started | union of two cuboids |
| **Channel/Groove** | U-shaped trough | Not started | diff(cuboid, cuboid) |

### Tier 2: Medium Complexity
Require multiple boolean ops or careful crystallographic alignment.

| Component | Description | Status | Notes |
|-----------|-------------|--------|-------|
| **Dovetail Joint (Male)** | Interlocking connector piece | Not started | Extruded trapezoid polygon |
| **Dovetail Joint (Female)** | Matching receptacle | Not started | Must fit male piece atomically |
| **Rhombic Prism** | Diamond-shaped cross-section | Not started | polygon(4) + extrude, {110} alignment |
| **Stepped Shaft** | Rod with diameter changes | Not started | Union of aligned prisms |
| **Slot/Keyway** | Rectangular channel for alignment | Not started | diff with extruded rect |
| **Pyramidal Tip** | 4-sided point | Not started | 4 half_spaces meeting at apex |

### Tier 3: Advanced
Complex crystallographic shapes or multi-part assemblies.

| Component | Description | Status | Notes |
|-----------|-------------|--------|-------|
| **Cuboctahedron** | Archimedean solid, 14 faces | Not started | facet_shell with {100}+{111} |
| **Tetrahedral Frame** | Open tetrahedron structure | Not started | diff operations on tetrahedron |
| **Interlocking Pair** | Male+female that fit atomically | Not started | Requires precise offset calculation |
| **Ratchet Tooth** | Asymmetric sawtooth profile | Not started | Extruded asymmetric polygon |
| **Bearing Race** | Faceted "cylinder" approximation | Not started | High-N polygon prism, hollow |

---

## Reusable Custom Nodes (Library)

As we build components, we should extract reusable patterns as custom nodes.

### Planned Custom Nodes

| Node Name | Purpose | Parameters | Status |
|-----------|---------|------------|--------|
| `tetrahedron` | 4-faced solid from {111} planes | size, center | Not started |
| `octahedron` | 8-faced solid from {111} planes | size, center | Not started |
| `prism_n` | N-sided regular prism | n_sides, radius, height | Not started |
| `pyramid_n` | N-sided pyramid | n_sides, base_radius, height | Not started |
| `chamfered_cuboid` | Cuboid with cut corners | extent, chamfer | Not started |
| `hollow_prism` | Prismatic tube | n_sides, outer_r, inner_r, height | Not started |

### Built Custom Nodes

*None yet - to be populated as we create them*

---

## Design Constraints

### atomCAD Limitations to Remember
- **Coordinates are in lattice units** (not angstroms) - cubic diamond ≈ 3.567 Å per unit
- **facet_shell only works with cubic unit cells**
- **Single-digit nanometers** means roughly 1-25 lattice units (3.5-90 Å)
- **Large structures slow down** - keep designs compact
- **Sphere/circle are approximations** - prefer faceted geometry

### Crystallographic Considerations
- **{111} surfaces** - Most stable diamond surfaces, 3-fold symmetric
- **{100} surfaces** - Can reconstruct (2×1), 4-fold symmetric
- **{110} surfaces** - 2-fold symmetric, different bonding pattern
- **Miller indices in half_space** - Use to cut along specific crystal planes

### Physical Constraints
- **Minimum feature size** ~1-2 lattice units (one unit cell minimum)
- **Surface passivation** - use `passivate: true` for realistic hydrogen termination
- **Bond integrity** - `rm_single: true` removes unstable single-bonded atoms
- **Wall thickness** - Need ~2-3 lattice units minimum for structural integrity

---

## Session Log

### Session 2 - 2026-01-26
- Tested octahedron using 8 {111} half_spaces - works great! (deleted after testing)
- Found & fixed bug: custom nodes didn't render until save/reload
- **Fix:** Added `validate_network()` call in `ai_edit_network` ([ai_assistant_api.rs:160](rust/src/api/structure_designer/ai_assistant_api.rs#L160))
- **Next:** Build persistent primitives (octahedron, hexprism), test polygon+extrude

### Session 1 - 2026-01-26
- Created `ai_playground/` folder and this research.md
- Established Crystal-First Design Philosophy (half_space > polygon+extrude > cuboid; avoid sphere/circle)
- Documented Oh symmetry of cubic diamond for design leverage

---

## Bugs & Feature Requests

**Instructions:** When you encounter atomCAD bugs or missing features that block efficient work, document them here briefly. Include: what you tried, what failed, and any workaround found. This helps prioritize development and warns future sessions.

### Bugs Found

| Issue | Severity | Workaround | Reported |
|-------|----------|------------|----------|
| *None yet* | | | |

### Feature Requests

| Feature | Why Needed | Priority |
|---------|------------|----------|
| *None yet* | | |

---

## Ideas & Notes

### Crystallographic Design Ideas
- Use {111} half_spaces to create tetrahedral/octahedral shapes (natural diamond forms)
- Use {110} half_spaces for rhombic/dodecahedral features
- Combine {100} and {111} for truncated shapes (cuboctahedron)
- Interlocking parts should mate along high-symmetry planes

### Future Exploration
- Can we create interlocking parts that actually fit together atomically?
- What's the minimum wall thickness for structural integrity?
- How do different crystallographic orientations affect surface properties?
- Which facet combinations produce the cleanest atom_fill results?
- Can we exploit surface reconstruction for functional surfaces?

### Open Questions
- How many polygon sides approximate "round" acceptably? (6? 8? 12?)
- Best Miller indices for various cutting operations?
- How to calculate offsets for interlocking parts at atomic precision?
- Does facet_shell give better results than manual half_space intersection?

---

## File Index

| File | Description | Created |
|------|-------------|---------|
| `research.md` | This file - project documentation | 2026-01-26 |
