# 2D Geometry nodes

← Back to [Reference Guide hub](../../atomCAD_reference_guide.md)

These nodes output a 2D geometry which can be used later as an input to an extrude node to create 3d geometry.
Similarly to the 3D geometry nodes, positions and sizes are usually discrete integer numbers meant in crystal lattice coordinates.

## drawing_plane

2D geometry nodes are on the XY plane by default. However you can draw on any arbitrary plane by using a `drawing_plane` node and plugging its output into a 2D geometry node's `d_plane` input pin.

![](../../atomCAD_images/drawing_plane.png)

2D binary operations can be executed only on 2D shapes on the same drawing plane.

## rect

Outputs a rectangle with integer minimum corner coordinates and integer width and height.

![](../../atomCAD_images/rect_node.png)

![](../../atomCAD_images/rect_props.png)

![](../../atomCAD_images/rect_viewport.png)

## circle

Outputs a circle with integer center coordinates and integer radius.

![](../../atomCAD_images/circle_node.png)

![](../../atomCAD_images/circle_props.png)

![](../../atomCAD_images/circle_viewport.png)

## reg_poly

Outputs a regular polygon with integer radius. The number of sides is a property too.
Now that we have general polygon node this node is less used.

![](../../atomCAD_images/reg_poly_node.png)

![](../../atomCAD_images/reg_poly_props.png)

![](../../atomCAD_images/reg_poly_viewport.png)

## polygon

Outputs a general polygon with integer coordinate vertices. Both convex and concave polygons can be created with this node.
The vertices can be freely dragged.
You can create a new vertex by dragging an edge.
Delete a vertex by dragging it onto one of its neighbour.

![](../../atomCAD_images/polygon_node.png)

![](../../atomCAD_images/polygon_viewport.png)

## half_plane

Outputs a half plane.
You can manipulate the two integer coordinate vertices which define the boundary line of the half plane.
Both vertices are displayed as a triangle-based prism. The direction of the half plane is indicated by the direction of the triangle.

![](../../atomCAD_images/half_plane_node.png)

![](../../atomCAD_images/half_plane_props.png)

![](../../atomCAD_images/half_plane_viewport.png)

## union_2d

Computes the Boolean union of any number of 2D geometries. The `shapes` input accepts an array of `Geometry2D` values (array-typed input; you can connect multiple wires and they will be concatenated).

![](../../atomCAD_images/union_2d_node.png)

![](../../atomCAD_images/union_2d_viewport.png)

## intersect_2d

Computes the Boolean intersection of any number of 2D geometries. The `shapes` input pin accepts an array of `Geometry2D` values.

![](../../atomCAD_images/intersect_2d_node.png)

![](../../atomCAD_images/intersect_2d_viewport.png)

## diff_2d

Computes the Boolean difference of two 2D geometries.

![](../../atomCAD_images/diff_2d_node.png)

![](../../atomCAD_images/diff_2d_viewport.png)

We could have designed this node to have two single geometry inputs but for convenience reasons (to avoid needing to use too many nodes) both of its input pins accept geometry arrays and first a union operation is done on the individual input pins before the diff operation.
The node expression is the following:

```
diff_2d(base, sub) = diff_2d(union_2d(...each base input...), union_2d(...each sub input...))
```
