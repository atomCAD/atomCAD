import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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

class _NodeNetworksListPanelState extends State<NodeNetworksListPanel>
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
              // Add and Delete network buttons
              Padding(
                padding: const EdgeInsets.all(8.0),
                child: Row(
                  children: [
                    // Add network button
                    Expanded(
                      child: SizedBox(
                        height: AppSpacing.buttonHeight,
                        child: Tooltip(
                          message: 'Add network',
                          child: ElevatedButton.icon(
                            onPressed: () {
                              model.addNewNodeNetwork();
                            },
                            icon: Icon(Icons.add,
                                size: 16, color: AppColors.textOnDark),
                            label: const Text('Add'),
                            style: AppButtonStyles.primary,
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 8.0),
                    // Delete network button
                    Expanded(
                      child: SizedBox(
                        height: AppSpacing.buttonHeight,
                        child: Tooltip(
                          message: 'Delete network',
                          child: ElevatedButton.icon(
                            onPressed: model.nodeNetworkView != null
                                ? () => _handleDeleteNetwork(context, model)
                                : null,
                            icon: Icon(
                              Icons.delete,
                              size: 16,
                              color: model.nodeNetworkView != null
                                  ? AppColors.textOnDark
                                  : null,
                            ),
                            label: const Text('Delete'),
                            style: AppButtonStyles.primary,
                          ),
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
                    _NodeNetworkListView(model: model),
                    _NodeNetworkTreeView(model: model),
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

/// Extracted list view widget with rename functionality
class _NodeNetworkListView extends StatefulWidget {
  final StructureDesignerModel model;

  const _NodeNetworkListView({required this.model});

  @override
  State<_NodeNetworkListView> createState() => _NodeNetworkListViewState();
}

class _NodeNetworkListViewState extends State<_NodeNetworkListView> {
  // Track which node network is being renamed (if any)
  String? _editingNetworkName;
  final TextEditingController _renameController = TextEditingController();
  final FocusNode _renameFocusNode = FocusNode();

  @override
  void initState() {
    super.initState();
    _renameFocusNode.addListener(_onFocusChange);
  }

  @override
  void dispose() {
    _renameFocusNode.removeListener(_onFocusChange);
    _renameController.dispose();
    _renameFocusNode.dispose();
    super.dispose();
  }

  void _onFocusChange() {
    if (!_renameFocusNode.hasFocus && _editingNetworkName != null) {
      _commitRename();
    }
  }

  @override
  Widget build(BuildContext context) {
    final nodeNetworks = widget.model.nodeNetworkNames;
    final activeNetworkName = widget.model.nodeNetworkView?.name;

    if (nodeNetworks.isEmpty) {
      return const Center(
        child: Text('No node networks available'),
      );
    }

    return ListView.builder(
      itemCount: nodeNetworks.length,
      itemBuilder: (context, index) {
        final network = nodeNetworks[index];
        final networkName = network.name;
        final bool isActive = networkName == activeNetworkName;
        final bool isEditing = _editingNetworkName == networkName;
        final bool hasValidationErrors = network.validationErrors != null;

        return Builder(
          builder: (BuildContext itemContext) {
            return GestureDetector(
              onSecondaryTap: () {
                final RenderBox itemBox =
                    itemContext.findRenderObject() as RenderBox;
                final Offset offset = itemBox.localToGlobal(Offset.zero);
                final Size itemSize = itemBox.size;
                final Size screenSize = MediaQuery.of(context).size;

                final RelativeRect position = RelativeRect.fromLTRB(
                  offset.dx,
                  offset.dy,
                  screenSize.width - (offset.dx + itemSize.width),
                  screenSize.height - (offset.dy + itemSize.height),
                );

                showMenu(
                  context: context,
                  position: position,
                  items: [
                    const PopupMenuItem(
                      value: 'rename',
                      child: Text('Rename'),
                    ),
                  ],
                ).then((value) {
                  if (value == 'rename') {
                    _startRenaming(networkName);
                  }
                });
              },
              onDoubleTap: () {
                _startRenaming(networkName);
              },
              child: Tooltip(
                message: hasValidationErrors ? network.validationErrors! : '',
                child: Container(
                  decoration: hasValidationErrors
                      ? BoxDecoration(
                          border: Border.all(color: Colors.red, width: 2.0),
                          borderRadius: BorderRadius.circular(4.0),
                        )
                      : isEditing
                          ? BoxDecoration(
                              border: Border.all(
                                color: isActive
                                    ? Colors.white.withOpacity(0.5)
                                    : Colors.blue.withOpacity(0.5),
                                width: 1.5,
                              ),
                              borderRadius: BorderRadius.circular(4.0),
                              boxShadow: [
                                BoxShadow(
                                  color: (isActive ? Colors.white : Colors.blue)
                                      .withOpacity(0.2),
                                  blurRadius: 4,
                                  spreadRadius: 1,
                                ),
                              ],
                            )
                          : null,
                  child: ListTile(
                    dense: true,
                    visualDensity: AppSpacing.compactVerticalDensity,
                    contentPadding:
                        const EdgeInsets.symmetric(horizontal: 12, vertical: 0),
                    title: isEditing
                        ? CallbackShortcuts(
                            bindings: {
                              const SingleActivator(LogicalKeyboardKey.escape):
                                  _cancelRename,
                            },
                            child: Theme(
                              data: Theme.of(context).copyWith(
                                textSelectionTheme: TextSelectionThemeData(
                                  cursorColor:
                                      isActive ? Colors.white : Colors.black,
                                  selectionColor: isActive
                                      ? Colors.white.withOpacity(0.3)
                                      : Colors.blue.withOpacity(0.3),
                                  selectionHandleColor:
                                      isActive ? Colors.white : Colors.blue,
                                ),
                              ),
                              child: TextField(
                                controller: _renameController,
                                focusNode: _renameFocusNode,
                                autofocus: true,
                                style: AppTextStyles.regular.copyWith(
                                  color: isActive ? Colors.white : Colors.black,
                                  fontWeight: FontWeight.w500,
                                ),
                                decoration: InputDecoration(
                                  isDense: true,
                                  contentPadding: const EdgeInsets.symmetric(
                                    horizontal: 8,
                                    vertical: 8,
                                  ),
                                  enabledBorder: OutlineInputBorder(
                                    borderRadius: BorderRadius.circular(4),
                                    borderSide: BorderSide(
                                      color:
                                          isActive ? Colors.white : Colors.blue,
                                      width: 2.0,
                                    ),
                                  ),
                                  focusedBorder: OutlineInputBorder(
                                    borderRadius: BorderRadius.circular(4),
                                    borderSide: BorderSide(
                                      color:
                                          isActive ? Colors.white : Colors.blue,
                                      width: 2.5,
                                    ),
                                  ),
                                  filled: true,
                                  fillColor: isActive
                                      ? AppColors.selectionBackground
                                          ?.withOpacity(0.9)
                                      : Colors.white,
                                  hintText:
                                      'Enter network name (Esc to cancel)',
                                  hintStyle: TextStyle(
                                    color: isActive
                                        ? Colors.white.withOpacity(0.7)
                                        : Colors.grey.withOpacity(0.7),
                                    fontStyle: FontStyle.italic,
                                  ),
                                ),
                                onSubmitted: (value) => _commitRename(),
                                onEditingComplete: () => _commitRename(),
                              ),
                            ),
                          )
                        : Text(networkName, style: AppTextStyles.regular),
                    selected: isActive,
                    selectedTileColor: AppColors.selectionBackground,
                    selectedColor: AppColors.selectionForeground,
                    onTap: () {
                      if (isEditing) return;
                      widget.model.setActiveNodeNetwork(networkName);
                    },
                  ),
                ),
              ),
            );
          },
        );
      },
    );
  }

  void _startRenaming(String networkName) {
    setState(() {
      _editingNetworkName = networkName;
      _renameController.text = networkName;
    });
    _renameController.selection = TextSelection(
      baseOffset: 0,
      extentOffset: _renameController.text.length,
    );
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _renameFocusNode.requestFocus();
    });
  }

  void _commitRename() {
    if (_editingNetworkName != null) {
      final newName = _renameController.text;
      if (newName.isNotEmpty && newName != _editingNetworkName) {
        widget.model.renameNodeNetwork(_editingNetworkName!, newName);
      }
      _renameFocusNode.unfocus();
      setState(() {
        _editingNetworkName = null;
      });
    }
  }

  void _cancelRename() {
    if (_editingNetworkName != null) {
      _renameFocusNode.unfocus();
      setState(() {
        _editingNetworkName = null;
      });
    }
  }
}

/// Placeholder tree view widget (to be implemented)
class _NodeNetworkTreeView extends StatelessWidget {
  final StructureDesignerModel model;

  const _NodeNetworkTreeView({required this.model});

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Text(
        'Tree view - Coming soon',
        style: TextStyle(fontSize: 16, color: Colors.grey),
      ),
    );
  }
}
