## Features

### Basic features

- Add atom quickly
- Create bond
- Select atoms and bonds
- Delete selected
- Move selected
- Copy selected
- Rotate selected
- Change view (balls and sticks vs. balls only)
- Add part (from other file or built-in part library)
- Undo-redo
- Relaxation
- Satisfy with Hydrogen
- Measure distances
- Groups and scene tree

### Other features

- Add procedural part

- Smart-building features. Like auto-bond when moving a part closer to another part. Smart positioning. Simpler to build a lattice by hand this way for example.
- Historical edit features. For example go back in history and insert new operations there.
- Volume fill and boolean (diff, union, intersection with shapes). Non trivial: how to fill the volumes

## Architecture overview

Architectural elements:

- Kernel
- UI
- Renderer

### Kernel

- Written in Rust.
- Simple model representation (atoms, bonds and group tree)
- Stores edit history tree (saved with model to file)
- Unit testable
- Called Kernel for simplicity, but it is more and editor backend, as it also deals with tool state and selection state

More details:

[Kernel](../kernel/kernel.md)

### UI

- Made in Flutter
- Adaptive, selection-based action bar

More details:

[UI](../ui_design/ui_design.md)

### Renderer

- Only atoms, bonds and editor 'gizmos' need to be rendered, so the renderer do not need to be general purpose

- Must support multiple views (like balls and stick vs. balls only)
- Must Handle huge models in the long run (not in the short run though, so we can start with something simple)

More details:

[Renderer](../renderer/renderer.md)

