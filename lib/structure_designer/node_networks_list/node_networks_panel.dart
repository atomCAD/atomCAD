import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_network_list_view.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/node_network_tree_view.dart';

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
              Padding(
                padding: const EdgeInsets.all(8.0),
                child: Row(
                  children: [
                    // Back button
                    Tooltip(
                      message: 'Go Back',
                      child: IconButton(
                        onPressed: model.canNavigateBack()
                            ? () => model.navigateBack()
                            : null,
                        icon: Icon(
                          Icons.arrow_back,
                          size: 20,
                          color: model.canNavigateBack()
                              ? AppColors.primaryAccent
                              : null,
                        ),
                        padding: const EdgeInsets.all(4.0),
                      ),
                    ),
                    // Forward button
                    Tooltip(
                      message: 'Go Forward',
                      child: IconButton(
                        onPressed: model.canNavigateForward()
                            ? () => model.navigateForward()
                            : null,
                        icon: Icon(
                          Icons.arrow_forward,
                          size: 20,
                          color: model.canNavigateForward()
                              ? AppColors.primaryAccent
                              : null,
                        ),
                        padding: const EdgeInsets.all(4.0),
                      ),
                    ),
                    const SizedBox(
                        width:
                            16.0), // Gap between navigation and action buttons
                    // Add network button (icon only)
                    Expanded(
                      child: Tooltip(
                        message: 'Add network',
                        child: IconButton(
                          onPressed: () {
                            model.addNewNodeNetwork();
                          },
                          icon: Icon(
                            Icons.add,
                            size: 20,
                            color: AppColors.primaryAccent,
                          ),
                          padding: const EdgeInsets.all(4.0),
                        ),
                      ),
                    ),
                    const SizedBox(width: 8.0),
                    // Delete network button (icon only)
                    Expanded(
                      child: Tooltip(
                        message: 'Delete network',
                        child: IconButton(
                          onPressed: model.nodeNetworkView != null
                              ? () => _handleDeleteNetwork(context, model)
                              : null,
                          icon: Icon(
                            Icons.delete,
                            size: 20,
                            color: model.nodeNetworkView != null
                                ? AppColors.primaryAccent
                                : null,
                          ),
                          padding: const EdgeInsets.all(4.0),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              // Divider
              const Divider(height: 1),
              // Tabs
              TabBar(
                controller: _tabController,
                tabs: const [
                  Tab(text: 'List'),
                  Tab(text: 'Tree'),
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

  // Handle the delete network button press
  Future<void> _handleDeleteNetwork(
      BuildContext context, StructureDesignerModel model) async {
    final networkName = model.nodeNetworkView!.name;
    final confirmed = await _showDeleteConfirmationDialog(context, networkName);

    if (confirmed == true) {
      final errorMessage = model.deleteNodeNetwork(networkName);
      if (errorMessage != null && context.mounted) {
        await _showDeleteErrorDialog(context, errorMessage);
      }
    }
  }

  // Show confirmation dialog for network deletion
  Future<bool?> _showDeleteConfirmationDialog(
      BuildContext context, String networkName) {
    return showDialog<bool>(
      context: context,
      builder: (BuildContext context) {
        return AlertDialog(
          title: const Text('Delete Network'),
          content: Text(
            'Are you sure you want to remove the node network "$networkName"?',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(false),
              child: const Text('Cancel'),
            ),
            TextButton(
              onPressed: () => Navigator.of(context).pop(true),
              child: const Text('Delete'),
            ),
          ],
        );
      },
    );
  }

  // Show error dialog when deletion fails
  Future<void> _showDeleteErrorDialog(
      BuildContext context, String errorMessage) {
    return showDialog(
      context: context,
      builder: (BuildContext context) {
        return AlertDialog(
          title: const Text('Cannot Delete Network'),
          content: Text(errorMessage),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('OK'),
            ),
          ],
        );
      },
    );
  }
}
