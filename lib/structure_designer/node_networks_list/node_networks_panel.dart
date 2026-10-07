import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_network_tree_view.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_networks_action_bar.dart';

/// The user-types panel: the action bar above the tree of node networks and
/// record type defs.
class NodeNetworksPanel extends StatelessWidget {
  final StructureDesignerModel model;

  const NodeNetworksPanel({super.key, required this.model});

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Navigation and action buttons
              NodeNetworksActionBar(model: model),
              const Divider(height: 1),
              Expanded(child: NodeNetworkTreeView(model: model)),
            ],
          );
        },
      ),
    );
  }
}
