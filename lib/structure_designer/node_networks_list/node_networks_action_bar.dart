import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// Action bar for node networks with navigation and management buttons.
class NodeNetworksActionBar extends StatelessWidget {
  final StructureDesignerModel model;

  const NodeNetworksActionBar({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Row(
        children: [
          // Back button
          Tooltip(
            message: 'Go Back',
            child: IconButton(
              key: const Key('back_button'),
              onPressed:
                  model.canNavigateBack() ? () => model.navigateBack() : null,
              icon: Icon(
                Icons.arrow_back,
                size: 20,
                color: model.canNavigateBack() ? AppColors.primaryAccent : null,
              ),
              padding: const EdgeInsets.all(4.0),
            ),
          ),
          // Forward button
          Tooltip(
            message: 'Go Forward',
            child: IconButton(
              key: const Key('forward_button'),
              onPressed: model.canNavigateForward()
                  ? () => model.navigateForward()
                  : null,
              icon: Icon(
                Icons.arrow_forward,
                size: 20,
                color:
                    model.canNavigateForward() ? AppColors.primaryAccent : null,
              ),
              padding: const EdgeInsets.all(4.0),
            ),
          ),
          const SizedBox(
              width: 16.0), // Gap between navigation and action buttons
          // Add network button (icon only)
          Expanded(
            child: Tooltip(
              message: 'Add network',
              child: IconButton(
                key: const Key('add_network_button'),
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
                key: const Key('delete_network_button'),
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
    return showDraggableAlertDialog<bool>(
      context: context,
      key: const Key('delete_confirm_dialog'),
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
  }

  // Show error dialog when deletion fails
  Future<void> _showDeleteErrorDialog(
      BuildContext context, String errorMessage) {
    return showDraggableAlertDialog(
      context: context,
      title: const Text('Cannot Delete Network'),
      content: Text(errorMessage),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('OK'),
        ),
      ],
    );
  }
}
