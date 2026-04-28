# Annotation nodes

← Back to [Reference Guide hub](../../atomCAD_reference_guide.md)

## comment

Adds text annotations to document your node network. Comment nodes do not have input or output pins and do not affect the evaluation of the network.

**Properties**

- `Label` — An optional title displayed in a yellow header bar.
- `Text` — The main comment text.

Comment nodes can be resized by dragging the handle in the bottom-right corner.

## parameter

Defines an input parameter for a subnetwork. When placed inside a node network that is used as a custom node, each `parameter` node becomes an input pin on the resulting custom node. See the [Subnetworks](../node_networks.md#subnetworks) section for details and examples.

**Properties**

- `Name` — The parameter name (becomes the input pin label on the custom node).
- `Type` — The data type of the parameter.
- `Sort Order` — Determines the order of parameters on the custom node.
