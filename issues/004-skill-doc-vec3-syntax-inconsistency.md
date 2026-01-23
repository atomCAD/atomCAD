# Issue: Skill Documentation Shows Incorrect vec3/ivec3 Syntax

## Summary

The atomCAD skill documentation shows vector values using tuple syntax `(x, y, z)` for vec3/ivec3 nodes, but the actual nodes require separate `x`, `y`, `z` parameters.

## Severity

**Low** - Documentation issue only. Causes initial confusion but `describe` reveals correct syntax.

## Location

File: `.claude/skills/atomcad/skill.md`

## Steps to Reproduce

```bash
cd c:/machine_phase_systems/flutter_cad

# 1. Try creating vec3 with tuple syntax (as documentation might suggest)
./atomcad-cli edit --code="pos1 = vec3 { value: (1.5, 2.5, 3.5) }"
# Output:
# {"success":true,"nodes_created":["pos1"],...,"warnings":["Unknown property 'value' on node type 'vec3'"]}

# 2. Check actual syntax
./atomcad-cli describe vec3
# Output:
# Node: vec3
# Category: MathAndProgramming
# Description: Outputs an Vec3 value.
#
# Inputs:
#   x : Float  [default: 0.0]
#   y : Float  [default: 0.0]
#   z : Float  [default: 0.0]
#
# Output: Vec3

# 3. Correct syntax
./atomcad-cli edit --code="pos2 = vec3 { x: 1.5, y: 2.5, z: 3.5 }"
# Works correctly

./atomcad-cli evaluate pos2
# Output: (1.500000, 2.500000, 3.500000)
```

## The Confusion

There's a conceptual difference between:
1. **Literal vector values** in parameters (e.g., `center: (5, 5, 5)` on sphere) - uses tuple syntax
2. **Vector constructor nodes** (vec3, ivec3, vec2, ivec2) - use separate x, y, z inputs

The skill documentation doesn't clearly distinguish these two cases.

## Clarification Needed in Documentation

### Tuple syntax works for:
- Built-in node parameters that accept vectors:
  ```
  sphere { center: (5, 5, 5), radius: 3 }
  cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }
  ```

### Separate x/y/z parameters for:
- Vector constructor nodes:
  ```
  vec3 { x: 1.5, y: 2.5, z: 3.5 }
  ivec3 { x: 1, y: 2, z: 3 }
  vec2 { x: 1.0, y: 2.0 }
  ivec2 { x: 1, y: 2 }
  ```

## Suggested Documentation Update

Add a note in the skill documentation under "Data Types" or a new "Syntax Notes" section:

```markdown
### Vector Syntax

**Literal vectors** (for node parameters): Use tuple syntax
- `sphere { center: (5, 5, 5) }`
- `cuboid { extent: (10, 10, 10) }`

**Vector constructor nodes** (vec2, vec3, ivec2, ivec3): Use separate component inputs
- `vec3 { x: 1.5, y: 2.5, z: 3.5 }` - Creates a Vec3 output
- `ivec3 { x: 1, y: 2, z: 3 }` - Creates an IVec3 output

The tuple syntax `(x, y, z)` cannot be used with `value:` on vector nodes.
```

## Files to Update

- `.claude/skills/atomcad/skill.md`
