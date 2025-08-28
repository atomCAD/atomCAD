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