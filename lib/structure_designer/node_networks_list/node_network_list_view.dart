import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/structure_designer/identifier_validation.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/find_usages_menu.dart';

/// Discriminator between the two kinds of user-defined types listed in this
/// view: node networks and record type defs.
enum _UserTypeKind { network, recordDef }

class _UserTypeEntry {
  final String name;
  final _UserTypeKind kind;
  final String? validationErrors; // only meaningful for networks

  _UserTypeEntry.network(this.name, this.validationErrors)
      : kind = _UserTypeKind.network;
  _UserTypeEntry.recordDef(this.name)
      : kind = _UserTypeKind.recordDef,
        validationErrors = null;
}

/// List view widget for user types (node networks + record defs) with rename
/// functionality and per-row activation.
class NodeNetworkListView extends StatefulWidget {
  final StructureDesignerModel model;

  const NodeNetworkListView({super.key, required this.model});

  @override
  State<NodeNetworkListView> createState() => _NodeNetworkListViewState();
}

class _NodeNetworkListViewState extends State<NodeNetworkListView>
    with AutomaticKeepAliveClientMixin {
  // Track which entry is being renamed (if any). The kind is needed because
  // networks and record defs route their rename through different APIs.
  String? _editingName;
  _UserTypeKind? _editingKind;
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
    if (!_renameFocusNode.hasFocus && _editingName != null) {
      _commitRename();
    }
  }

  @override
  Widget build(BuildContext context) {
    super.build(context); // Required for AutomaticKeepAliveClientMixin

    final activeNetworkName = widget.model.nodeNetworkView?.name;
    final activeRecordDef = widget.model.activeRecordDefName;

    // Build the unified entry list: networks first (alphabetical preserved
    // from the kernel), then record defs (alphabetical from the kernel).
    final entries = <_UserTypeEntry>[
      ...widget.model.nodeNetworkNames
          .map((n) => _UserTypeEntry.network(n.name, n.validationErrors)),
      ...widget.model.recordTypeDefNames.map(_UserTypeEntry.recordDef),
    ];

    if (entries.isEmpty) {
      return const Center(
        child: Text('No user types defined'),
      );
    }

    return ListView.builder(
      itemCount: entries.length,
      itemBuilder: (context, index) {
        final entry = entries[index];
        final entryName = entry.name;
        final bool isActive = entry.kind == _UserTypeKind.network
            ? (activeRecordDef == null && entryName == activeNetworkName)
            : (entryName == activeRecordDef);
        final bool isEditing =
            _editingName == entryName && _editingKind == entry.kind;
        final bool hasValidationErrors = entry.validationErrors != null;

        // Networks only: how many instance nodes reference this network,
        // anywhere in the design (Find Usages, issue #414). Record defs have no
        // usage search yet, and a zero count reserves no space at all.
        final int usageCount = entry.kind == _UserTypeKind.network
            ? (widget.model.networkUsageCounts[entryName] ?? 0)
            : 0;

        return Builder(
          builder: (BuildContext itemContext) {
            return GestureDetector(
              onSecondaryTap: () {
                final RelativeRect position =
                    menuPositionForWidget(itemContext);

                showMenu<String>(
                  context: context,
                  position: position,
                  items: <PopupMenuEntry<String>>[
                    // Navigation first, separated from the editing actions.
                    if (entry.kind == _UserTypeKind.network) ...[
                      const PopupMenuItem(
                        value: 'find_usages',
                        child: Text('Find Usages'),
                      ),
                      const PopupMenuDivider(),
                    ],
                    const PopupMenuItem(
                      value: 'rename',
                      child: Text('Rename'),
                    ),
                    // Duplicate is network-only: a shallow copy (inline zone
                    // bodies copied, references to other networks kept as
                    // references) under an auto-generated unique name.
                    if (entry.kind == _UserTypeKind.network)
                      const PopupMenuItem(
                        value: 'duplicate',
                        child: Text('Duplicate'),
                      ),
                    const PopupMenuItem(
                      value: 'delete',
                      child: Text('Delete'),
                    ),
                  ],
                ).then((value) {
                  if (!itemContext.mounted) return;
                  if (value == 'find_usages') {
                    findUsagesOfNetwork(
                      context: itemContext,
                      model: widget.model,
                      networkName: entryName,
                      position: menuPositionForWidget(itemContext),
                    );
                  } else if (value == 'rename') {
                    _startRenaming(entryName, entry.kind);
                  } else if (value == 'duplicate') {
                    widget.model.duplicateNodeNetwork(entryName);
                  } else if (value == 'delete') {
                    _handleDelete(context, entryName, entry.kind);
                  }
                });
              },
              onDoubleTap: () {
                _startRenaming(entryName, entry.kind);
              },
              child: Tooltip(
                message: hasValidationErrors ? entry.validationErrors! : '',
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
                    key: Key(
                        '${entry.kind == _UserTypeKind.network ? 'network' : 'record_def'}_item_$entryName'),
                    dense: true,
                    visualDensity: AppSpacing.compactVerticalDensity,
                    contentPadding:
                        const EdgeInsets.symmetric(horizontal: 12, vertical: 0),
                    leading: Icon(
                      entry.kind == _UserTypeKind.network
                          ? Icons.account_tree
                          : Icons.data_object,
                      size: 16,
                      color: isActive
                          ? AppColors.selectionForeground
                          : Colors.grey,
                    ),
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
                                key: const Key('rename_text_field'),
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
                        : Text(entryName, style: AppTextStyles.regular),
                    // A bare number, no icon — the number *is* the information,
                    // and sidebar space is tight. Clicking it opens the same
                    // usage picker as the context-menu entry.
                    trailing: usageCount > 0
                        ? Builder(
                            builder: (BuildContext countContext) => Tooltip(
                              message:
                                  'Used by $usageCount node${usageCount == 1 ? '' : 's'}',
                              child: InkWell(
                                onTap: () => findUsagesOfNetwork(
                                  context: countContext,
                                  model: widget.model,
                                  networkName: entryName,
                                  position: menuPositionForWidget(countContext),
                                ),
                                child: Padding(
                                  padding: const EdgeInsets.symmetric(
                                      horizontal: 4, vertical: 2),
                                  child: Text(
                                    '$usageCount',
                                    style: AppTextStyles.regular.copyWith(
                                      fontSize: 11,
                                      color: isActive
                                          ? AppColors.selectionForeground
                                              .withValues(alpha: 0.8)
                                          : Colors.grey,
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          )
                        : null,
                    selected: isActive,
                    selectedTileColor: AppColors.selectionBackground,
                    selectedColor: AppColors.selectionForeground,
                    onTap: () {
                      if (isEditing) return;
                      if (entry.kind == _UserTypeKind.network) {
                        widget.model.setActiveNodeNetwork(entryName);
                      } else {
                        widget.model.setActiveRecordDef(entryName);
                      }
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

  void _startRenaming(String name, _UserTypeKind kind) {
    setState(() {
      _editingName = name;
      _editingKind = kind;
      _renameController.text = name;
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
    if (_editingName == null || _editingKind == null) return;
    final oldName = _editingName!;
    final kind = _editingKind!;
    final newName = _renameController.text;
    if (newName.isNotEmpty && newName != oldName) {
      final validationError = validateUserName(newName);
      if (validationError != null) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Rename failed: $validationError')),
          );
        }
      } else if (kind == _UserTypeKind.network) {
        final success = widget.model.renameNodeNetwork(oldName, newName);
        if (!success && mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Rename failed: name already exists')),
          );
        }
      } else {
        final error = widget.model.renameRecordTypeDef(oldName, newName);
        if (error != null && mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Rename failed: $error')),
          );
        }
      }
    }
    _renameFocusNode.unfocus();
    setState(() {
      _editingName = null;
      _editingKind = null;
    });
  }

  void _cancelRename() {
    if (_editingName != null) {
      _renameFocusNode.unfocus();
      setState(() {
        _editingName = null;
        _editingKind = null;
      });
    }
  }

  Future<void> _handleDelete(
      BuildContext context, String name, _UserTypeKind kind) async {
    final kindLabel =
        kind == _UserTypeKind.network ? 'node network' : 'record type def';
    final titleLabel =
        kind == _UserTypeKind.network ? 'Network' : 'Record Type Def';
    final confirmed = await showDraggableAlertDialog<bool>(
      context: context,
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

    if (confirmed == true && context.mounted) {
      final errorMessage = kind == _UserTypeKind.network
          ? widget.model.deleteNodeNetwork(name)
          : widget.model.deleteRecordTypeDef(name);
      if (errorMessage != null && context.mounted) {
        await showDraggableAlertDialog(
          context: context,
          title: Text('Cannot Delete $titleLabel'),
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
  }
}
