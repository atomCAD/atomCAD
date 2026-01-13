import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_network_list_view.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_network_tree_view.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_networks_action_bar.dart';

/// A widget that displays node networks in list and tree views with tabs.
class NodeNetworksPanel extends StatefulWidget {
  final StructureDesignerModel model;

  const NodeNetworksPanel({
    super.key,
    required this.model,
  });

  @override
  State<NodeNetworksPanel> createState() => _NodeNetworksPanelState();
}

class _NodeNetworksPanelState extends State<NodeNetworksPanel>
    with SingleTickerProviderStateMixin {
  late TabController _tabController;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 2, vsync: this);
  }

  @override
  void dispose() {
    _tabController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: widget.model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Navigation and action buttons
              NodeNetworksActionBar(model: model),
              // Divider
              const Divider(height: 1),
              // Tabs
              TabBar(
                controller: _tabController,
                tabs: const [
                  Tab(key: Key('network_list_tab'), text: 'List'),
                  Tab(key: Key('network_tree_tab'), text: 'Tree'),
                ],
              ),
              // Tab views
              Expanded(
                child: TabBarView(
                  controller: _tabController,
                  children: [
                    NodeNetworkListView(model: model),
                    NodeNetworkTreeView(model: model),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}
