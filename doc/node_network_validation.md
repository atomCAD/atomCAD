There are some operations when editing a node network that can potentially make it invalid, so a validation mechanism is needed.
As parameter nodes inside the node network can be modified, added and deleted,
this validation mechanism is the one which determines what parameters the node type
defined by this sub network has. More specifically the validation process rebuilds the parameters member of the NodeType of this sub network.

Structure designer aims to keep every node network validated all the time. Validation
is automatically called each time there is a chance that a node network became invalid. Once a node network is invalid it is displayed in red in the node networks list panel. In this case the validate button can be used to validate the given node network.
When a node network becomes invalid we invalidate all the node networks that use the given node network as a subnetwork.

Technically the NodeNetwork gets a new 'validated' boolean property.

Network validation is done by a dedicated struct called NetworkValidator.
