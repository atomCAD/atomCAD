import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/structure_designer/identifier_validation.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/new_folder_dialog.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// Action bar for the user-types panel: navigation arrows, plus add/delete
/// buttons for node networks and record type defs.
class NodeNetworksActionBar extends StatelessWidget {
  final StructureDesignerModel model;

  const NodeNetworksActionBar({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    final hasActiveDef = model.activeRecordDefName != null;
    final hasActiveNetwork = model.nodeNetworkView != null;
    final hasActiveItem = hasActiveDef || hasActiveNetwork;
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
          // Add network button
          Expanded(
            child: Tooltip(
              message: 'Add network',
              child: IconButton(
                key: const Key('add_network_button'),
                onPressed: () {
                  model.addNewNodeNetwork();
                },
                icon: Icon(
                  Icons.account_tree_outlined,
                  size: 20,
                  color: AppColors.primaryAccent,
                ),
                padding: const EdgeInsets.all(4.0),
              ),
            ),
          ),
          const SizedBox(width: 8.0),
          // Add record def button
          Expanded(
            child: Tooltip(
              message: 'Add record type def',
              child: IconButton(
                key: const Key('add_record_def_button'),
                onPressed: () => _handleAddRecordDef(context, model),
                icon: Icon(
                  Icons.data_object,
                  size: 20,
                  color: AppColors.primaryAccent,
                ),
                padding: const EdgeInsets.all(4.0),
              ),
            ),
          ),
          const SizedBox(width: 8.0),
          // New (root-level) folder button. Subfolders are created from a
          // folder's right-click menu; this button is the only way to make a
          // top-level folder. See doc/design_empty_folders.md.
          Expanded(
            child: Tooltip(
              message: 'New folder',
              child: IconButton(
                key: const Key('add_folder_button'),
                onPressed: () => _handleAddFolder(context, model),
                icon: Icon(
                  Icons.create_new_folder_outlined,
                  size: 20,
                  color: AppColors.primaryAccent,
                ),
                padding: const EdgeInsets.all(4.0),
              ),
            ),
          ),
          const SizedBox(width: 8.0),
          // Delete active item button
          Expanded(
            child: Tooltip(
              message:
                  hasActiveDef ? 'Delete record type def' : 'Delete network',
              child: IconButton(
                key: const Key('delete_network_button'),
                onPressed: hasActiveItem
                    ? () => _handleDeleteActive(context, model)
                    : null,
                icon: Icon(
                  Icons.delete,
                  size: 20,
                  color: hasActiveItem ? AppColors.primaryAccent : null,
                ),
                padding: const EdgeInsets.all(4.0),
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// Routes deletion to either `deleteNodeNetwork` or `deleteRecordTypeDef`
  /// depending on which kind of user type is active. The schema editor takes
  /// priority — if a record def is active, it is the candidate for deletion
  /// regardless of which network is also "selected" in the kernel.
  Future<void> _handleDeleteActive(
      BuildContext context, StructureDesignerModel model) async {
    if (model.activeRecordDefName != null) {
      final defName = model.activeRecordDefName!;
      final confirmed = await _showDeleteConfirmationDialog(
          context, defName, 'Record Type Def', 'record type def');
      if (confirmed == true) {
        final errorMessage = model.deleteRecordTypeDef(defName);
        if (errorMessage != null && context.mounted) {
          await _showDeleteErrorDialog(
              context, errorMessage, 'Cannot Delete Record Type Def');
        }
      }
      return;
    }
    final networkName = model.nodeNetworkView!.name;
    final confirmed = await _showDeleteConfirmationDialog(
        context, networkName, 'Network', 'node network');
    if (confirmed == true) {
      final errorMessage = model.deleteNodeNetwork(networkName);
      if (errorMessage != null && context.mounted) {
        await _showDeleteErrorDialog(
            context, errorMessage, 'Cannot Delete Network');
      }
    }
  }

  /// Prompts for a name and creates a new empty folder at the top level.
  /// (Subfolders are created from a folder's right-click menu.)
  Future<void> _handleAddFolder(
      BuildContext context, StructureDesignerModel model) async {
    final name = await showNewFolderNameDialog(context: context);
    if (name == null || name.trim().isEmpty || !context.mounted) return;
    final error = model.addFolder(name.trim());
    if (error != null && context.mounted) {
      await _showDeleteErrorDialog(context, error, 'Cannot Create Folder');
    }
  }

  /// Opens a dialog asking for the new def's name. On confirm, calls the
  /// model and reports any error via dialog (e.g. name collision).
  Future<void> _handleAddRecordDef(
      BuildContext context, StructureDesignerModel model) async {
    final controller = TextEditingController();
    final formKey = GlobalKey<FormState>();
    final name = await showDialog<String?>(
      context: context,
      barrierDismissible: false,
      builder: (context) => DraggableDialog(
        width: 360,
        dismissible: true,
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Text('New Record Type Def',
                  style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
              const SizedBox(height: 16),
              Form(
                key: formKey,
                child: TextFormField(
                  controller: controller,
                  autofocus: true,
                  decoration: const InputDecoration(
                    labelText: 'Name',
                    border: OutlineInputBorder(),
                  ),
                  validator: (value) {
                    final v = value ?? '';
                    final err = validateUserName(v);
                    return err;
                  },
                  onFieldSubmitted: (_) {
                    if (formKey.currentState!.validate()) {
                      Navigator.of(context).pop(controller.text);
                    }
                  },
                ),
              ),
              const SizedBox(height: 20),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(null),
                    child: const Text('Cancel'),
                  ),
                  const SizedBox(width: 8),
                  ElevatedButton(
                    onPressed: () {
                      if (formKey.currentState!.validate()) {
                        Navigator.of(context).pop(controller.text);
                      }
                    },
                    child: const Text('Create'),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );

    if (name != null && name.isNotEmpty && context.mounted) {
      final errorMessage = model.addRecordTypeDef(name);
      if (errorMessage != null && context.mounted) {
        await _showDeleteErrorDialog(
            context, errorMessage, 'Cannot Add Record Type Def');
      }
    }
  }

  // Show confirmation dialog for user-type deletion
  Future<bool?> _showDeleteConfirmationDialog(
      BuildContext context, String name, String titleLabel, String kindLabel) {
    return showDraggableAlertDialog<bool>(
      context: context,
      key: const Key('delete_confirm_dialog'),
      title: Text('Delete $titleLabel'),
      content: Text(
        'Are you sure you want to remove the $kindLabel "$name"?',
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
      BuildContext context, String errorMessage, String title) {
    return showDraggableAlertDialog(
      context: context,
      title: Text(title),
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
