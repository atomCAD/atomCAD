import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_network_list_view.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_network_tree_view.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_networks_action_bar.dart';

/// Index of the **Tree** tab, the default view of the panel.
const int NODE_NETWORKS_TREE_TAB_INDEX = 1;

/// A widget that displays node networks in list and tree views with tabs.
///
/// The panel is rebuilt from scratch on every document switch (it is keyed by
/// the document id), so which tab is showing is session UI owned by the
/// caller: [initialTabIndex] seeds the tab controller and [onTabChanged]
/// reports the user's choice back so the next document opens on the same view.
class NodeNetworksPanel extends StatefulWidget {
  final StructureDesignerModel model;
  final int initialTabIndex;
  final ValueChanged<int>? onTabChanged;

  const NodeNetworksPanel({
    super.key,
    required this.model,
    this.initialTabIndex = NODE_NETWORKS_TREE_TAB_INDEX,
    this.onTabChanged,
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
    _tabController = TabController(
      length: 2,
      vsync: this,
      initialIndex: widget.initialTabIndex,
    );
    _tabController.addListener(_onTabControllerChanged);
  }

  void _onTabControllerChanged() {
    if (!_tabController.indexIsChanging) {
      widget.onTabChanged?.call(_tabController.index);
    }
  }

  @override
  void dispose() {
    _tabController.removeListener(_onTabControllerChanged);
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
