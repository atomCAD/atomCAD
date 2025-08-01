It is not trivial how to apply transforms in the node network.

We have two transformation nodes: geo_trans and atom_trans.
Both have the following properties which are directly exposed on the UI:
- translation: world space translation vector
- rotation: euler angles in degrees applied in the order xyx on the local gadget axes.

The difference is only that geo_trans only supports rotations which are multiples of 90 degrees.

For geometry we have a simple guiding axiom: geometry is always rigidly attached to the transformation
gizmo. (Geometry never moves in the transformation gizmo's local coordinate system.)

The reason this is not trivial is that it is an arbitrary choice that we use worls space translation and also that we use euler angles around the local axes of the transformation gizmo.
It has a some advantages, but it also has some disadvantages, and implications on the UX.

Advantages:
- rotation is simple to reason about if interpreted as local rotations
- translation is more easy to reason about as world space translation

Disadvantages:
- In case of rotated axes when dragging a gizmo axis (e.g. dragging the x axis)
actually a different axis coordinate is changed in the node data (e.g. y coordinate is changed, as the local x axis is the global y axis)

Ultimately the main question is: what was the user's intent with that node?
Example: The user rotated a cuboid by 90 degrees around the x axis and translated it by 10 units in the y direction.
In our interpretation if the orientation changes because of a previous node change, the
rotation by 90 degrees around the x axis of the cuboid is still the intention,
while the translation by 10 units is still along the global y direction despite
some rotations.

We start using the application this way as see whether we need options for other transform interpretations. If there is clearly a need we will apply it.
