# Creating Medieval Diamond Nanostructure Castles

## Project Intent

This is an **experimental design project** exploring the creation of medieval castle parts and castles with atomCAD using cubic diamond lattices. The goal is to design parameterized components that can be combined to build castles.

This project serves multiple purposes:
1. **Discover viable component designs** - What shapes and structures work well at the nanoscale?
2. **Build a library of reusable nodes** - Create parameterized custom nodes that can be composed
3. **Learn atomCAD's capabilities and limits** - Document what works, what doesn't, and workarounds
4. **Iterate on designs** - Create v1, v2, v3... of components as we learn better approaches

---

## Crystal-First Design Philosophy

### Core Principle: Work WITH the Lattice, Not Against It

Diamond has a discrete crystalline structure. Designs that respect crystallographic planes and symmetries will produce cleaner, more stable atomic structures than those that fight the lattice.

### Preferred Building Blocks

Prefer built-in nodes like half_space, half_plane, polygon, cuboid, extrude, etc...

Use only sparingly:

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

---

## Rules for AI Agent Sessions

### Document Maintenance
**Keep this file concise!** AI agents have limited context windows.
- Session logs: 3-5 bullet points max per session, focus on outcomes not process
- Prefer tables and lists over prose
- Delete obsolete information rather than accumulating it

### Before Starting Work
1. **Read this file first** - Understand current state and what's been tried
2. **Check the Session Log** below for recent progress
3. The user will load in ai_playground/castles/project.cnnd: **Review existing node networks in it** to understand what's been built

### During Work
1. **Use the atomcad skill** - All node network creation should go through `atomcad-cli`
2. **Create custom nodes** - Prefer building reusable parameterized subnetworks over one-off designs. Explore the possibility of building assemblies using these custom nodes.
3. **Think crystallographically** - Use half_space, polygon+extrude, respect lattice symmetries
4. **Take screenshots** - Visual documentation helps future sessions understand designs
5. **Document learnings** - Add notes about what worked/didn't work

### After Work
1. **Update this file** - Add a session log entry summarizing what was done
2. **Update component status** - Mark components as designed/tested/refined
3. **Note any issues or ideas** - Help the next session continue productively
4. The user will save the progress into project.cnnd.



---

## Design Constraints

### atomCAD Limitations to Remember
- **Coordinates are in lattice units** (not angstroms) - cubic diamond ≈ 3.567 Å per unit
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

### 2026-01-29: Initial Building Blocks (v1)
- Created **`wall_segment`** custom node: parameterized rectangular wall (length, height, thickness)
- Created **`square_tower`** custom node: hollow square tower (outer_width, height, wall_thickness)
- Built **`simple_castle`** assembly: 4 corner towers + 4 connecting walls (~38,940 atoms)
- Learned: use `ivec3` to construct dynamic vectors (can't use parameters in vector literals)

## Component Library

All custom nodes output **Geometry**. Use `atom_fill` only in the final assembly.

| Component | Status | Parameters | Notes |
|-----------|--------|------------|-------|
| `wall_segment` | v1 | length, height, thickness | Basic cuboid, X-aligned |
| `square_tower` | v1 | outer_width, height, wall_thickness | Hollow via diff |
| `arrow_slit_wall` | v1 | length, height, thickness, slit_width, slit_height, slit_z | X-aligned wall with centered slit |
| `arrow_slit_wall_y` | v1 | length, height, thickness, slit_width, slit_height, slit_z | Y-aligned wall with centered slit |
| `simple_castle` | v1 | (uses above) | 4 towers + 4 walls with slits |

---

## Bugs & Feature Requests

**Instructions:** When you encounter atomCAD bugs or missing features that block efficient work, document them here briefly. Include: what you tried, what failed, and any workaround found. This helps prioritize development and warns future sessions.

### Bugs Found

| Bug | Description | Workaround |
|-----|-------------|------------|
| **Heredoc + comments** | Using `# comment` lines in heredoc input causes silent parse failure - subsequent nodes not created | Use `\n` escape sequences in `--code` flag, or add nodes incrementally |

### Feature Requests

| Feature | Why Needed | Priority |
|---------|------------|----------|
| **Wireable polygon vertices** | `vertices` is literal-only, can't compute vertices dynamically | Low (half_plane workaround exists) |


