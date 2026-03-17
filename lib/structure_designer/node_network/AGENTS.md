# Node Network Editor - Agent Instructions

Interactive visual node graph editor widget. Handles rendering, interaction, and manipulation of the node DAG.

## Files

| File | Purpose |
|------|---------|
| `node_network.dart` | Main editor widget: pan/zoom, selection, wire dragging, keyboard shortcuts |
| `node_network_painter.dart` | Custom painter: grid, wires (Bezier curves), pin hit testing |
| `node_widget.dart` | Individual node rendering: pins, title, drag, context menu |
| `comment_node_widget.dart` | Special rendering for Comment nodes |
| `add_node_popup.dart` | Node type picker dialog with category filtering |

## Coordinate System

Two spaces:
- **Logical space:** Where node positions are stored (pan-invariant)
- **Screen space:** Rendered pixel coordinates
- Conversion: `screen = (logical + panOffset) * scale`

## Zoom Levels

Three discrete levels with different detail:
1. **Normal (1.0):** Full detail — pins, labels, subtitles
2. **Medium (0.6):** Simplified — title only, smaller pins
3. **Far (0.35):** Minimal — text only, no pins

## Interaction Model

- **Pan:** Middle mouse drag, or Shift+right-click drag
- **Zoom:** Mouse wheel (zoom-to-cursor)
- **Select node:** Click (Ctrl=toggle, Shift=add)
- **Rectangle select:** Click+drag on empty space
- **Wire creation:** Drag from pin → drop on compatible pin
- **Auto-connect:** Drop wire in empty space → opens `AddNodePopup` filtered by type
- **Keyboard:** Ctrl+C/X/V (copy/cut/paste), Del (delete), Ctrl+D (duplicate)

## Wire Rendering

Wires use cubic Bezier curves with data-type-based coloring:
- Selected wires get a glow effect
- Hit testing uses expanded area for easier clicking
- Pin positions calculated differently per zoom level

## Data Type Colors

| Type | Color Family |
|------|-------------|
| Bool/Int/Float | Warm orange |
| Vec2/Vec3/IVec | Cool blue |
| Geometry | Purple |
| Atomic | Green |
| UnitCell/Motif | Teal/cyan |
| Functions | Amber |

## Node Widget States

- **Active:** Thick border, full glow (0xFFD84315)
- **Selected:** Medium border, partial glow (0xFFE08000)
- **Error:** Red border with glow
- **Normal:** Blue border

## Constants (must match Rust `node_layout.rs`)

- `BASE_NODE_WIDTH = 160`
- `BASE_NODE_VERT_WIRE_OFFSET = 33`
- `BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM = 22`
