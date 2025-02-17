# UI Design

- Made in Flutter
- Node Network-based
- Adaptive, selection-based action bar

TODO: some of this document were writteen before the introduction of the node network based UI, and needs to be reconciled with it. These are related to direct editing of atomic structures, which will be present, but in the context of the 'edit atomic structure' node.

## Camera movement

Camera movement operations are the following:

- Moving
- Rotating
- Zooming

There are parameters to all 3 operations. When moving the camera parallel with the camera plane, the question is how sensitive this movement should be to mouse movement or touch movement. When rotating, the question is what is the pivot point. When zooming, the question is what to zoom on. The answer to all of this is that we use the mouse position or touch position to determine a pivot point, and we can do the camera movement operation based on the pivot point. The pivot point is the center of the first atom hit by the ray cast based on mouse position. If there is no hit then it is the center of the atom which is closest to the ray. If the scene is empty then the pivot point is where the ray hits the base plane. If the ray do not hit the plane in a specific distance then the pivot point is the point on the ray with that distance.

The means of camera movement can be input device and platform dependent. We will implement it to be easily changeable.

I just describe some sensible default settings:

### Typical 3 button mouse with scroll wheel (typical on Windows)

- Move : drag with middle mouse button
- Rotate: drag with right mouse button
- Zoom: use mouse scroll wheel

### One mouse button with scroll wheel (magic mouse)

- Move : drag with mouse button + SHIFT
- Rotate: drag with mouse button + COMMAND
- Zoom: use mouse scroll wheel

### Touchscreen (mobile)

TODO

## UI Parts

There are the following panels on the UI:

- Menu bar
- 3D viewport
- Node networks list
- Node network editor
- Action icons panel
- Editor properties panel.  When no tool is active these are just some basic settings for editing molecular structures. When a tool is active this becomes a tool properties panel and displays the properties of the active tool.
- Item properties panel (atom properties or bond properties or group properties based on selection)

## Actions and tools

Available actions are represented by icons on the left part of the viewport.

The available actions presented to the user is adaptive, depends on the selection. For actions that are only available when something is selected I denoted with *(S)*.

Here is a list of actions:

- Add part (from XYZ file or internal library)
- Delete *(S)*
- Copy *(S)*
- Create group *(S)* (creates a group from the selection)

Most actions are just one-off actions that do something upon pressing the icon. For actions a modal configuration panel may be presented before performing actions. Some actions can be so-called tools. If an action is a tool, pressing its icon it is highlighted and the tool becomes the active tool. When in a tool, the viewport UI interaction is different that when in a no-tool state.  When getting out of a tool, the user gets back to this 'no active tool' state.

As a design philosophy we tend to make most things available in the default state of the application in a simple intuitive way and only use tool-states where the task is either rare or is complicated and would complicate the default behavior of the application.

## Default state (no tool selected)

Here we describe how the application works when no tool is selected. The common editing tasks can be achieved in this state. Later we will discuss the special behavior of the application in certain tools. 

### Selection

- Rectangular selection with left click and drag
- Left click to select individual atom or bond

Holding the shift key while doing either of the above you can add to the existing selection, holding the Ctrl key while doing either of the above you can invert the selection for the newly selected items.

### Adding atoms

You can add an atom the following ways:

- Left click somewhere
- Drag with left click on an existing atom and release the button somewhere. This way a bond is also created.

The atom type to be added can be set on the editor properties panel. 

### Adding bonds

You can do this by dragging from an atom with left click to another atom. A bond is also created when releasing at empty space as described above, but in this case an atom is also created.

The multiplicity of the bond can be set on the editor properties panel.

## Tools

No tool is planned at the moment, but down the line there will be some.
