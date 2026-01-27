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
| **Octahedron** | 8-faced shape from {111} planes | **Done** | Custom node created, 6415 atoms at size=8 |
| **Truncated Cube** | Cube with chamfered corners | Not started | cuboid + half_spaces cutting corners |
| **Hexagonal Prism** | 6-sided column | **Done** | Custom node created, ~20k atoms at radius=8, height=12 |
| **Wedge** | Triangular prism | **Done** | half_plane + extrude, parametric size/height |
| **L-Bracket** | Corner structural element | **Done** | union of cuboids, parametric length/width/thickness |
| **Channel/Groove** | U-shaped trough | **Done** | diff(cuboid, cuboid), parametric with wall thickness |

### Tier 2: Medium Complexity
Require multiple boolean ops or careful crystallographic alignment.

| Component | Description | Status | Notes |
|-----------|-------------|--------|-------|
| **Dovetail Joint (Male)** | Interlocking connector piece | Not started | Extruded trapezoid polygon |
| **Dovetail Joint (Female)** | Matching receptacle | Not started | Must fit male piece atomically |
| **Rhombic Prism** | Diamond-shaped cross-section | Not started | polygon(4) + extrude, {110} alignment |
| **Stepped Shaft** | Rod with diameter changes | Not started | Union of aligned prisms |
| **Slot/Keyway** | Rectangular channel for alignment | Not started | diff with extruded rect |
| **Pyramidal Tip** | 4-sided point | **Done** | 4 {101}-family half_spaces, apex at origin |
| **Rod Logic Knob** | Small protrusion on cylindrical rod | Not started | For mechanical computing gates |
| **Gear Tooth** | Single triangular/trapezoidal tooth profile | Not started | Basic building block for gears |
| **Hollow Hexprism** | Tube with hexagonal cross-section | Not started | Bearing race or channel housing |

### Tier 3: Advanced
Complex crystallographic shapes or multi-part assemblies.

| Component | Description | Status | Notes |
|-----------|-------------|--------|-------|
| **Cuboctahedron** | Archimedean solid, 14 faces | Not started | facet_shell with {100}+{111} |
| **Tetrahedral Frame** | Open tetrahedron structure | Not started | diff operations on tetrahedron |
| **Interlocking Pair** | Male+female that fit atomically | Not started | Requires precise offset calculation |
| **Ratchet Tooth** | Asymmetric sawtooth profile | Not started | Extruded asymmetric polygon |
| **Bearing Race** | Faceted "cylinder" approximation | Not started | High-N polygon prism, hollow |
| **Sleeve Bearing Pair** | Concentric shaft + sleeve | Not started | Test m-fold/n-fold symmetry for superlubricity |
| **Rod Logic Channel** | Hollow channel with sliding rod | Not started | Test clearance for mechanical logic |
| **Simple Planetary Gear** | Sun gear + planets in casing | Not started | Complex assembly, ~12 lattice units diameter |
| **Honeycomb Strut** | Lightweight structural frame element | Not started | For pressure vessel skeletons |
| **Vee-Notch Gear** | Gear tooth nestled in race notch | Not started | Drexler/Goddard nanoscale gear concept |

---

## Reusable Custom Nodes (Library)

As we build components, we should extract reusable patterns as custom nodes.

### Planned Custom Nodes

| Node Name | Purpose | Parameters | Status |
|-----------|---------|------------|--------|
| `tetrahedron` | 4-faced solid from {111} planes | size, center | **Done** (center not impl) |
| `octahedron` | 8-faced solid from {111} planes | size, center | **Done** (center not impl) |
| `prism_n` | N-sided regular prism | n_sides, radius, height | Blocked (reg_poly literal-only) |
| `pyramid_n` | N-sided pyramid | n_sides, base_radius, height | Not started |
| `chamfered_cuboid` | Cuboid with cut corners | extent, chamfer | Not started |
| `hollow_prism` | Prismatic tube | n_sides, outer_r, inner_r, height | Not started |

### Built Custom Nodes

| Node Name | Purpose | Parameters | Notes |
|-----------|---------|------------|-------|
| `octahedron` | 8-faced {111} solid | `size: Int` (default 10) | Works well, supports literal params |
| `hexprism` | 6-sided prism | `height: Int` (default 12) | radius fixed at 8, see reg_poly limitation |
| `tetrahedron` | 4-faced {111} solid | `size: Int` (default 10) | 4 half_spaces, fully parametric |
| `wedge` | Triangular prism | `size: Int`, `height: Int` | 3 half_planes + extrude, fully parametric |
| `l_bracket` | L-shaped corner bracket | `length`, `width`, `thickness: Int` | union of 2 cuboids via ivec3 wiring |
| `pyramid_tip` | 4-sided pyramid point | `size: Int` | 4 {101} half_spaces + base, apex at origin |
| `channel` | U-shaped trough | `length`, `width`, `height`, `wall: Int` | diff(cuboid, cuboid), uses expr for inner dims |

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

### Session 7 - 2026-01-27 (Research)
- **Literature review:** Drexler's *Nanosystems*, Freitas's *Nanomedicine*, Merkle's bearing papers
- Added "Nanomachine Design Research" section with component specifications from literature
- **Key findings:** Sleeve bearings use symmetry mismatch (m-fold/n-fold) for zero-wear rotation; rod logic ~16nm³/gate; planetary gears ~4.3nm diameter
- Added 8 new components to build lists: rod logic knob, gear tooth, hollow hexprism, sleeve bearing pair, rod logic channel, planetary gear, honeycomb strut, vee-notch gear
- **Scale insight:** Most literature designs are 3-15nm (8-42 lattice units) - achievable but at upper atomCAD limits

### Session 6 - 2026-01-26
- Created **l_bracket** custom node - union of 2 cuboids with `ivec3` wiring for parametric extents
- Created **pyramid_tip** custom node - 4 {101}-family half_spaces meeting at apex, single `size` param
- Created **channel** custom node - diff(outer, inner) cuboid using `expr` nodes for inner dimensions
- **Key technique:** Use `ivec3` nodes to wire parameters into `cuboid.extent` (tuple literals don't accept wires)
- **Key technique:** Use `expr` nodes for arithmetic on parameters (e.g., `width - 2*wall` for inner width)

### Session 5 - 2026-01-26
- Created **tetrahedron** custom node (4 {111} half_spaces) - fully parametric `size`
- Created **wedge** custom node using `half_plane` + `extrude` - fully parametric `size` and `height`
- **Key insight:** `half_plane` has wireable `m_index`/`shift`, workaround for literal-only `reg_poly`/`polygon`
- This technique enables parametric 2D shapes that can be extruded

### Session 4 - 2026-01-26
- **Fixed bug:** Custom node literal params now work (`octahedron { size: 5 }` works directly)
- Fix was in `node_networks_serialization.rs` - loaded networks now use `CustomNodeData`
- **Next:** Continue building primitives library (tetrahedron, wedge, etc.)

### Session 3 - 2026-01-26
- Created **octahedron** custom node (8 {111} half_spaces) - working, parametric `size`
- Created **hexprism** custom node (reg_poly + extrude) - working, parametric `height` only
- Found bugs: literal params ignored, half_plane m_index silent (both now fixed)
- Feature gap: reg_poly radius/num_sides are literal-only, limits parametric shapes

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

| Bug | Description | Workaround |
|-----|-------------|------------|
| **Heredoc + comments** | Using `# comment` lines in heredoc input causes silent parse failure - subsequent nodes not created | Use `\n` escape sequences in `--code` flag, or add nodes incrementally |

### Feature Requests

| Feature | Why Needed | Priority |
|---------|------------|----------|
| **Wireable reg_poly parameters** | `radius` and `num_sides` are literal-only, blocking parametric regular polygons | Low (half_plane workaround exists) |
| **Wireable polygon vertices** | `vertices` is literal-only, can't compute vertices dynamically | Low (half_plane workaround exists) |

---

## Ideas & Notes

### Crystallographic Design Ideas
- Use {111} half_spaces to create tetrahedral/octahedral shapes (natural diamond forms)
- Use {110} half_spaces for rhombic/dodecahedral features
- Combine {100} and {111} for truncated shapes (cuboctahedron)
- Interlocking parts should mate along high-symmetry planes

### Workaround: Parametric 2D Shapes via half_plane
Since `reg_poly` and `polygon` have literal-only parameters, use `half_plane` nodes instead:
- `half_plane` has wireable `m_index` (IVec2), `center`, and `shift`
- Create ivec2 nodes for Miller indices, wire to half_plane.m_index
- Intersect multiple half_planes with `intersect_2d` to create arbitrary polygons
- Then `extrude` the result for prismatic shapes
- Example: 3 half_planes → triangle → extrude → wedge (fully parametric)

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

## Nanomachine Design Research (Literature Review)

This section documents theoretical nanomachine designs from the molecular nanotechnology literature that could inform our atomCAD design work.

### Key Sources

| Source | Author(s) | Focus |
|--------|-----------|-------|
| *Nanosystems* (1992) | K. Eric Drexler | Comprehensive nanomechanical engineering textbook |
| *Nanomedicine Vol. I* (1999) | Robert A. Freitas Jr. | Medical nanorobot design specifications |
| Nanofactory Collaboration | Freitas, Merkle et al. | Diamond mechanosynthesis research |

### Theoretical Nanomachine Components

The following components have been designed computationally and could serve as design targets:

#### Bearings

| Design | Specifications | Key Features |
|--------|---------------|--------------|
| **Sleeve Bearing** (Merkle) | Two bent {111} diamond sheets as hoops | Inner hoop rotates inside outer; only C and H atoms |
| **Superlubricity Bearing** | m-fold shaft in n-fold sleeve | Energy barrier period = GCD(m,n)/mn; zero wear if atomically precise |

**Design insight:** Molecular bearings can "run dry" - atomically precise surfaces have zero wear if forces don't dislodge atoms. Symmetry mismatch between shaft and sleeve creates low-friction rotation.

#### Gears

| Design | Specifications | Notes |
|--------|---------------|-------|
| **Planetary Gear** (Drexler/Merkle) | 4.3nm × 4.4nm, 12 moving parts, ~51k daltons | Strained silicon shell, sulfur termination, 9 planet gears |
| **2nd Gen Planetary** | 4,235 atoms, 47.6 nm³ volume | More stable but still has slip at high frequencies |

**Design insight:** Nanoscale gears may look completely different from macroscale equivalents. Goddard proposed "Vee design" where gear tooth nestles in notch - impossible to assemble macroscopically but feasible at molecular scale.

#### Rod Logic (Mechanical Computing)

| Component | Specifications | Notes |
|-----------|---------------|-------|
| **Interlock Gate** | ~16 nm³/gate | Knobs on rods prevent/enable motion |
| **Register** | ~40 nm³/register, 0.1ns switching | 1nm wide rods |
| **Full Nanocomputer** | (400nm)³, 10⁶ gates, ~60nW | EMP-resistant, orders of magnitude lower power than CMOS |

**Design insight:** Rod logic uses the principle that two objects can't occupy the same space. Rods slide in channels within a diamond matrix; "knobs" on crossing rods create logic gates.

#### Pumps and Sorting Rotors

| Design | Specifications | Application |
|--------|---------------|-------------|
| **Molecular Sorting Rotor** | 3-stage assemblies | Respirocyte O₂/CO₂ pumping |
| **Simple Pump** (Drexler) | 6,165 atoms | Selective molecular transport |
| **Respirocyte Pumping Station** | 12 stations per device | Glucose-powered, reversible pumping |

**Design insight:** Sorting rotors have different binding site tips for different molecule types. Rotation moves bound molecules from low to high concentration against gradient.

#### Tooltips and Manipulators

| Design | Specifications | Notes |
|--------|---------------|-------|
| **DCB6Ge Tooltip** | First complete DMS tooltip | Successfully simulated C₂ dimer placement on C(110) |
| **Handle Structure** | 0.1-10 μm diamond rod/cone | Grippable by SPM tip or MEMS manipulator |

### Reference Designs: Respirocyte (Freitas, 1998)

The respirocyte is the most detailed theoretical medical nanorobot design, providing concrete specifications:

- **Size:** 1 μm diameter spherical
- **Structure:** 18 billion atoms, diamondoid 1000 atm pressure vessel
- **Capacity:** 3 billion O₂/CO₂ molecules
- **Power:** Glucose metabolism, chemomechanical turbine
- **Components:**
  - 12 equatorial pumping stations (50% of surface)
  - Molecular sorting rotor arrays (3-stage)
  - Onboard nanocomputer with chemical/pressure sensors
  - Glucose tanks and powerplants
  - Surface "barcode" identification patterns

### Component Mapping to atomCAD

Based on the literature, here are components we could attempt in atomCAD at appropriate scales:

| Literature Component | atomCAD Approach | Complexity | Notes |
|---------------------|------------------|------------|-------|
| Sleeve bearing inner/outer | Two concentric hex/oct prisms | Medium | Test superlubricity symmetry matching |
| Rod logic knob | Small protrusion on shaft | Low | Simple diff/union geometry |
| Rod logic channel | Hollow prism with rod inside | Medium | Test clearance tolerances |
| Gear tooth profile | Asymmetric extruded polygon | Medium | Involute vs simple triangular |
| Ratchet pawl | Asymmetric sawtooth | Medium | Already in Tier 3 list |
| Pyramidal tooltip base | pyramid_tip variant | Low | Already built |
| Sorting rotor binding pocket | Hemispherical cavity in rotor | High | Requires precise geometry |
| Honeycomb/geodesic frame | Intersecting struts | High | For lightweight pressure vessels |

### Crystallographic Considerations from Literature

The literature emphasizes specific surface properties:

| Surface | Properties | Applications |
|---------|------------|--------------|
| **(111)** | Most stable, 3-fold symmetric, lowest surface energy | Bearing surfaces, natural cleave planes |
| **(100)** | 4-fold symmetric, can reconstruct (2×1), highest tensile strength | Structural members along cube axes |
| **(110)** | 2-fold symmetric, better doping efficiency, higher C-H density | Electronic applications, alternative bearing surfaces |

**Key finding:** Tensile strength is highest for [100] direction, lower for [110], lowest for [111]. Design load-bearing members along [100] axes.

### Practical Scale Constraints

| Metric | Value | Notes |
|--------|-------|-------|
| Diamond lattice constant | 3.567 Å | 1 lattice unit in atomCAD |
| Planetary gear diameter | 4.3 nm | ~12 lattice units |
| Rod logic gate | ~2.5 nm | ~7 lattice units |
| Minimum viable bearing | ~2-3 nm | ~6-8 lattice units |
| Respirocyte | 1000 nm | Way beyond atomCAD practical range |

**Implication:** Most literature designs are 3-15nm scale, which maps to 8-42 lattice units - achievable in atomCAD but at the upper end of practical rendering.

---

## File Index

| File | Description | Created |
|------|-------------|---------|
| `research.md` | This file - project documentation | 2026-01-26 |
| `l_bracket.png` | Screenshot of l_bracket custom node | 2026-01-26 |
| `pyramid_tip.png` | Screenshot of pyramid_tip (flat angle) | 2026-01-26 |
| `pyramid_tip3.png` | Screenshot of pyramid_tip (better angle) | 2026-01-26 |
| `channel.png` | Screenshot of channel custom node | 2026-01-26 |
