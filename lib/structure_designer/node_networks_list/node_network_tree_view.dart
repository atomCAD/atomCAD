import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_fancy_tree_view/flutter_fancy_tree_view.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// Discriminator between the two kinds of leaves in the user-types tree.
enum _LeafKind { network, recordDef }

/// Tree node representing either a namespace (folder) or a leaf
/// (a node network or record type def).
class _NodeNetworkTreeNode {
  final String label; // Simple name (last segment)
  final String?
      fullName; // Qualified name for leafs, namespace path for namespaces
  final List<_NodeNetworkTreeNode> children;
  final bool isLeaf;
  /// Only meaningful when `isLeaf == true`. Determines which API path is
  /// used for activation/rename/delete.
  final _LeafKind? leafKind;

  _NodeNetworkTreeNode({
    required this.label,
    this.fullName,
    this.children = const [],
    required this.isLeaf,
    this.leafKind,
  });
}

/// Builds a tree from networks (with namespace dots) and record def names
/// (always flat at the root). Record defs do not participate in the
/// namespace hierarchy in v1.
List<_NodeNetworkTreeNode> _buildTreeFromNames(
    List<String> networkQualifiedNames, List<String> recordDefNames) {
  // Map to track namespace nodes: full namespace path -> node
  final Map<String, _NodeNetworkTreeNode> namespaceNodes = {};
  final List<_NodeNetworkTreeNode> roots = [];

  // Sort names to process parent namespaces before children
  final sortedNames = List<String>.from(networkQualifiedNames)..sort();

  for (final qualifiedName in sortedNames) {
    final segments = getSegments(qualifiedName);

    // Build all intermediate namespace nodes if needed
    for (int i = 0; i < segments.length - 1; i++) {
      final namespacePath = segments.sublist(0, i + 1).join('.');

      if (!namespaceNodes.containsKey(namespacePath)) {
        final namespaceNode = _NodeNetworkTreeNode(
          label: segments[i],
          fullName:
              namespacePath, // Store namespace path for expansion tracking
          children: [],
          isLeaf: false,
        );

        namespaceNodes[namespacePath] = namespaceNode;

        // Add to parent or roots
        if (i == 0) {
          roots.add(namespaceNode);
        } else {
          final parentPath = segments.sublist(0, i).join('.');
          final parentNode = namespaceNodes[parentPath]!;
          (parentNode.children as List).add(namespaceNode);
        }
      }
    }

    // Create leaf node for the actual node network
    final leafNode = _NodeNetworkTreeNode(
      label: getSimpleName(qualifiedName),
      fullName: qualifiedName,
      children: [],
      isLeaf: true,
      leafKind: _LeafKind.network,
    );

    // Add leaf to its parent or roots
    if (segments.length == 1) {
      roots.add(leafNode);
    } else {
      final parentPath = segments.sublist(0, segments.length - 1).join('.');
      final parentNode = namespaceNodes[parentPath]!;
      (parentNode.children as List).add(leafNode);
    }
  }

  // Record defs appear flat at the root, alphabetical.
  final sortedRecordDefs = List<String>.from(recordDefNames)..sort();
  for (final defName in sortedRecordDefs) {
    roots.add(_NodeNetworkTreeNode(
      label: defName,
      fullName: defName,
      children: const [],
      isLeaf: true,
      leafKind: _LeafKind.recordDef,
    ));
  }

  return roots;
}

/// Tree view widget for node networks with hierarchical namespace display.
class NodeNetworkTreeView extends StatefulWidget {
  final StructureDesignerModel model;

  const NodeNetworkTreeView({super.key, required this.model});

  @override
  State<NodeNetworkTreeView> createState() => _NodeNetworkTreeViewState();
}

class _NodeNetworkTreeViewState extends State<NodeNetworkTreeView>
    with AutomaticKeepAliveClientMixin {
  late TreeController<_NodeNetworkTreeNode> _treeController;
  final Set<String> _expandedNamespaces = {}; // Track expanded namespace paths
  List<String>? _lastNetworkNames; // For change detection
  String? _lastActiveNetwork; // Track last active network for auto-expansion

  // Rename state
  String? _editingNodeFullName; // fullName of node being renamed
  bool _editingIsLeaf = true; // Whether the node being edited is a leaf
  final TextEditingController _renameController = TextEditingController();
  final FocusNode _renameFocusNode = FocusNode();

  @override
  bool get wantKeepAlive => true; // Keep widget alive when switching tabs

  @override
  void initState() {
    super.initState();
    _renameFocusNode.addListener(_onRenameFocusChange);
    _updateTree();
  }

  @override
  void didUpdateWidget(NodeNetworkTreeView oldWidget) {
    super.didUpdateWidget(oldWidget);

    // Compose a single list keyed by kind so it changes whenever either
    // networks or record defs are added/removed/renamed.
    final currentNames = _composeKeyedNames();
    final currentActiveNetwork = widget.model.nodeNetworkView?.name;

    // Check if active network changed - expand ancestors if so
    if (currentActiveNetwork != _lastActiveNetwork &&
        currentActiveNetwork != null) {
      _expandAncestorsOf(currentActiveNetwork);
      _lastActiveNetwork = currentActiveNetwork;
    }

    // Only rebuild if network list actually changed
    if (_lastNetworkNames != null &&
        _listsEqual(_lastNetworkNames!, currentNames)) {
      return; // Skip rebuild - just a selection change
    }

    // Cancel any in-progress rename when network list changes
    _cancelRename();

    _updateTree();
  }

  /// Returns a single list of "kind-prefixed" names so equality checks
  /// detect changes in either the network or record-def collection.
  List<String> _composeKeyedNames() {
    final out = <String>[];
    for (final n in widget.model.nodeNetworkNames) {
      out.add('n:${n.name}');
    }
    for (final r in widget.model.recordTypeDefNames) {
      out.add('r:$r');
    }
    return out;
  }

  bool _listsEqual(List<String> a, List<String> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  void _updateTree() {
    final qualifiedNames =
        widget.model.nodeNetworkNames.map((n) => n.name).toList();
    final recordDefs = widget.model.recordTypeDefNames;
    _lastNetworkNames = _composeKeyedNames();

    final roots = _buildTreeFromNames(qualifiedNames, recordDefs);

    _treeController = TreeController<_NodeNetworkTreeNode>(
      roots: roots,
      childrenProvider: (node) => node.children,
    );

    // Restore expansion state for existing namespaces
    _restoreExpansionState(roots);

    // If there's an active network, ensure its ancestors are expanded
    final activeNetwork = widget.model.nodeNetworkView?.name;
    if (activeNetwork != null) {
      _expandAncestorsOf(activeNetwork);
      _lastActiveNetwork = activeNetwork;
    }
  }

  void _restoreExpansionState(List<_NodeNetworkTreeNode> roots) {
    // Traverse tree and expand nodes whose path is in _expandedNamespaces
    final validNamespaces = <String>{};

    void traverse(List<_NodeNetworkTreeNode> nodes, String parentPath) {
      for (final node in nodes) {
        if (!node.isLeaf) {
          final namespacePath =
              parentPath.isEmpty ? node.label : '$parentPath.${node.label}';

          validNamespaces.add(namespacePath);

          if (_expandedNamespaces.contains(namespacePath)) {
            _treeController.expand(node);
          }

          if (node.children.isNotEmpty) {
            traverse(node.children, namespacePath);
          }
        }
      }
    }

    traverse(roots, '');

    // Clean up: remove namespaces that no longer exist
    _expandedNamespaces.retainAll(validNamespaces);
  }

  void _expandAncestorsOf(String qualifiedName) {
    final segments = getSegments(qualifiedName);

    // If it's a root-level node, nothing to expand
    if (segments.length <= 1) return;

    // Compute all ancestor namespace paths and add to expansion set
    for (int i = 1; i < segments.length; i++) {
      final namespacePath = segments.sublist(0, i).join('.');
      _expandedNamespaces.add(namespacePath);
    }

    // Find and expand the actual tree nodes
    void expandInTree(List<_NodeNetworkTreeNode> nodes, String parentPath) {
      for (final node in nodes) {
        if (!node.isLeaf) {
          final namespacePath =
              parentPath.isEmpty ? node.label : '$parentPath.${node.label}';

          if (_expandedNamespaces.contains(namespacePath)) {
            _treeController.expand(node);

            // Continue traversing children
            if (node.children.isNotEmpty) {
              expandInTree(node.children, namespacePath);
            }
          }
        }
      }
    }

    expandInTree(_treeController.roots.toList(), '');
  }

  void _onFolderToggled(_NodeNetworkTreeNode node) {
    _treeController.toggleExpansion(node);

    // Update tracking set (namespace path is stored in node.fullName)
    final namespacePath = node.fullName!;
    if (_treeController.getExpansionState(node)) {
      _expandedNamespaces.add(namespacePath);
    } else {
      _expandedNamespaces.remove(namespacePath);
    }
  }

  // --- Rename logic ---

  void _onRenameFocusChange() {
    if (!_renameFocusNode.hasFocus && _editingNodeFullName != null) {
      _commitRename();
    }
  }

  void _startRenaming(_NodeNetworkTreeNode node) {
    setState(() {
      _editingNodeFullName = node.fullName;
      _editingIsLeaf = node.isLeaf;
      if (node.isLeaf) {
        _renameController.text = getSimpleName(node.fullName!);
      } else {
        _renameController.text = node.label;
      }
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
    if (_editingNodeFullName == null) return;

    final newSegment = _renameController.text.trim();
    if (newSegment.isEmpty) {
      _cancelRename();
      return;
    }

    final oldFullName = _editingNodeFullName!;
    bool success = true;
    String? errorMessage;

    if (_editingIsLeaf) {
      // Find the kind of leaf so we route to the correct API.
      final leafKind = _findLeafKind(oldFullName);
      if (leafKind == _LeafKind.recordDef) {
        // Record defs are flat — no namespace handling.
        if (newSegment != oldFullName) {
          errorMessage =
              widget.model.renameRecordTypeDef(oldFullName, newSegment);
          success = errorMessage == null;
        }
      } else {
        // Network leaf — preserve namespace.
        final namespace = getNamespace(oldFullName);
        final newFullName = combineQualifiedName(namespace, newSegment);
        if (newFullName != oldFullName) {
          success = widget.model.renameNodeNetwork(oldFullName, newFullName);
        }
      }
    } else {
      // Namespace node rename
      final oldPrefix = oldFullName;
      final parentNamespace = getNamespace(oldPrefix);
      final newPrefix = combineQualifiedName(parentNamespace, newSegment);
      if (newPrefix != oldPrefix) {
        success = widget.model.renameNamespace(oldPrefix, newPrefix);
        if (success) {
          // Migrate expansion state
          _migrateExpansionState(oldPrefix, newPrefix);
        }
      }
    }

    if (!success && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
            content:
                Text('Rename failed: ${errorMessage ?? 'name already exists'}')),
      );
    }

    _renameFocusNode.unfocus();
    setState(() {
      _editingNodeFullName = null;
    });
  }

  /// Walks the live tree to find a leaf's kind by name. Returns null if not
  /// found (defensive — the leaf usually still exists when committing).
  _LeafKind? _findLeafKind(String fullName) {
    _LeafKind? found;
    void traverse(List<_NodeNetworkTreeNode> nodes) {
      for (final n in nodes) {
        if (n.isLeaf && n.fullName == fullName) {
          found = n.leafKind;
          return;
        }
        if (n.children.isNotEmpty) traverse(n.children);
        if (found != null) return;
      }
    }

    traverse(_treeController.roots.toList());
    return found;
  }

  void _migrateExpansionState(String oldPrefix, String newPrefix) {
    final toRemove = <String>[];
    final toAdd = <String>[];
    for (final ns in _expandedNamespaces) {
      if (ns == oldPrefix || ns.startsWith('$oldPrefix.')) {
        toRemove.add(ns);
        toAdd.add(ns.replaceFirst(oldPrefix, newPrefix));
      }
    }
    _expandedNamespaces.removeAll(toRemove);
    _expandedNamespaces.addAll(toAdd);
  }

  void _cancelRename() {
    if (_editingNodeFullName != null) {
      _renameFocusNode.unfocus();
      setState(() {
        _editingNodeFullName = null;
      });
    }
  }

  // --- Delete logic ---

  void _handleDelete(BuildContext context, _NodeNetworkTreeNode node) {
    if (node.isLeaf) {
      _showDeleteConfirmation(context, node.fullName!,
          node.leafKind ?? _LeafKind.network);
    } else {
      final affectedNetworks = _collectLeafNames(node);
      _showNamespaceDeleteConfirmation(
          context, node.fullName!, affectedNetworks);
    }
  }

  List<String> _collectLeafNames(_NodeNetworkTreeNode node) {
    final names = <String>[];
    if (node.isLeaf) {
      names.add(node.fullName!);
    } else {
      for (final child in node.children) {
        names.addAll(_collectLeafNames(child));
      }
    }
    return names;
  }

  Future<void> _showDeleteConfirmation(
      BuildContext context, String name, _LeafKind kind) async {
    final kindLabel =
        kind == _LeafKind.network ? 'node network' : 'record type def';
    final titleLabel =
        kind == _LeafKind.network ? 'Network' : 'Record Type Def';
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
      final errorMessage = kind == _LeafKind.network
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

  Future<void> _showNamespaceDeleteConfirmation(
    BuildContext context,
    String prefix,
    List<String> affectedNetworks,
  ) async {
    final confirmed = await showDraggableAlertDialog<bool>(
      context: context,
      title: const Text('Delete Namespace'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
              'Delete "$prefix" and all ${affectedNetworks.length} network${affectedNetworks.length == 1 ? '' : 's'} within it?'),
          const SizedBox(height: 8),
          const Text('Networks to be deleted:',
              style: TextStyle(fontWeight: FontWeight.bold)),
          const SizedBox(height: 4),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 300),
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: affectedNetworks
                    .map((n) => Padding(
                          padding: const EdgeInsets.only(left: 8),
                          child: Text('\u2022 $n'),
                        ))
                    .toList(),
              ),
            ),
          ),
        ],
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
      final errorMessage = widget.model.deleteNamespace(prefix);
      if (errorMessage != null && context.mounted) {
        await showDraggableAlertDialog(
          context: context,
          title: const Text('Cannot Delete Namespace'),
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

  // --- Context menu ---

  void _showContextMenu(
    BuildContext context,
    _NodeNetworkTreeNode node,
    Offset globalPosition,
  ) {
    final screenSize = MediaQuery.of(context).size;
    final position = RelativeRect.fromLTRB(
      globalPosition.dx,
      globalPosition.dy,
      screenSize.width - globalPosition.dx,
      screenSize.height - globalPosition.dy,
    );

    // Check current CLI lock state for this node
    final isLocked = node.fullName != null &&
        widget.model.isCliWriteLocked(node.fullName!);

    final items = <PopupMenuEntry<String>>[
      const PopupMenuItem(
        value: 'rename',
        child: Text('Rename'),
      ),
      const PopupMenuItem(
        value: 'delete',
        child: Text('Delete'),
      ),
      const PopupMenuDivider(),
      PopupMenuItem(
        value: 'toggle_cli_access',
        child: Text(isLocked ? 'Allow CLI Access' : 'Deny CLI Access'),
      ),
    ];

    showMenu<String>(
      context: context,
      position: position,
      items: items,
    ).then((value) {
      if (value == 'rename') {
        _startRenaming(node);
      } else if (value == 'delete') {
        _handleDelete(context, node);
      } else if (value == 'toggle_cli_access' && node.fullName != null) {
        widget.model.setCliAccess(node.fullName!, allowed: isLocked);
      }
    });
  }

  @override
  void dispose() {
    _renameFocusNode.removeListener(_onRenameFocusChange);
    _renameController.dispose();
    _renameFocusNode.dispose();
    _treeController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    super.build(context); // Required for AutomaticKeepAliveClientMixin

    final nodeNetworks = widget.model.nodeNetworkNames;
    final recordDefs = widget.model.recordTypeDefNames;

    if (nodeNetworks.isEmpty && recordDefs.isEmpty) {
      return const Center(
        child: Text('No user types defined'),
      );
    }

    return AnimatedTreeView<_NodeNetworkTreeNode>(
      treeController: _treeController,
      nodeBuilder: (context, entry) {
        final node = entry.node;
        final activeNetworkName = widget.model.nodeNetworkView?.name;
        final activeRecordDef = widget.model.activeRecordDefName;
        final isActiveNetwork = node.isLeaf &&
            node.leafKind == _LeafKind.network &&
            activeRecordDef == null &&
            node.fullName == activeNetworkName;
        final isActiveRecordDef = node.isLeaf &&
            node.leafKind == _LeafKind.recordDef &&
            node.fullName == activeRecordDef;
        final isActive = isActiveNetwork || isActiveRecordDef;
        final isEditing = _editingNodeFullName == node.fullName;

        return GestureDetector(
          key: Key(node.isLeaf
              ? (node.leafKind == _LeafKind.recordDef
                  ? 'record_def_tree_item_${node.fullName}'
                  : 'network_tree_item_${node.fullName}')
              : 'namespace_tree_item_${node.fullName}'),
          onSecondaryTapDown: (details) {
            _showContextMenu(context, node, details.globalPosition);
          },
          onDoubleTap: () {
            _startRenaming(node);
          },
          child: InkWell(
            onTap: () {
              if (isEditing) return;
              if (node.isLeaf) {
                if (node.leafKind == _LeafKind.recordDef) {
                  widget.model.setActiveRecordDef(node.fullName!);
                } else {
                  widget.model.setActiveNodeNetwork(node.fullName!);
                }
              } else {
                // Namespace node - toggle expansion (with state tracking)
                _onFolderToggled(node);
              }
            },
            child: TreeIndentation(
              entry: entry,
              guide: const IndentGuide.connectingLines(indent: 20),
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 4.0),
                child: Row(
                  children: [
                    // Expand/collapse icon for namespace nodes
                    if (!node.isLeaf)
                      FolderButton(
                        isOpen: entry.hasChildren
                            ? _treeController.getExpansionState(node)
                            : false,
                        onPressed: () => _onFolderToggled(node),
                      ),
                    // Label (with icon for leaf nodes)
                    Expanded(
                      child: Container(
                        padding: EdgeInsets.fromLTRB(
                          node.isLeaf ? 6 : 0,
                          4,
                          8,
                          4,
                        ),
                        decoration: isActive
                            ? BoxDecoration(
                                color: AppColors.selectionBackground,
                                borderRadius: BorderRadius.circular(4),
                              )
                            : null,
                        child: Row(
                          children: [
                            // Icon for leaf nodes (inside the selection container)
                            if (node.isLeaf) ...[
                              Icon(
                                node.leafKind == _LeafKind.recordDef
                                    ? Icons.data_object
                                    : Icons.account_tree,
                                size: 16,
                                color: isActive
                                    ? AppColors.selectionForeground
                                    : Colors.grey,
                              ),
                              const SizedBox(width: 6),
                            ],
                            Expanded(
                              child: isEditing
                                  ? _buildRenameField(isActive)
                                  : Text(
                                      node.label,
                                      style: AppTextStyles.regular.copyWith(
                                        color: isActive
                                            ? AppColors.selectionForeground
                                            : null,
                                        fontWeight: node.isLeaf
                                            ? FontWeight.normal
                                            : FontWeight.w500,
                                      ),
                                    ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  Widget _buildRenameField(bool isActive) {
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): _cancelRename,
      },
      child: SizedBox(
        height: 24,
        child: TextField(
          key: const Key('tree_rename_text_field'),
          controller: _renameController,
          focusNode: _renameFocusNode,
          autofocus: true,
          style: AppTextStyles.regular.copyWith(
            color: isActive ? Colors.white : Colors.black,
            fontSize: 13,
          ),
          decoration: InputDecoration(
            isDense: true,
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 6,
              vertical: 4,
            ),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(3),
              borderSide: BorderSide(
                color: isActive ? Colors.white : Colors.blue,
                width: 1.5,
              ),
            ),
            focusedBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(3),
              borderSide: BorderSide(
                color: isActive ? Colors.white : Colors.blue,
                width: 2.0,
              ),
            ),
            filled: true,
            fillColor: isActive
                ? AppColors.selectionBackground?.withValues(alpha: 0.9)
                : Colors.white,
          ),
          onSubmitted: (_) => _commitRename(),
          onEditingComplete: () => _commitRename(),
        ),
      ),
    );
  }
}
