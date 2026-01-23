# Issue: Skill Documentation Incorrect Parameter Names for atom_trans

## Summary

The atomCAD skill documentation (`.claude/skills/atomcad/skill.md`) shows incorrect parameter names for the `atom_trans` node, causing connection warnings when following the documentation.

## Severity

**Low** - Documentation issue only. The correct parameters can be discovered via `describe atom_trans`.

## Location

File: `.claude/skills/atomcad/skill.md`

## Steps to Reproduce

```bash
# 1. Following the skill documentation pattern for atom_trans:
cd c:/machine_phase_systems/flutter_cad

# The skill doc implies usage like:
./atomcad-cli edit --code="moved = atom_trans { atoms: some_atoms, translate: (10.0, 0.0, 0.0) }"

# This produces warnings:
# {"success":true,...,"warnings":["Unknown property 'translate' on node type 'atom_trans'","Connection warning for moved.atoms: Parameter 'atoms' not found on node type 'atom_trans'"]}

# 2. Check actual parameter names
./atomcad-cli describe atom_trans
# Output:
# Node: atom_trans
# Category: AtomicStructure
# Description: The atom_trans node transforms atomic structures...
#
# Inputs:
#   molecule    : Atomic  [required, wire-only]
#   translation : Vec3    [default: (0.0, 0.0, 0.0)]
#   rotation    : Vec3    [default: (0.0, 0.0, 0.0)]

# 3. Correct usage:
./atomcad-cli edit --code="moved = atom_trans { molecule: some_atoms, translation: (10.0, 0.0, 0.0) }"
# This works without warnings
```

## Incorrect (Current Documentation)

The skill doc doesn't explicitly show atom_trans usage, but the pattern implies:
- Input parameter: `atoms`
- Property: `translate`

## Correct (Actual API)

- Input parameter: `molecule`
- Property: `translation`

## Suggested Fix

Update the skill documentation to use correct parameter names. If there's an example using atom_trans, it should be:

```bash
# Transform atomic structure
atoms = atom_fill { shape: geom, visible: false }
moved = atom_trans { molecule: atoms, translation: (10.0, 0.0, 0.0), visible: true }
```

## Related

The skill documentation encourages using `describe <node>` to discover parameters, which is good practice. However, having consistent documentation reduces friction for users.

## Files to Update

- `.claude/skills/atomcad/skill.md` - If atom_trans is mentioned, update parameter names
