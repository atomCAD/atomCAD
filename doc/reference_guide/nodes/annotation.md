# Annotation nodes

← Back to [Reference Guide hub](../../atomCAD_reference_guide.md)

## comment

Adds text annotations to document your node network. Comment nodes do not have input or output pins and do not affect the evaluation of the network.

**Properties**

- `Label` — An optional title displayed in a yellow header bar.
- `Text` — The main comment text.

Comment nodes can be resized by dragging the handle in the bottom-right corner.

**Editing a note**

- **Double-click the note** to edit it directly on the canvas. The click lands
  where you aimed it: double-clicking the title bar edits the title,
  double-clicking the body edits the text, and the cursor is placed at the
  character you clicked. The title field is always shown while editing, so a
  note that has no title yet can be given one without leaving the canvas. The
  note's context menu has an *Edit note* item that does the same thing.
- **Tab** moves between the title and the text. **Enter** in the title finishes
  the edit; in the text it inserts a line break.
- **Click anywhere outside the note** to finish, or press **Escape** to discard
  the changes made since the edit began.
- The whole edit is a single undo step, whether it was made on the canvas or in
  the properties panel.

The `Label` and `Text` fields in the properties panel on the right still work
and stay in sync with in-place edits — the two are just different ways to reach
the same note. Note text is plain text; it is not formatted as Markdown.

## parameter

Defines an input parameter for a subnetwork. When placed inside a node network that is used as a custom node, each `parameter` node becomes an input pin on the resulting custom node. See the [Subnetworks](../node_networks.md#subnetworks) section for details and examples.

**Properties**

- `Name` — The parameter name (becomes the input pin label on the custom node).
- `Type` — The data type of the parameter.
- `Sort Order` — Determines the order of parameters on the custom node.
