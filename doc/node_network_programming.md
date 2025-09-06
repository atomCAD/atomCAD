# Node network programming in atomCAD

## Generalizing the programmability of a node network

atomCAD's node network evaluation is functional: a node is evaluated by first recursively evaluating its input nodes and then evaluating the node itself. There is no explicit control flow, no global state or side effects.
This is a good for visual debugging because any node can be made visible by just evaluating them on the fly, but the challenge is: how to make this more programmable without losing the functional nature? Also it would be nice to introduce more programmability gradually, as needed, and not by redesigning the whole system.
This is a challange in other node networks, and different node networks chose different approaches.
For example Unreal Engine Blueprints are very procedural: there is explicit state (variables) and explicit control flow (control flow wire).
Blender geometry nodes are more functional / dataflow oriented but quite complicated in terms of how they are evaluated. They contain not very elegant concepts along the way like attribute nodes.

I have identified the following completely functional simple extensions / new features that fits into the current system, can be introduced gradually, and makes atomCAD more programmable:


### Switch node

This node can handle conditions in completely functional way.

Inputs:
- condition (boolean)
- true_value
- false_value

The type of true_value and false_value must be the same as the output type of the node.

### Expression node

This node can evaluate an string mathematical expression in a sinmple matehematical expression language.

Inputs:
- expression (string)
- named values can be added dynamically with their type

Output:
value (number, vector or bool)?

### higher order function nodes

examples: map, filter, reduce, sequence

Let's take the 'map' example. In a functional language 'map' is a function that takes a collection of values and a function that can convert an element in the collection to another element. It returns a new collection with the converted elements.
The challange is: how the function input can be passed to the 'map' node?

An elegant solution is that each node has yet another pin. This pin can be
displayed at the top of the node. It is a function pin. It can be connected to input function pins, like the one of the 'map' node.
This way any repeated execution is carantened into executing a subnetwork multiple times, so we do not introduce complicated control flow inside a newtork.

Example of the power of these kind of nodes: imagine a node that can generate an array of numbers in a range. Connecting this range into a map node you can generate an array of geometries shifted in a pattern. Passing this into a union node you have a repeated pattern geoemtry.

## Validation 

There are some operations when editing a node network that can potentially make it invalid, so a validation mechanism is needed.
As parameter nodes inside the node network can be modified, added and deleted,
this validation mechanism is the one which determines what parameters the node type
defined by this sub network has. More specifically the validation process rebuilds the parameters member of the NodeType of this sub network.

Structure designer aims to keep every node network validated all the time. Validation
is automatically called each time there is a chance that a node network became invalid. Once a node network is invalid it is displayed in red in the node networks list panel. In this case the validate button can be used to validate the given node network.
When a node network becomes invalid we invalidate all the node networks that use the given node network as a subnetwork.

Technically the NodeNetwork gets a new 'validated' boolean property.

Network validation is done by a dedicated struct called NetworkValidator.

## Migration to property - parameter equivalence

Currently a node has parameters (input pins) and properties (settable on the UI),
stored in objects inherited from the NodeData trait.

It would be ideal that all properties were also parameters so that they could be programmatically set.
It would be too much work all at once, so let's do it gradually.

- We introduce some new DataType-s like IVec3, Vec3, etc...
- We introduce new parameters of some nodes with the same name or similar name as some properties.
Example: min_corner and extent parameters for cuboid.

- We change the eval and implicit eval functions for the node so that it gets the data from the parameter value if provided
and from the property only if a parameter value is not provided.

On the UI it would be ideal that if a parameter value is provided the editable property is hidden. An even better feature would
be that although the editable property is hidden, a read only value is displayed: the currently evaluated value for that input pin.

This does not seem to be too much work to do for an example node for some example parameters, but it is quite a lot of work to do it
for all the nodes and all parameters. On the other hand doing it for one example node I can eliminate all the risks.
So I need to implement this asap for the cuboid node and the min_corner and extent parameters.

Next I should do it for the geo_trans and the atom_trans nodes as those are the highest probability candidates for usage.


## 'default' input pin of parameter nodes

When editing a network which is typically used as a subnetwork it is hard to show meaningful evaluated
content unless there are defult values for the parameters.
Therefore we introduce an input pin for the parameter nodes named 'default'. This should have the same type
as the output pin of the parameter node (which is the parameter data type).
The default pin is only evaluated when the parameter is evaluated in a context that there is no parent node network on
the call stack.
When nothing is connected to a parameter node the node evaluation will result in the missing input error.

## Refactor implicit evaluation (geo_tree)

Making everything programmable forced me to think through the evaluation process.
Implicit evaluation is not a good fit for a functional evaluation process
as when implicitly evaluating a geometry each node needs to be evaluated for lots of sample points and calculating subtrees over and over while getting the same value would waste resources extensively.

Currently we use the same node network to do explicit evaluation and do implicit evaluation for Geometry sub networks. This is not scalable for the generic case.

An elegant solution is to completely separate implicit evaluation into a completely
different subsystem. On the node network there should be only explicit evaluation,
but the output of that evaluation in case of geometry nodes is a geometry algebraic expression. This geometrric algebraic expression is stored as a tree.
This can be then used for implicit evaluation, but implicit evaluation in this case
is already a completely different subsystem which has nothing to to do with the node network.

## Types

The atomCAD node network currently has a very simple type system.
There is no 'type constructors' (no 'array of' or 'struct of' type constructs). (Arrays are implicitly supported though as we will see.)
We might introduce more complex types in the future though.

Pin types are the following:

  Int,
  Float,
  Vec2,
  Vec3,
  IVec2,
  IVec3,
  Geometry2D,
  Geometry,
  Atomic

Calculated values can have the type of the output pin or the 'Error' type.

### Arrays

Arrays are supported 'implicitly' in a way that each output value can be an array of the given type (or Error). A single value is just a one length arrray.

Some rules regarding arrays:

- In case of 'multi' pins (where multiple output pins are connected to a single input pin)
the arrays are concatenated.
- When the node expects one single none-error value on an input, the following happen:
  - if the array is empty an error occurs on the input
  - if the arrays has multiple elements the first element is taken without error
  - If the first element is Error, obviously there is an error on the input

### Type conversion

Usually pins with the same type can be connected, but there are also some exceptions,
where automatic conversion happens:

- Int can be converted to Float and vica versa (rounding)
- IVec2 can be converted to Vec2 and vica versa (rounding)
- IVec3 can be converted to Vec3 and vica versa (rounding)
