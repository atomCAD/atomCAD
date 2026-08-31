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

**Anchoring a note to what it documents**

A note can be *anchored* to a wire or to a node. The association is drawn as a
dashed grey leader line from the note's edge to its subject, and — unlike mere
proximity on the canvas — it is stored with the design, so it survives moving
things around, automatic layout, saving and reloading.

- **To anchor:** drag the small handle in the note's **bottom-left** corner (the
  link icon, opposite the resize handle) and drop it on a wire or on a node.
  Dropping on empty space changes nothing. A note can only be anchored to a
  wire or node in the same place it lives itself — a note inside a
  higher-order function's body anchors to things in that body, not outside it.
- **To re-aim:** drag the handle onto a different target. The previous anchor
  is replaced.
- **To remove:** **click** the handle — once a note is anchored the handle turns
  orange and its icon becomes a broken link, and a plain click detaches it.
  Right-click → **Remove anchor** does the same thing.

Releasing an anchoring drag over empty space leaves the note as it was; it is a
cancel, not a detach. (Wires are thin targets, so missing one is easy — the note
keeps its existing anchor and you can simply try again.)

The leader line ends at the midpoint of an anchored wire, or at the border of
an anchored node. Anchoring is undoable (`Ctrl+Z`) and has no effect whatsoever
on evaluation — an anchor is documentation, never a connection, and it never
makes its target count as "used".

If the anchored wire is disconnected, or the anchored node deleted, the anchor
is dropped and the leader line disappears; it is never silently re-pointed at
something else. Undoing the deletion brings the anchor back.

**Notes and automatic layout**

*Edit > Auto-Layout Network* rearranges the graph, and notes are placed against
it afterwards rather than being treated as nodes in it:

- An **anchored** note is placed next to whatever its anchor points at — beside
  the node, or beside the wire's midpoint. When there is no room there (a
  laid-out graph has only about 50 px between columns and 30 px between nodes,
  far less than a note needs), it goes to the nearest margin above or below the
  drawing, lined up with its subject, with the leader line pointing back in.
  That is deliberate: annotations in the margin with leaders into the drawing is
  the normal drafting arrangement.
- An **unanchored** note keeps its position relative to the drawing's top-left
  corner, so a note stays with its graph even when the whole graph is moved to
  the canvas origin. If something else has since been placed there, it falls to
  the margin like any other note.

Notes are always placed at their real size, they never push graph nodes out of
the way, and they never overlap a node or each other. A note anchored deep
inside a wide graph can end up with a long leader line to the margin; anchor it
and it will at least point at the right thing.

Anchors can also be written by hand (or by an AI assistant) in the network text
editor via the comment's `on:` property — see *Comment anchors* in
[`doc/node_network_text_format.md`](../../node_network_text_format.md). A note
authored that way may carry several anchors and draws one leader line per
anchor; the drag handle sets a single anchor.

## parameter

Defines an input parameter for a subnetwork. When placed inside a node network that is used as a custom node, each `parameter` node becomes an input pin on the resulting custom node. See the [Subnetworks](../node_networks.md#subnetworks) section for details and examples.

**Properties**

- `Name` — The parameter name (becomes the input pin label on the custom node).
- `Type` — The data type of the parameter.
- `Sort Order` — Determines the order of parameters on the custom node.
