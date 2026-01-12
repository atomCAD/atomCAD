# Comment Node - UX Design Document

*Design document for adding comment/annotation nodes to atomCAD's node network editor.*

## Overview

Comment nodes are resizable text boxes that allow users to document and explain parts of their node network. They are **not functional nodes** — they have no pins, do not participate in evaluation, and exist purely for annotation purposes.

## Goals

- Allow users to add explanatory text at specific positions in the node network
- Keep implementation simple and maintainable
- Follow existing UI patterns and conventions

## Non-Goals (v1)

- Inline text editing on the canvas
- Rich text formatting or markdown support
- Grouping/framing multiple nodes visually

## Visual Design

### Appearance

- **Shape:** Resizable rectangle
- **Background:** Muted/pastel color (e.g., semi-transparent yellow or gray) to distinguish from functional nodes
- **Border:** Dashed border to reinforce that this is an annotation, not a functional node
- **No pins:** No input or output pins
- **No eye icon:** Comments are not "visible" in the 3D viewport
- **Title bar (optional):** Small header showing "Comment" or a user-defined label

### Example Mockup

```
┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
╎ Comment                    ╎
╎───────────────────────────╎
╎ This section handles the   ╎
╎ unit cell transformation   ╎
╎ before atom filling.       ╎
╎                       ◢    ╎
└╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘
        ◢ = resize handle
```

## Interaction Model

### Adding a Comment

1. Right-click on empty space in the node network editor → Add Node dialog opens
2. A new **"Annotation"** category appears **first** in the category list
3. Select "Comment" from the Annotation category
4. Comment node is placed at the click position with default size

**Rationale:** Using the existing Add Node dialog keeps the UI consistent and makes comments discoverable through the filter field.

### Moving

- **Left-click and drag** on the comment node to move it (same as regular nodes)

### Resizing

- **Corner drag handles** allow resizing the comment box
- Minimum size constraint prevents comments from becoming too small to read

### Selecting

- **Left-click** to select the comment node
- Selected comment shows resize handles and can be deleted with `Del` key
- Selection follows existing node selection behavior

### Editing Text

- **Edit text in the Node Properties Panel** (right side panel)
- When a comment node is selected, the properties panel shows:
  - A "Label" text field (optional short title, shown in header)
  - A "Text" multi-line text area (main content)

**Rationale:** Editing in the properties panel:
- Follows the existing pattern for all node types
- Avoids complexity of canvas-based text editing (cursor management, keyboard focus conflicts)
- Keeps undo/redo consistent with other property edits

### Deleting

- Select the comment node and press `Del` (same as regular nodes)

## Text Content

### v1: Plain Text Only

- No markdown rendering
- No rich text formatting
- Text wraps within the comment box boundaries

**Rationale:** Plain text is simple to implement and covers the primary use case. Formatting can be added in a future version if users request it.

## Serialization (.cnnd files)

Comment nodes must be saved and loaded as part of the node network:
- Stored in the `.cnnd` file alongside regular nodes
- Properties: position (x, y), size (width, height), label, text content
- Comments have a unique node ID like regular nodes

## Category Placement in Add Node Dialog

| Category | Description |
|----------|-------------|
| **Annotation** *(new, first)* | Comment |
| Math and Programming | Existing nodes... |
| 2D Geometry | Existing nodes... |
| 3D Geometry | Existing nodes... |
| Atomic Structure | Existing nodes... |
| Other | Existing nodes... |
| Custom | User-defined networks |

The "Annotation" category appears first to make it easily discoverable.

## Future Considerations (out of scope for v1)

- **Inline editing:** Allow clicking on comment text to edit directly on canvas
- **Markdown support:** Render basic markdown (bold, italic, lists)
- **Frame nodes:** Resizable frames that visually group multiple nodes
- **Color customization:** Let users choose comment background color
- **Collapse/expand:** Minimize comments to just show the label

## Open Questions (Resolved)

| Question | Decision |
|----------|----------|
| Inline editing vs. property panel? | Property panel (simpler, consistent) |
| Markdown support? | No (plain text for v1) |
| Where in Add Node dialog? | New "Annotation" category at top |
| Should comments have a label/title? | Yes, optional label shown in header |

---

*Document created: Comment Node UX Design*
*Status: Approved for implementation*
