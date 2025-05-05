import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/scene_composer_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A widget that displays a list of clusters from the SceneComposerModel.
class ClusterListPanel extends StatefulWidget {
  final SceneComposerModel model;

  const ClusterListPanel({
    super.key,
    required this.model,
  });

  @override
  State<ClusterListPanel> createState() => _ClusterListPanelState();
}

class _ClusterListPanelState extends State<ClusterListPanel> {
  // Track which cluster is being renamed (if any)
  BigInt? _editingClusterId;
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
      child: Consumer<SceneComposerModel>(
        builder: (context, model, child) {
          final clusters = model.sceneComposerView?.clusters;

          if (clusters == null || clusters.isEmpty) {
            return const Center(
              child: Text('No clusters available'),
            );
          }

          return ListView.builder(
            itemCount: clusters.length,
            itemBuilder: (context, index) {
              final cluster = clusters[index];

              bool isEditing = _editingClusterId == cluster.id;

              // Create a context menu for right-click actions
              return Builder(
                builder: (BuildContext itemContext) {
                  return GestureDetector(
                    onSecondaryTap: () {
                      // Get the render box from the current item context, not the list context
                      final RenderBox itemBox =
                          itemContext.findRenderObject() as RenderBox;
                      final Offset offset = itemBox.localToGlobal(Offset.zero);

                      // Size of the screen and item
                      final Size itemSize = itemBox.size;
                      final Size screenSize = MediaQuery.of(context).size;

                      // Calculate the position with respect to the screen edges
                      final RelativeRect position = RelativeRect.fromLTRB(
                        offset.dx, // Left edge of the item
                        offset.dy, // Top edge of the item
                        screenSize.width -
                            (offset.dx +
                                itemSize.width), // Distance from right edge
                        screenSize.height -
                            (offset.dy +
                                itemSize.height), // Distance from bottom edge
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
                          _startRenaming(cluster);
                        }
                      });
                    },
                    // Add double tap for renaming
                    onDoubleTap: () {
                      _startRenaming(cluster);
                    },
                    child: ListTile(
                      dense: true,
                      visualDensity: AppSpacing.compactVerticalDensity,
                      contentPadding: const EdgeInsets.symmetric(
                          horizontal: 12, vertical: 0),
                      title: isEditing
                          ? TextField(
                              controller: _renameController,
                              focusNode: _renameFocusNode,
                              autofocus: true,
                              style: AppTextStyles.regular,
                              decoration: const InputDecoration(
                                isDense: true,
                                contentPadding:
                                    EdgeInsets.symmetric(vertical: 8),
                                border: OutlineInputBorder(),
                              ),
                              onSubmitted: (value) {
                                _commitRename(model);
                              },
                              onEditingComplete: () {
                                _commitRename(model);
                              },
                            )
                          : Text(
                              cluster.name,
                              style: AppTextStyles.regular,
                            ),
                      selected: cluster.selected,
                      selectedTileColor: AppColors.selectionBackground,
                      selectedColor: AppColors.selectionForeground,
                      onTap: () {
                        if (isEditing) {
                          return; // Don't change selection when in edit mode
                        }

                        // Determine the selection modifier based on pressed keys
                        final selectModifier =
                            HardwareKeyboard.instance.isControlPressed
                                ? SelectModifier.toggle
                                : HardwareKeyboard.instance.isShiftPressed
                                    ? SelectModifier.expand
                                    : SelectModifier.replace;

                        // Call the model's selectCluster method
                        model.selectClusterById(cluster.id, selectModifier);
                      },
                    ),
                  );
                },
              );
            },
          );
        },
      ),
    );
  }

  // Start the rename process for a cluster
  void _startRenaming(ClusterView cluster) {
    setState(() {
      _editingClusterId = cluster.id;
      _renameController.text = cluster.name;
    });
    // This will make the TextField select all text when it gets focus
    _renameController.selection = TextSelection(
      baseOffset: 0,
      extentOffset: _renameController.text.length,
    );
  }

  // Apply the rename and exit edit mode
  void _commitRename(SceneComposerModel model) {
    if (_editingClusterId != null) {
      final newName = _renameController.text;
      if (newName.isNotEmpty) {
        model.renameCluster(_editingClusterId!, newName);
      }

      // Explicitly unfocus before removing the text field
      _renameFocusNode.unfocus();

      setState(() {
        _editingClusterId = null;
      });
    }
  }
}
