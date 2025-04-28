import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A widget that displays a list of node networks from the StructureDesignerModel.
class NodeNetworksListPanel extends StatefulWidget {
  final StructureDesignerModel model;

  const NodeNetworksListPanel({
    super.key,
    required this.model,
  });

  @override
  State<NodeNetworksListPanel> createState() => _NodeNetworksListPanelState();
}

class _NodeNetworksListPanelState extends State<NodeNetworksListPanel> {
  // Track which node network is being renamed (if any)
  String? _editingNetworkName;
  final TextEditingController _renameController = TextEditingController();
  final FocusNode _renameFocusNode = FocusNode();

  @override
  void dispose() {
    _renameController.dispose();
    _renameFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: widget.model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          final nodeNetworks = model.nodeNetworkNames;
          final activeNetworkName = model.nodeNetworkView?.name;

          // Create a column to contain both button and list
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Add network button
              Padding(
                padding: const EdgeInsets.all(8.0),
                child: SizedBox(
                  width: double.infinity,
                  height: AppSpacing.buttonHeight,
                  child: ElevatedButton.icon(
                    onPressed: () {
                      model.addNewNodeNetwork();
                    },
                    icon: Icon(Icons.add, size: 16, color: AppColors.textOnDark),
                    label: const Text('Add network'),
                    style: AppButtonStyles.primary,
                  ),
                ),
              ),
              // Divider between button and list
              const Divider(height: 1),
              // Node networks list
              Expanded(
                child: nodeNetworks.isEmpty
                    ? const Center(
                        child: Text('No node networks available'),
                      )
                    : ListView.builder(
                        itemCount: nodeNetworks.length,
                        itemBuilder: (context, index) {
                          final networkName = nodeNetworks[index];
                          final bool isActive =
                              networkName == activeNetworkName;
                          final bool isEditing =
                              _editingNetworkName == networkName;

                          // Create a context menu for right-click actions
                          return Builder(
                            builder: (BuildContext itemContext) {
                              return GestureDetector(
                                onSecondaryTap: () {
                                  // Get the render box from the current item context
                                  final RenderBox itemBox = itemContext
                                      .findRenderObject() as RenderBox;
                                  final Offset offset =
                                      itemBox.localToGlobal(Offset.zero);

                                  // Size of the screen and item
                                  final Size itemSize = itemBox.size;
                                  final Size screenSize =
                                      MediaQuery.of(context).size;

                                  // Calculate the position with respect to the screen edges
                                  final RelativeRect position =
                                      RelativeRect.fromLTRB(
                                    offset.dx, // Left edge of the item
                                    offset.dy, // Top edge of the item
                                    screenSize.width -
                                        (offset.dx +
                                            itemSize
                                                .width), // Distance from right edge
                                    screenSize.height -
                                        (offset.dy +
                                            itemSize
                                                .height), // Distance from bottom edge
                                  );

                                  showMenu(
                                    context: context,
                                    position: position,
                                    items: [
                                      PopupMenuItem(
                                        value: 'rename',
                                        child: const Text('Rename'),
                                      ),
                                    ],
                                  ).then((value) {
                                    if (value == 'rename') {
                                      _startRenaming(networkName);
                                    }
                                  });
                                },
                                // Add double tap for renaming
                                onDoubleTap: () {
                                  _startRenaming(networkName);
                                },
                                child: ListTile(
                                  dense: true,
                                  visualDensity:
                                      AppSpacing.compactVerticalDensity,
                                  contentPadding: const EdgeInsets.symmetric(
                                      horizontal: 12, vertical: 0),
                                  title: isEditing
                                      ? TextField(
                                          controller: _renameController,
                                          focusNode: _renameFocusNode,
                                          autofocus: true,
                                          // Use appropriate text color based on selection state
                                          style: isActive
                                              ? AppTextStyles.regular.copyWith(
                                                  color: AppColors.textOnDark)
                                              : AppTextStyles.regular,
                                          decoration: InputDecoration(
                                            isDense: true,
                                            contentPadding:
                                                const EdgeInsets.symmetric(
                                                    vertical: 8),
                                            // Use appropriate border color based on selection state
                                            border: OutlineInputBorder(
                                              borderSide: BorderSide(
                                                color: isActive
                                                    ? Colors.white70
                                                    : Colors.grey,
                                              ),
                                            ),
                                            // Use appropriate fill color based on selection state
                                            filled: true,
                                            fillColor: isActive
                                                ? AppColors.selectionBackground
                                                : Colors.white,
                                          ),
                                          onSubmitted: (value) {
                                            _commitRename(model);
                                          },
                                          onEditingComplete: () {
                                            _commitRename(model);
                                          },
                                        )
                                      : Text(
                                          networkName,
                                          style: AppTextStyles.regular,
                                        ),
                                  selected: isActive,
                                  selectedTileColor:
                                      AppColors.selectionBackground,
                                  selectedColor: AppColors.selectionForeground,
                                  onTap: () {
                                    if (isEditing) {
                                      return; // Don't change selection when in edit mode
                                    }

                                    // Set the active node network
                                    model.setActiveNodeNetwork(networkName);
                                    model.refreshFromKernel();
                                  },
                                ),
                              );
                            },
                          );
                        },
                      ),
              ),
            ],
          );
        },
      ),
    );
  }

  // Start the rename process for a node network
  void _startRenaming(String networkName) {
    setState(() {
      _editingNetworkName = networkName;
      _renameController.text = networkName;
    });
    // This will make the TextField select all text when it gets focus
    _renameController.selection = TextSelection(
      baseOffset: 0,
      extentOffset: _renameController.text.length,
    );
  }

  // Apply the rename and exit edit mode
  void _commitRename(StructureDesignerModel model) {
    if (_editingNetworkName != null) {
      final newName = _renameController.text;
      if (newName.isNotEmpty && newName != _editingNetworkName) {
        model.renameNodeNetwork(_editingNetworkName!, newName);
      }

      // Explicitly unfocus before removing the text field
      _renameFocusNode.unfocus();

      setState(() {
        _editingNetworkName = null;
      });
    }
  }
}
