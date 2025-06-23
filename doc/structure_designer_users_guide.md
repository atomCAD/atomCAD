# atomCAD Structure Designer User's Guide

## Introduction

The Structure Designer is a tool for creating diamond crystal structures with defects.
It support non-destructive editing through the use of a node network.

## Notations in this document

Instead of the usual TODO notation we use TODOC and TODEV notation in this document:

- TODEV: means that the feature mentioned needs to be developd and documented
- TODOC: means that something needs to be documented but is already developed in atomCAD

## Basic Tutorial

See [Structure Designer Tutorial](./structure_designer_tutorial.md)

I recommend first reading the tutorial before reading this document which is more of a reference.

## Parts of the UI

This is how the full window looks like:
![](./structure_designer_images/full_window.png)

---

- Menu Bar: for loading and saving a design and for opening the preferences panel:
![](./structure_designer_images/menu_bar.png)

---

- 3D Viewport: you can see the node network results here. You can rotate, zoom and pan the camera in the viewport:
![](./structure_designer_images/3d_viewport.png)

---

- Node Network Editor Panel: you can edit the active node network here:
![](./structure_designer_images/node_network_editor_panel.png)

---

- Node Properties Panel: you can edit the properties of the active node here:
![](./structure_designer_images/cuboid_properties_panel.png)

---

- Node Networks List Panel: you can select the active node network here:
![](./structure_designer_images/node_networks_list_panel.png)

---

- Geometry Visualization Preferences Panel: common settings for geometry visualization:
![](./structure_designer_images/geometry_visualization_preferences_panel.png)

---

- Camera Control Panel: common settings for the camera:
![](./structure_designer_images/camera_control_panel.png)

---

## 3D Viewport

You can navigate in the viewport the following way:

- Move: drag with middle mouse button
- Rotate: drag with right mouse button
- Zoom: use mouse scroll wheel

## Node network composability and Node networks list panel

A node network consist of nodes. A node can be a built-in node or a custom node.
You can create a custom node by creating a node network with the same name which will be the implementation of the custom node.
Node networks are composable this way like functions in a programming language. When you use node B in node network A then node network B acts as a subnetwork of node network A.
As nodes can have parameters and outputs you will see how these can be set-up in a subnetwork later in thi document.

A structure design consist of node networks. The list of these node networks is displayed in the Node networks list panel. You can select one to edit in the node network editor panel. You can create a new node network in your design by clicking on the ''

## Node network editor panel

A new node can be added to the network by clickingthe mouse right button: it will open the Add Node Menu.
You can drag nodes around using mouse left-click and drag. You can connect pins of nodes by left-click dragging a pin to another pin.
Currently you can select one node or wire by lef-clicking it. Selecting a node or wire has the following uses:

- You can delete the selected node or wire by pressing the del key on the keyboard
- You can edit the properties of the selected node in the node properties panel
- When a node is selected you may be able to interact with the model in the viewport: what kind of interations these are depend on the node type and we will discuss them in the nodes reference section. Most of the time this involves one or more interactive *gadgets* appearing in the viewport.

Selecting a node is not the same as making its output visible in the viewport.
Node visibility is controlled by toggling the eye icon on the upper right corner of the node.

TODEV: being able to select and drag multiple nodes should be possible.

TODEV: selection and visibility can be orthogonally controlled, which is powerful but we need a convenient mode too in which selection and visibility are linked and is more intuitive most of the time.

## Input and output pin color codes

The color of a pin represents its data type. A wire can go only from an output pin to an input pin and they must have the same color. The following colors are used:

- Blue: 3D geometry
- Purple: 2D geometry
- Green: Atomic structure

You create 2D geometry to eventually use the *extrude* node to create 3D geometry from it. You create 3D geometry to eventually use the *geo_to_atom* node to create an atomic structure from it.

## Nodes reference

### 2D Geometry editing nodes

TOWRITE

### 3D Geometry editing nodes

TOWRITE

### Atomic structure related nodes

TOWRITE

