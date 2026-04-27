# Reference Guide Documentation Plan (post v0.3.0)

## Purpose

Between the `v0.3.0` release tag (2026-03-23) and current `main`, 153 commits
have landed. Many of them introduce user-facing features, new nodes, renamed
concepts, or UI changes that are not yet reflected in
`doc/atomCAD_reference_guide.md`. This document enumerates the work needed to
bring the reference guide on `main` up to date.

The work is split into phases that can be picked up independently by separate
agents. Phases are ordered by dependency: later phases assume the terminology
introduced in earlier phases is already used in the document.

**Phase 0 (split the reference guide into multiple files) must be done first.**
All later phases assume the multi-file layout described there.

## How to use this plan

Each entry below names a feature, points to its design doc and representative
commits, and identifies the section(s) of the reference guide that need to be
created or updated. **An agent picking up a phase should:**

1. Read the corresponding design doc(s) listed for the feature.
2. Read the actual code in `rust/src/` and `lib/` to confirm the design was
   implemented as written (designs sometimes drift during implementation).
3. Cross-check against existing reference-guide style (terse, user-facing,
   with the same prose voice used in the existing sections — see
   `doc/atomCAD_reference_guide.md` for the conventions).
4. Update the reference guide on `main`. Do not retroactively edit the
   `v0.3.0-docs` tag.

Tick items off as completed by replacing `[ ]` with `[x]`.

## Out of scope for this plan

- Internal-only refactors that have no user-facing surface (e.g. `cnnd`
  v2→v3 migration internals, inline atom metadata storage refactor, the
  `array of HasAtoms → array of Molecule` refactor). These are listed in the
  appendix only for traceability.
- Bug fixes that do not change documented behavior (`migration bug fix`,
  `csg operations fix`, `inter cell bonding bug fix`, `output string bug fix`,
  `atom_fill migration fix`, `cnnd migration fix`, `motif_sub behaviour fix`,
  `downstream unfreezing bug`, `motif_edit phase 6 bug fixes`, etc.).
- **Producing screenshots/images.** Agents should mark where images are
  needed using the TODO convention below; the user will create the actual
  images.

---

## Image TODO convention (applies to all phases)

The reference guide uses screenshots in `doc/atomCAD_images/`. Agents
working on these phases must **not invent or fabricate image paths** —
instead, mark the exact spot where an image is needed using a placeholder
that is greppable, visible in rendering, and easy to replace later.

### Marker syntax

Place the marker on its own line at the position the image should appear:

```markdown
![TODO(image): short description of what the image should show](TODO)
```

This is a normal markdown image with `TODO` as both the destination and a
prefix in the alt text. Properties:

- **Visible in render**: GitHub renders it as a broken image with the
  description shown — impossible to miss when reviewing.
- **Greppable**: `grep -rn "](TODO)" doc/` lists every pending image
  across the docs.
- **Easy replacement**: when the user supplies the image, replacement is
  a single edit — change `TODO` to `../atomCAD_images/foo.png` (path depth
  per file location) and edit the alt text into a real caption.

The description should be concrete enough that the user knows what to
capture without re-reading the design doc — name the node, the panel,
the action, or the visual element (e.g. "the `motif_edit` node selected
with the edit-controls panel visible", not just "motif_edit screenshot").

### When to add an image marker

- **New nodes (Phase 2):** one image per node, typically the node selected
  in the network with its properties panel visible, or the node's output in
  the 3D viewport — whichever is more illustrative. Skip the image only for
  trivial nodes whose behavior is fully described by their pin types
  (e.g. `get_structure`, `with_structure` may not need one).
- **New UI features / tools (Phase 4):** one image showing the feature in
  use (e.g. rectangle selection grabbing bonds, the new parameter-override
  UI in `atom_fill`).
- **New conceptual features with visual representation:** one image where
  the concept is hard to convey in prose alone (e.g. abstract type pin
  coloring, blueprint alignment). Skip for pure data-type or terminology
  renames.
- **Text-format / CLI / `expr` content:** no images — code blocks suffice.

### When NOT to add an image marker

- A section is purely a terminology rename (e.g. Geometry → Blueprint).
- The same affordance is already shown by an existing image — reuse the
  existing image rather than requesting a new one.
- The change is a bug fix that doesn't alter the visible UI.
- The text already references an existing image that still applies.

### Budget guideline

For the post-v0.3.0 work as a whole, expect roughly **15–20 new images
total** (≈ 8 new nodes × 1 image each + a handful for UI/conceptual
changes). If an agent's draft would push a single phase past ~6 image
TODOs, that's a signal to look for prose-only descriptions or image reuse.

### After all docs are written

Before the user generates screenshots, summarize all pending image TODOs:

```bash
grep -rn "](TODO)" doc/ | sort
```

The user works through this list once at the end (or per phase, as
preferred), creating each image in `doc/atomCAD_images/` and replacing
the marker with a real path.

---

## Phase 0 — Split the reference guide into multiple files

The current `doc/atomCAD_reference_guide.md` is ~1370 lines and the post-v0.3.0
content will push it past 2500 lines. Split it into a hub page plus per-topic
files **before** any feature documentation work begins. Doing it first means:

- The Phase 1 terminology rename is much easier to do correctly across many
  small files than one huge one (easier to spot misses).
- New node entries from Phase 2 land directly in the right per-category file
  with no later restructuring.
- The `v0.3.0-docs` tag stays untouched — it captures the single-file state,
  which is correct for that release.

Phase 0 is split into two sub-phases (0a and 0b) so each can fit cleanly into
one agent session. Phase 0b assumes 0a is already complete and committed.

### Shared conventions (apply to both 0a and 0b)

#### Target layout

```
doc/
├── atomCAD_reference_guide.md          # Hub: title, intro, TOC, links to sub-pages
└── reference_guide/
    ├── direct_editing.md               # was: "Direct Editing Mode"
    ├── ui.md                           # was: "Parts of the UI"
    ├── node_networks.md                # was: "Node Networks" (concepts only)
    ├── nodes/
    │   ├── annotation.md               # comment, parameter
    │   ├── math_programming.md         # int, float, vec*, ivec*, bool, string,
    │   │                               #   expr, range, map
    │   ├── geometry_2d.md              # drawing_plane, rect, circle, reg_poly,
    │   │                               #   polygon, half_plane, union_2d,
    │   │                               #   intersect_2d, diff_2d
    │   ├── geometry_3d.md              # extrude, cuboid, sphere, half_space,
    │   │                               #   facet_shell, union, intersect, diff,
    │   │                               #   lattice_move, lattice_rot
    │   ├── atomic.md                   # import_xyz, export_xyz, atom_fill,
    │   │                               #   atom_move/rot/union/lmove/lrot,
    │   │                               #   apply_diff, relax, add/remove_hydrogen,
    │   │                               #   atom_cut, atom_edit
    │   └── other.md                    # unit_cell (→ lattice_vecs), motif, …
    ├── headless_cli.md                 # was: "Headless Mode (CLI)"
    └── claude_code.md                  # was: "Using with Claude Code"
```

Rationale: the per-category nodes split mirrors the existing H2/H3 structure
of the reference guide, so there is a clean 1-to-1 mapping. One-file-per-node
would create ~50 small files with poor overview; one-file-for-all-nodes is
what we are leaving behind.

#### Heading-demotion rule (applies to every extracted sub-page)

When moving content into a new file:

- A section that was `### Foo` in the old document with one `## Parent` above
  it becomes `## Foo` in the new file (since the new file has its own
  `# Title`). Sub-sections (`####`) become `###`, and so on.
- Each new file starts with a top-level `# Title` matching the section name.
- Add a one-line "← Back to [Reference Guide hub](../atomCAD_reference_guide.md)"
  link directly under the title. The relative path is
  `../atomCAD_reference_guide.md` from `reference_guide/*.md` and
  `../../atomCAD_reference_guide.md` from `reference_guide/nodes/*.md`.

#### Image paths

Images live at `doc/atomCAD_images/` and stay there. After moving sections
into sub-pages, rewrite image paths:

- From `reference_guide/*.md`: `./atomCAD_images/foo.png` → `../atomCAD_images/foo.png`.
- From `reference_guide/nodes/*.md`: `./atomCAD_images/foo.png` → `../../atomCAD_images/foo.png`.

After each phase, find image references with
`grep -rn "atomCAD_images" doc/reference_guide/` and verify the prefixes
are correct for each file's depth.

#### Anchor links and cross-references

The existing reference guide uses bare `#anchor` links for cross-references.
After the split:

- **Within a sub-page**: bare `#anchor` links continue to work — no change.
- **Across sub-pages**: rewrite as `relative_path.md#anchor`. For example,
  a reference from `direct_editing.md` to the `atom_edit` node entry becomes
  `./nodes/atomic.md#atom_edit`. From `nodes/atomic.md` to a section in
  `direct_editing.md`, use `../direct_editing.md#section-name`.
- **Sub-page → hub**: `../atomCAD_reference_guide.md#section-name`.

The full cross-reference rewrite is performed in Phase 0b once all sub-pages
exist. In Phase 0a, leave any cross-page `#anchor` links as-is — they will
be broken between 0a and 0b, but 0b's cross-reference pass fixes them.

#### Hub page (`atomCAD_reference_guide.md`) contents

The hub stays at the same path so the README pin and the `v0.3.0-docs` tag
both keep working unchanged. After the split the hub contains:

1. Title `# atomCAD Reference Guide`.
2. The existing **Introduction** prose (currently lines 5–30 of the source
   file). The intro is short and self-contained — keep it on the hub so a
   new reader gets oriented before hitting the TOC.
3. A **Contents** section that links to each sub-page with one short line
   describing what it covers. Example:

   ```markdown
   ## Contents
   - [Direct Editing Mode](./reference_guide/direct_editing.md) — the
     simplified beginner mode and the atom editor.
   - [Parts of the UI](./reference_guide/ui.md) — viewport, panels, menu
     bar, preferences.
   - [Node Networks](./reference_guide/node_networks.md) — core concepts:
     data types, subnetworks, functional programming.
   - **Nodes reference**
     - [Annotation nodes](./reference_guide/nodes/annotation.md)
     - [Math and programming nodes](./reference_guide/nodes/math_programming.md)
     - [2D geometry nodes](./reference_guide/nodes/geometry_2d.md)
     - [3D geometry nodes](./reference_guide/nodes/geometry_3d.md)
     - [Atomic structure nodes](./reference_guide/nodes/atomic.md)
     - [Other nodes](./reference_guide/nodes/other.md)
   - [Headless Mode (CLI)](./reference_guide/headless_cli.md)
   - [Using with Claude Code](./reference_guide/claude_code.md)
   ```

   The hub is built once in Phase 0a with the **full** TOC above, including
   links to the node sub-pages that 0b will create. Those links will resolve
   to 404 on github.com between 0a and 0b — that is expected and is fixed
   when 0b lands.

---

### Phase 0a — Setup, hub, and non-node sub-pages

**Scope:** create the directory structure, write the hub page with the full
TOC, and extract the 5 non-node H2 sections into sub-pages. This establishes
the file-structure pattern (heading demotion, hub link, image-path rewrite)
on smaller, simpler content before tackling the node-reference bulk in 0b.

**Files created in 0a (5 sub-pages + hub rewrite):**

| Source (current H2 in `doc/atomCAD_reference_guide.md`)            | Destination file                              |
|--------------------------------------------------------------------|-----------------------------------------------|
| `## Direct Editing Mode` (line ~32)                                | `reference_guide/direct_editing.md`           |
| `## Parts of the UI` (line ~154)                                   | `reference_guide/ui.md`                       |
| `## Node Networks` (line ~444, **concepts only — exclude "Nodes reference"**) | `reference_guide/node_networks.md` |
| `## Headless Mode (CLI)` (line ~1332)                              | `reference_guide/headless_cli.md`             |
| `## Using with Claude Code` (line ~1338)                           | `reference_guide/claude_code.md`              |
| (rewrite) `doc/atomCAD_reference_guide.md`                         | hub: title + intro + full TOC                 |

For "Node Networks" the agent must stop at the boundary where
`## Nodes reference` begins (around line 538) — that whole subtree belongs
to 0b. The current "Node Networks" H2 contains H3 subsections like
"Anatomy of a node", "Data types", "Node properties vs. input pins",
"Subnetworks", "Functional programming in atomCAD" — those go into
`node_networks.md`.

**Steps for Phase 0a:**

- [ ] Read the current `doc/atomCAD_reference_guide.md` from start to finish
      to internalize structure and prose voice.
- [ ] Create `doc/reference_guide/` and `doc/reference_guide/nodes/`
      directories (the latter is empty in 0a but is needed so the hub TOC's
      relative paths resolve later in 0b).
- [ ] Extract each of the 5 source sections into its destination sub-page:
      - Apply the heading-demotion rule.
      - Add the back-to-hub link directly under the title.
      - Rewrite image paths to use `../atomCAD_images/`.
      - Leave bare `#anchor` cross-references as-is for now.
- [ ] Replace `doc/atomCAD_reference_guide.md` with the new hub page
      (title, full intro prose copied verbatim, full Contents TOC).
- [ ] Run a content-loss check: `cat doc/reference_guide/direct_editing.md
      doc/reference_guide/ui.md doc/reference_guide/node_networks.md
      doc/reference_guide/headless_cli.md doc/reference_guide/claude_code.md`
      and confirm every paragraph from the corresponding original H2
      sections is present (modulo heading demotion and the back-to-hub
      link). The "Nodes reference" content under `## Node Networks` should
      NOT appear in any 0a output — it is reserved for 0b.
- [ ] `grep -rn "atomCAD_images" doc/reference_guide/` — every match should
      use `../atomCAD_images/` (no `./` or `../../` at this depth).
- [ ] Commit as one commit, e.g. `split reference guide phase 0a:
      hub + non-node sub-pages`.

**Verification checklist for Phase 0a:**

- [ ] Hub page renders on github.com with the intro and full TOC.
- [ ] Each of the 5 non-node sub-pages exists and renders.
- [ ] Each sub-page has a working back-to-hub link at the top.
- [ ] Every image in each sub-page renders.
- [ ] Hub TOC links to non-node sub-pages all resolve. Hub TOC links to
      `nodes/*.md` resolve to 404 — **expected, fixed by 0b**.
- [ ] Existing "Nodes reference" content is still present in
      `doc/atomCAD_reference_guide.md` only if 0a was implemented as a
      pure additive split. The recommended approach is to remove it from
      the hub (since the hub becomes intro+TOC only), letting it be
      re-introduced as `nodes/*.md` files in 0b. **Choose one approach
      and document it in the commit message** so 0b's agent knows whether
      to extract from the hub or from the pre-0a snapshot at HEAD~1.

---

### Phase 0b — Node category sub-pages and cross-reference cleanup

**Scope:** extract the 6 node-category H3 sections from the original
"Nodes reference" H2 into per-category sub-pages, then perform the
global cross-reference rewrite across all sub-pages and the repo-wide
reference update.

**Prerequisite:** Phase 0a is committed. The pre-0a state of the
reference guide is also saved verbatim at
`doc/_atomCAD_reference_guide_pre_phase_0a.md` (a leading-underscore
temporary file created during 0a). It is also recoverable via
`git show HEAD~1:doc/atomCAD_reference_guide.md` or
`git show v0.3.0-docs:doc/atomCAD_reference_guide.md`.

**Cleanup:** After Phase 0b is complete and verified, delete
`doc/_atomCAD_reference_guide_pre_phase_0a.md` — it is a temporary
hand-off file, not part of the published documentation.

**Files created in 0b (6 node sub-pages):**

| Source (current H3 inside `## Nodes reference`)            | Destination file                              |
|------------------------------------------------------------|-----------------------------------------------|
| `### Annotation nodes` (line ~551)                         | `reference_guide/nodes/annotation.md`         |
| `### Math and programming nodes` (~574)                    | `reference_guide/nodes/math_programming.md`   |
| `### 2D Geometry nodes` (~764)                             | `reference_guide/nodes/geometry_2d.md`        |
| `### 3D Geometry nodes` (~862)                             | `reference_guide/nodes/geometry_3d.md`        |
| `### Atomic structure nodes` (~1016)                       | `reference_guide/nodes/atomic.md`             |
| `### Other nodes` (~1188)                                  | `reference_guide/nodes/other.md`              |

For these files the heading-demotion rule still applies, but note that the
source H3 (`### Annotation nodes`) becomes the `# Annotation nodes` title
of the new file, the H4s under it (`#### comment`, `#### parameter`)
become `## comment`, `## parameter`, and so on.

**Steps for Phase 0b:**

- [ ] Confirm Phase 0a is committed: `git log --oneline -5` should show
      a phase 0a commit. The pre-split content (with the original
      single-file structure) is available at
      `doc/_atomCAD_reference_guide_pre_phase_0a.md`. If that file has
      been deleted, retrieve it via
      `git show HEAD~1:doc/atomCAD_reference_guide.md` or
      `git show v0.3.0-docs:doc/atomCAD_reference_guide.md`.
- [ ] Extract each of the 6 H3 sections into its destination sub-page:
      - Apply the heading-demotion rule.
      - Add the back-to-hub link directly under the title (using
        `../../atomCAD_reference_guide.md` since the file is one level
        deeper).
      - Rewrite image paths to use `../../atomCAD_images/`.
- [ ] Cross-reference rewrite pass across **all** sub-pages (both 0a and
      0b output):
      - `grep -rn "](#" doc/reference_guide/` to find all bare anchor links.
      - For each match, determine which sub-page contains the target
        anchor. If the anchor is in the same file, leave it alone.
        Otherwise rewrite to `relative_path.md#anchor`.
      - Repeat with `grep -rn "atomCAD_reference_guide.md#"
        doc/reference_guide/` to catch any references that already use
        the old single-file path with a deep anchor.
- [ ] Repo-wide reference check:
      ```bash
      grep -rn "atomCAD_reference_guide" --include="*.md" --include="*.dart" --include="*.rs"
      ```
      Confirm each match is either pointing at the hub (no anchor — fine)
      or pointing at a sub-page (fix the path if it still uses the old
      single-file deep anchor). Known callsites: `README.md` (already
      pinned to `v0.3.0-docs` — no change), `doc/atomCAD_basic_tutorial.md`
      lines 126 and 130 (hub pointers — fine).
- [ ] Image-path verification: `grep -rn "atomCAD_images" doc/reference_guide/`
      — node sub-pages must use `../../atomCAD_images/`, top-level
      sub-pages must use `../atomCAD_images/`.
- [ ] Content-loss check: concatenate every sub-page (in TOC order),
      strip back-to-hub links and de-promote headings programmatically
      (or eyeball it), and diff against the original
      `git show v0.3.0-docs:doc/atomCAD_reference_guide.md`. There should
      be no missing paragraphs.
- [ ] Commit as one commit, e.g. `split reference guide phase 0b:
      node category sub-pages + cross-reference cleanup`.

**Verification checklist for Phase 0b:**

- [ ] All 6 node sub-pages exist and render on github.com.
- [ ] Each node sub-page has a working back-to-hub link at the top.
- [ ] Every image in every node sub-page renders.
- [ ] Hub TOC's links to `nodes/*.md` all resolve (no 404s).
- [ ] No bare `](#anchor)` links remain that target a section in a
      different sub-page (`grep -rn "](#" doc/reference_guide/` should
      show only same-file anchors).
- [ ] Repo-wide grep for `atomCAD_reference_guide` shows no broken
      deep-anchor links.
- [ ] The README link from `main` (`./doc/atomCAD_reference_guide.md`)
      still resolves to the hub. *(The tag-pinned README link from the
      earlier README work is `v0.3.0-docs`, which is single-file —
      correct, we don't change it.)*
- [ ] Concatenation diff against `v0.3.0-docs:doc/atomCAD_reference_guide.md`
      shows only structural changes (heading demotion, back-to-hub
      links, image-path prefixes), no content loss.

---

## Phase 1 — Foundational terminology and type system

This phase MUST be done first. Several large refactors renamed core concepts;
the rest of the reference guide should read coherently in the new terminology
before new nodes are added.

### 1.1 Lattice space refactoring (rename pass)
- [ ] Apply rename throughout the document.
- Design doc: `doc/design_lattice_space_refactoring.md`
- Representative commits: `9ebe36f4` (design), `6cce5588` (Geometry → Blueprint),
  `5b121eb6` (UnitCell → LatticeVecs), `5de95a86` (unit_cell → lattice_vecs),
  `7d8f7cf3` (Lattice → Structure in doc), `fa51dd00` (phases 4–5),
  `3e871b13`/`fa6fedd4`/`88f35dac`/`e7efd387` (phase 7a–d),
  `ae0500a3` (final merge).
- Renames to apply (verify against current code first):
  - Data type `Geometry` → `Blueprint`
  - Data type/concept `Lattice` → `Structure` (in user-facing prose where applicable)
  - Node `unit_cell` → `lattice_vecs` (section currently at line 1190)
  - Any references to "lattice space" terminology elsewhere
- Sections affected: "Data types" (~line 456), "3D Geometry nodes"
  (~line 862), `unit_cell`/`lattice_vecs` node entry, plus scattered prose.

### 1.2 Crystal / Molecule split (replaces single `Atomic` data type)
- [ ] Update the data-types section and every node entry whose pin types
      previously read `Atomic` to use `Crystal`, `Molecule`, or both.
- Design doc: `doc/design_crystal_molecule_split.md`
- Representative commits: `433890a9` (design), `5c4c4093`–`6bd724f0` (steps 1–7).
- User-facing changes to document: what each new type holds, when each is
  produced, how implicit conversions work between them, and how the type
  system treats them in the abstract-type system (see 1.3).
- Sections affected: "Data types" (~line 456), every atomic node entry in
  "Atomic structure nodes" (~line 1016).

### 1.3 Abstract types and pin coloring
- [ ] Document the abstract-type concept and the visual coloring rules.
- Representative commits: `acb10497` (abstract types rename),
  `eab55dd2` (abstract type pin coloring).
- No dedicated design doc — agent must read the code in
  `rust/src/structure_designer/` (look for `AbstractType` / type unification)
  and the pin-rendering code in `lib/structure_designer/`.
- Sections affected: "Data types" (~line 456) — likely a new subsection;
  possibly "Anatomy of a node" (~line 448) if pin coloring is described there.

### 1.4 Matrix types
- [ ] Document the matrix data type, its operations, and any new nodes
      introduced for matrix manipulation.
- Design doc: `doc/design_matrix_types.md`
- Representative commits: `6769d1c5` (design), `9fe77139`/`4a225305`/
  `19d64415`/`5821f806`/`5ecc4964` (phases 1–5).
- Sections affected: "Data types" (new subsection), and likely "Math and
  programming nodes" (~line 574) for any matrix-specific nodes.

### 1.5 Multi-output pins (semantic change)
- [ ] Document that nodes can have multiple output pins, how the per-pin
      eye icon works in the UI, and how pin names are shown.
- Design doc: `doc/design_multi_output_pins.md`
- Representative commits: `de41ad97`–`a6fd4b41` (phases 0–6).
- User-facing changes: per-pin display toggle, pin name labels in the node
  header, and the new text-format `.pinname` reference syntax.
- Sections affected: "Anatomy of a node" (~line 448), "Node Properties Panel"
  if relevant (~line 302), and the text format documentation if it lives in
  this file (search for "text format" / `.cnnd` text syntax).

---

## Phase 2 — New core nodes

After Phase 1, all new node entries can be written using the correct types.
Each item below adds a new node section under "Atomic structure nodes" or
"3D Geometry nodes" as appropriate. Match the formatting of existing node
entries (heading, one-line summary, input/output pins, properties, examples).

### 2.1 `structure` node + Structure data type
- [ ] Add node entry; describe what `Structure` carries.
- Representative commits: `7e5da12b` (structure node), `f28761e5` (structure
  node and Structure data type).
- Source: `rust/src/structure_designer/nodes/structure.rs` (verify path).

### 2.2 `get_structure` / `with_structure` nodes
- [ ] Add two node entries.
- Representative commits: `8e7cf275` (get_structure), `f2175ca3` (with_structure).

### 2.3 `supercell` node
- [ ] Add node entry. Note: structure input pin is optional (commit `d0334770`).
- Design doc: `doc/design_supercell_node.md`
- Representative commits: `a7751671` (design), `8309bb98` (refinements),
  `24ad4913`/`a9656c78`/`21d5af87` (phases 1–3).

### 2.4 `import_cif` node
- [ ] Add node entry; describe CIF import flow and any new UI affordances.
- Design doc: `doc/design_import_cif.md`
- Representative commits: `13f66ce3` (design), `78eea280`–`431e7dfb` (phases 0–8).
- Also update "File Formats" / import section (the reference guide lists `.cif`
  as a supported format — confirm the section exists and add the node link).

### 2.5 `infer_bonds` node
- [ ] Add node entry.
- Design doc: `doc/design_infer_bonds_node.md`
- Representative commit: `d888ddf6`.

### 2.6 `sequence` node
- [ ] Add node entry; describe what "sequence" means in context.
- Design doc: there was a `doc/design/sequence_node.md` referenced in the diff
  but it appears removed on `main` — check git history (`git log --all -- doc/design/sequence_node.md`) to find the right design source.
- Representative commits: `b7bbe378`/`bc18e07a` (design), `e8f04107`/`60e6d228`
  (phases 1–2).
- Also covers commit `3730fda2` "display array outputs" which is closely related.

### 2.7 `motif_edit` node
- [ ] Add node entry. This is a substantial feature; the section may need
      sub-headings for the editing operations it supports.
- Design doc: `doc/design_motif_edit.md` (very large — 1574 lines)
- Representative commits: `fe360075` (design), `b5cbeb0b`–`c48259f8` (phases 1–9).

### 2.8 `atom_composediff` node
- [ ] Add node entry; describe diff composition semantics.
- Design doc: `doc/design_compose_diffs.md` (very large — 1306 lines)
- Representative commits: `01acc28d` (design), `944c9cc3`/`02105394`/`0c401b0a`
  (phases 1–3).

---

## Phase 3 — Updates to existing nodes

These features extend nodes that already have an entry in the reference guide.
Find the existing section and amend it.

### 3.1 `atom_replace` extensions (phase 2)
- [ ] Update existing `atom_replace` section with new capabilities.
- Design doc: `doc/design_atom_replace.md`
- Representative commits: `290a1926`/`13177b1a` (phase 2).

### 3.2 `atom_edit` updates
- [ ] Document tolerance property + input pin (commit `cdfc28d4`).
- [ ] Document multi-output: `result` and `diff` pins (covered conceptually
      in 1.5; here add the specific node behavior).
- [ ] Document the "add atom at position" tool (commit `40daeb78`) — this
      goes under "Add atom tool" subsection (~line 92) of the Direct Editing
      Mode section, not the node section.
- Section: existing `atom_edit` entry (~line 1172) and Direct Editing Mode
  (~line 32 onwards).

### 3.3 `atom_fill` UX/behavior updates
- [ ] Document the improved parameter-override UI (commit `a232ee74`).
- [ ] Document "effective atomic number" (commit `ddf1410b`).
- [ ] Document parameter elements simulating their default replacements
      (commit `1de9f48b`).
- Section: existing `atom_fill` entry (~line 1036).

### 3.4 `apply_diff` related — hybridization display
- [ ] Document that inferred hybridization is shown when auto hybridization
      is in use (commit `73139119`), plus diff-view hybridization fix
      (`25d2c8fa`). May belong in atom_edit / direct editing mode rather
      than apply_diff — verify.

---

## Phase 4 — UI / UX additions outside individual nodes

These changes affect general application behavior, not a single node.

### 4.1 Selection and visualization
- [ ] Rectangle select now selects bonds too (commit `df08d6c0`). Update
      Direct Editing Mode (~line 32) — selection behavior.
- [ ] Better bond colors, distinct from selection color (commit `5a5ab52f`).
      Update Atomic Structure Visualization (~line 385) or Direct Editing.
- [ ] Toggle geometry display on crystals and molecules (commit `4b125fca`).
      Update Geometry Visualization (~line 376) or Display Preferences (~line 319).

### 4.2 Phase transitions and movement
- [ ] Document the phase-transition feature.
- Design doc: `doc/design_phase_transitions_and_movement.md`
- Representative commits: `3e871b13`/`fa6fedd4`/`88f35dac`/`e7efd387`
  (phase 7a–d of lattice space refactoring).
- Likely belongs in Direct Editing Mode or a new top-level section; agent
  should decide based on the design doc.

### 4.3 Blueprint alignment
- [ ] Document the alignment feature. May add nodes, UI, or both —
      verify against the design doc and code.
- Design doc: `doc/design_blueprint_alignment.md`
- Representative commits: `0953b7c6` (design), `25ac62de`/`1d6d47d6`/
  `d61260a9`/`3bdd0b82` (phases 1–3).

### 4.4 `SameAsInput` UI
- [ ] Document the SameAsInput pin/type behavior in the UI.
- Representative commit: `1520b115`.
- No design doc — read code (search for `SameAsInput` in
  `rust/src/structure_designer/`).
- Section: likely "Anatomy of a node" or "Data types".

---

## Phase 5 — Text format and CLI surface

### 5.1 Multi-output pin reference syntax in text format
- [ ] Document the `.pinname` syntax for referring to non-default output
      pins in the text format (e.g. `atom_edit.diff`).
- Design doc: `doc/design_multi_output_pins.md` (Phase 5 section)
- Representative commit: included in `a6fd4b41` (multi output pins phase 5, 6).
- Section: search the reference guide for the text-format section.
  If text-format syntax is documented elsewhere (e.g.
  `doc/node_network_text_format.md`), update there instead and add a
  cross-reference from the reference guide.

---

## Appendix — Internal-only changes (no documentation needed)

Listed for traceability so future agents do not waste time on them:

- `cnnd` v2→v3 migration: `c2c228f2`, `0091edba`, `90ab8843`, `76771299`,
  `6743a73b`, `64e85188`, `7c23de08`, `05e4201b`, `9e1a0d38`, `b4e929e5`,
  `dfd708c2`, `e60ca8ab`. File-format internal; users see the migration
  happen transparently on load.
- Inline atom metadata refactor: `fd7eedf9`, `681e5aba`, `46d203c3`,
  `ce7d4a87`, `12f46327`, `e1544113`, `0951ad04`, `829c47e2`. Internal
  storage change; no user-facing surface.
- `array of HasAtoms → array of Molecule`: `66bed793`. Internal.
- `atom_union design refinements`: `058258a3`. Design-only refinement;
  if `atom_union` was already documented at v0.3.0, no update needed.
- `motif debug info`: `abff27f4`. Developer-facing only; verify before
  skipping.
- Bug fixes (no documented-behavior change): `23ee5af4`, `0a3c69af`,
  `66653777`, `13cfdbc0`, `dee4df35`, `0605898f`, `5f4da2ab`, `3ac27670`,
  `608637d0`, `6e8eb326`, `20a1d2b1`.
- Reference-guide work already merged to main pre-v0.3.0-docs tag:
  `d466446b`, `4a3386d6`, `09b88def`, `6ca2b5f7`, `08455ccb`, `4c6d2cf9`,
  `50a3d71d`, `08e5e3ce`. Captured by the `v0.3.0-docs` tag.

## Verification before each phase ships

After completing a phase, the agent should:

1. Re-read the modified sections end-to-end for prose voice consistency.
2. `flutter run` (or load a sample `.cnnd`) and verify any UI claims made in
   the new prose actually match what the user sees.
3. For new node entries, confirm the input/output pin lists match the actual
   node definitions in `rust/src/structure_designer/nodes/`.
4. If the section refers to images, place them under `doc/atomCAD_images/`
   and use relative links (so the relative-link cascade described in the
   README workflow continues to work).
