import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// List view widget for node networks with rename functionality.
class NodeNetworkListView extends StatefulWidget {
  final StructureDesignerModel model;

  const NodeNetworkListView({super.key, required this.model});

  @override
  State<NodeNetworkListView> createState() => _NodeNetworkListViewState();
}

class _NodeNetworkListViewState extends State<NodeNetworkListView>
    with AutomaticKeepAliveClientMixin {
  // Track which node network is being renamed (if any)
  String? _editingNetworkName;
  final TextEditingController _renameController = TextEditingController();
  final FocusNode _renameFocusNode = FocusNode();

  @override
  bool get wantKeepAlive => true; // Keep widget alive when switching tabs

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
    super.build(context); // Required for AutomaticKeepAliveClientMixin

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
                                    ? Colors.white.withValues(alpha: 0.5)
                                    : Colors.blue.withValues(alpha: 0.5),
                                width: 1.5,
                              ),
                              borderRadius: BorderRadius.circular(4.0),
                              boxShadow: [
                                BoxShadow(
                                  color: (isActive ? Colors.white : Colors.blue)
                                      .withValues(alpha: 0.2),
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
                                      ? Colors.white.withValues(alpha: 0.3)
                                                                             : Colors.blue.withValues(alpha: 0.3),
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
                                          ?.withValues(alpha: 0.9)
                                      : Colors.white,
                                  hintText:
                                      'Enter network name (Esc to cancel)',
                                  hintStyle: TextStyle(
                                    color: isActive
                                        ? Colors.white.withValues(alpha: 0.7)
                                        : Colors.grey.withValues(alpha: 0.7),
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
