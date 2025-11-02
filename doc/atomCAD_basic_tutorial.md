# atomCAD Basic Tutorial

I assume you downloaded atomCAD on one of the supported platforms (Windows, MacOS, Linux). If not, you can do so on the [project's Github page](../README.md).

## Navigating the viewport

When you open atomCAD you will see something like this:

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\atomcad_start.png)

The upper part of the screen with the trusty cube in it is called the **viewport**, while the lower part of the screen is called the **node network editor**.

When using the application you will navigate the viewport all the time. Practice it a bit to make sure you can effortlessly do it:

- Hover over the cube with the mouse, press  and hold the right mouse button **(RMB) and drag**: this is how you **orbit** with the camera.
- If you drag with the middle mouse button (**MMB**) instead, you will **pan** the camera.
- You can **zoom** in and out using the middle mouse wheel (**MMW**). 

>TIP: It matters where the mouse pointer is when starting these operations: that will be you **pivot point**. A new pivot point is registered when you hover over an atomic structure or geometry or hovering over the horizontal (XY) plane when starting these operations. You can turn on visualization of the pivot point in Edit/Preferences/Other Settings/Display camera pivot point. (A small red cube will be displayed at the pivot point).

## Working with the node network

Besides navigating the viewport another thing that you will do all the time is editing the node network.

Why a node network?

- If you used other CAD software you know that there are two main approaches to CAD modeling: direct modeling and **parametric modeling**. Direct modeling is good for rapid prototyping and parametric modeling is ideal for products that will undergo many iterations and require automated updates. The node network in atomCAD makes it not just fully parametric but also programmable. It has a bit steeper learning curve than a direct modeler but its power makes it worth it.
- If you are a programmer you will probably like the node network in atomCAD as it has nodes that make it a functional programming language. Each node is a function, and as functions are **composable** so are node networks: you can define a custom reusable node by implementing it as a node network. You do not need to have any programming experience to create a node network though: using the programming related nodes is optional.

Let's look at our current node network which displays our cube!

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\cuboid.png)

We have a single cuboid node. You can select it by left clicking on it. The properties of the node appear at the bottom right corner of the application window.

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\cuboid_props.png)

> TIP: notice that you can drag a node by dragging it with the left mouse button. You can pan the node network canvas by dragging it at any empty space with the middle mouse button.

## Node properties and the lattice 

Let's hover the mouse over the x coordinate of the *Extent* property and use the mouse wheel to increase it. The size of the cuboid changes real-time. You can click the field, type in an integer number, and the value will be submitted when you click out of the field or press enter. Try creating something like this:

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\updated_cuboid.png)

Let's play around with the *Min Corner* property too, and the cuboid moves around. Try to move it like this:

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\moved_cuboid.png)

Wait, aren't we supposed to edit crystal structures? And why integer numbers only?

In atomCAD you create crystal structures by cutting out parts of an infinite crystal. You have tools to create intricate geometries and then you can use the `atom_fill` node to convert them to atomic structures by doing the crystal cutting. Restricting these geometries to be constrained to the lattice makes it easier to create atomic structures that are physical (and, in the future, suitable for manufacturable workflows).

> By default you are working in the cubic diamond lattice. The grid of the XY plane always reflects the active lattice. See the documentation of the `unit_cell` node in the Reference Guide to learn about working with arbitrary unit cells. Please note that the cuboid node outputs a parallelepiped in a generic unit cell. 

## Constructive Solid Geometry (CSG) operations

We will fill our geometry with atoms soon, but let's make our geometry a bit more interesting.

Right clicking somewhere on the node network canvas the **Add Node** dialog comes up.

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\add_node.png)

Start typing in the filter field the text `cuboid`. select the `cuboid` from the filtered list and a new cuboid will be placed. Select it and play around with its properties so that it has some overlap with the other cuboid but none of them covers the other fully.

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\two_cuboids.png)

As the cuboids are overlapping perhaps you do not see clearly where your individual cuboid are exactly. There are multiple ways to make better sense of a situation like that:

- Left click on the eye icon on the individual cuboid nodes. This will toggle the visibility of the output of the node. When visibility is turned on the node contributes to the scene in the viewport, otherwise it doesn't.
- Choose the wireframe geometry visualization at the geometry visualization panel at the upper left part of the window.

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\wireframe_button.png)

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\wireframe.png)

Restore the visibility of the nodes and go back to solid geometry visualization.
Right click on the node network again and add a `diff`node.

Left click drag the output pin of the first `cuboid`: drag out a wire from it and release the left mouse button at the `base` input of the `diff` node. You connected the first `cuboid`'s output pin to the `base` input pin of the `diff` node.

Connect the output pin of the second `cuboid` to the `sub` input pin of the `diff` node.

You should see something like this:

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\diff.png)

## About node visibility

You may have noticed that when you select a node it automatically becomes visible and other nodes which provide their inputs become invisible. This is the behavior you want most of the time, but not always. At the upper left part of the window there are buttons to set the **node display policy**. The default mode is named 'Prefer Selected Nodes'. One other mode that you often want is the 'Manual' mode: In this case no matter what is selected a node will be visible or invisible only if you explicitly toggle its eye icon. You can read about the exact behavior of the node display policies in the Reference Guide.

## atom_fill node

Add an `atom_fill` node to your node network. Connect the output of the `diff` node into the `shape` input pin of the `atom_fill` node. If you select the `atom_fill` node you will se something like this:

![](C:\machine_phase_systems\flutter_cad\doc\tutorial_images\atom_fill.png)

## Saving your design and exporting atomic structures

In the file menu you will find options to save your design or load it back. atomCAD's own file format is called the .cnnd file format (abbreviation for Crystal Node Network Design). A .cnnd file contains one design which consists of possibly multiple node networks.

You can also export the currently visible atomic structures into .xyz or .mol formats.

## Where to go next

- Search for the `polygon` node and the `extrude` node in the reference guide: with these nodes you can create interesting geometries quickly. Also check out the `rect`, `union_2d`, `intersect_2d`, `diff_2d`, `half_plane` nodes: These are also helpful when creating 2D shapes. Always model as much as possible in 2D and use the extrude node: working in 2D is almost always easier and faster than working in 3D.
- For most geometries working in 2D and extruding will not be enough to reach to the final shape. Besides the CSG operations (`union`, `intersect`, `diff`) you will need to learn to use the `half_space` node. It has a bit steep learning curve but it is immensely powerful. As its name suggests it represents an infinite half space defined by the bounding plane and a direction (which side of the plane is filled). You could create any crystal geometry just by using `half_space` nodes and CSG nodes. For example it is easy to see that intersecting any geometry with a half space is equivalent with using the boundary plane of the half space as a cutter plane on the given geometry.
- Learn about the `lattice_move` and `lattice_rot` nodes to be able to transform your geometries with respect to lattice symmetries. 
- To work with other crystals than cubic diamond learn about the `unit_cell` and `motif` nodes. atomCAD is felxible enough that you can define your own unit cells and even your own motifs too.
- Eventually it is worth browsing through all the information in the reference guide. There are capabilities which we did not cover here at all like programming, creating subnetworks, etc...
- atomCAD is evolving rapidly. Check back often and see the new release notes for new capabilities.

