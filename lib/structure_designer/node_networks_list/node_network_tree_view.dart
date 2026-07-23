import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_fancy_tree_view/flutter_fancy_tree_view.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/move_namespace_dialog.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/new_folder_dialog.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/network_row_badges.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/find_usages_menu.dart';

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

/// Builds a tree from networks, record defs, and explicit empty folders. All
/// participate in the same dot-delimited namespace hierarchy: a record def
/// named `Physics.ElementMapping` nests under a `Physics` folder, which a
/// network `Physics.Spring` can share. Leaves are tagged with their [_LeafKind]
/// so the view routes activation/rename/delete to the right API.
///
/// [folderPaths] are deliberately-created empty folders (see
/// `doc/design_empty_folders.md`); they contribute folder nodes (and their
/// derived ancestors) but no leaves, and dedup against folders implied by
/// entity names through the shared namespace-node map.
List<_NodeNetworkTreeNode> _buildTreeFromNames(
    List<String> networkQualifiedNames,
    List<String> recordDefNames,
    List<String> folderPaths) {
  // Map to track namespace nodes: full namespace path -> node
  final Map<String, _NodeNetworkTreeNode> namespaceNodes = {};
  final List<_NodeNetworkTreeNode> roots = [];

  // Ensures a namespace (folder) node exists at `path`, creating any missing
  // ancestors. Idempotent via `namespaceNodes`.
  void ensureNamespace(String path) {
    final segments = getSegments(path);
    for (int i = 0; i < segments.length; i++) {
      final namespacePath = segments.sublist(0, i + 1).join('.');
      if (namespaceNodes.containsKey(namespacePath)) continue;
      final namespaceNode = _NodeNetworkTreeNode(
        label: segments[i],
        fullName: namespacePath,
        children: [],
        isLeaf: false,
      );
      namespaceNodes[namespacePath] = namespaceNode;
      if (i == 0) {
        roots.add(namespaceNode);
      } else {
        final parentPath = segments.sublist(0, i).join('.');
        (namespaceNodes[parentPath]!.children as List).add(namespaceNode);
      }
    }
  }

  // Combine all three kinds into one path-tagged list (folder = null leafKind),
  // then process uniformly. Sorting by path interleaves everything
  // alphabetically within each folder.
  final entries = <MapEntry<String, _LeafKind?>>[
    for (final n in networkQualifiedNames) MapEntry(n, _LeafKind.network),
    for (final r in recordDefNames) MapEntry(r, _LeafKind.recordDef),
    for (final f in folderPaths) MapEntry(f, null),
  ]..sort((a, b) => a.key.compareTo(b.key));

  for (final entry in entries) {
    final path = entry.key;
    final leafKind = entry.value;
    final segments = getSegments(path);

    if (leafKind == null) {
      // Explicit empty folder: materialize the folder node (and ancestors).
      ensureNamespace(path);
      continue;
    }

    // Entity leaf: ensure intermediate namespaces exist, then add the leaf.
    if (segments.length > 1) {
      ensureNamespace(segments.sublist(0, segments.length - 1).join('.'));
    }
    final leafNode = _NodeNetworkTreeNode(
      label: getSimpleName(path),
      fullName: path,
      children: const [],
      isLeaf: true,
      leafKind: leafKind,
    );
    if (segments.length == 1) {
      roots.add(leafNode);
    } else {
      final parentPath = segments.sublist(0, segments.length - 1).join('.');
      (namespaceNodes[parentPath]!.children as List).add(leafNode);
    }
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
    for (final f in widget.model.folderNames) {
      out.add('f:$f');
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
    final folders = widget.model.folderNames;
    _lastNetworkNames = _composeKeyedNames();

    final roots = _buildTreeFromNames(qualifiedNames, recordDefs, folders);

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
      // Inline rename only edits the last segment in place — preserve the
      // leaf's namespace for both kinds. A typed dot adds a hierarchy level.
      final namespace = getNamespace(oldFullName);
      final newFullName = combineQualifiedName(namespace, newSegment);
      if (newFullName != oldFullName) {
        // Route to the correct API by leaf kind.
        final leafKind = _findLeafKind(oldFullName);
        if (leafKind == _LeafKind.recordDef) {
          errorMessage =
              widget.model.renameRecordTypeDef(oldFullName, newFullName);
          success = errorMessage == null;
        } else {
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
            content: Text(
                'Rename failed: ${errorMessage ?? 'name already exists'}')),
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
        // Re-root the expanded path under the new prefix. `suffix` is "" for
        // the namespace itself or ".rest" for a descendant; an empty target
        // prefix (promote-to-root) drops the leading dot.
        final suffix = ns.substring(oldPrefix.length);
        final migrated = newPrefix.isEmpty
            ? (suffix.startsWith('.') ? suffix.substring(1) : suffix)
            : '$newPrefix$suffix';
        if (migrated.isNotEmpty) toAdd.add(migrated);
      }
    }
    _expandedNamespaces.removeAll(toRemove);
    _expandedNamespaces.addAll(toAdd);
  }

  // --- Move / rename (full-path dialog) ---

  /// Opens the full-path move dialog for a namespace folder, a network leaf,
  /// or a record-def leaf. Record defs are now first-class hierarchy members.
  ///
  /// [initialPath] seeds the dialog's editable target field; the context-menu
  /// caller omits it (the field then defaults to the current path), while a
  /// drag-and-drop caller passes the proposed drop target so the dialog opens
  /// pre-filled with the dropped-to location for confirmation.
  Future<void> _handleMove(BuildContext context, _NodeNetworkTreeNode node,
      {String? initialPath}) async {
    final oldPath = node.fullName;
    if (oldPath == null) return;

    if (node.isLeaf) {
      // Leaf move: the rename refreshes the panel itself; no namespace
      // expansion state needs migrating (only the leaf moves). Route to the
      // kind-appropriate dialog (which commits via the right model method).
      if (node.leafKind == _LeafKind.recordDef) {
        await showMoveRecordDialog(
          context: context,
          model: widget.model,
          oldName: oldPath,
          initialPath: initialPath,
        );
      } else {
        await showMoveNetworkDialog(
          context: context,
          model: widget.model,
          oldName: oldPath,
          initialPath: initialPath,
        );
      }
    } else {
      final newPrefix = await showMoveNamespaceDialog(
        context: context,
        model: widget.model,
        oldPrefix: oldPath,
        initialPath: initialPath,
      );
      if (newPrefix != null && mounted) {
        setState(() => _migrateExpansionState(oldPath, newPrefix));
      }
    }
  }

  // --- Drag-and-drop move ---

  /// True while a tree row is being dragged. Drives the "Move to top level"
  /// drop bar, which only appears mid-drag.
  bool _dragging = false;

  /// Destination namespace for a drop landing on [target]: a folder drops into
  /// itself; a leaf drops into its own containing namespace (file-explorer
  /// style — dropping onto a sibling means "into this folder").
  String _dropNamespaceFor(_NodeNetworkTreeNode target) {
    if (target.isLeaf) {
      return getNamespace(target.fullName!);
    }
    return target.fullName!;
  }

  /// Whether [dragged] may be dropped into [destNamespace]. Rejects no-ops
  /// (already there) and dropping a namespace into itself or a descendant
  /// (which would form a cycle).
  bool _isValidDrop(_NodeNetworkTreeNode dragged, String destNamespace) {
    final draggedPath = dragged.fullName;
    if (draggedPath == null) return false;
    if (!dragged.isLeaf) {
      if (destNamespace == draggedPath ||
          destNamespace.startsWith('$draggedPath.')) {
        return false;
      }
    }
    final proposed =
        combineQualifiedName(destNamespace, getSimpleName(draggedPath));
    if (proposed == draggedPath) return false; // no-op
    return true;
  }

  /// Opens the move dialog seeded with the drop target. Reuses [_handleMove]
  /// so namespace expansion-state migration and kind routing stay shared with
  /// the context-menu path.
  void _performDrop(BuildContext context, _NodeNetworkTreeNode dragged,
      String destNamespace) {
    final draggedPath = dragged.fullName;
    if (draggedPath == null) return;
    final proposed =
        combineQualifiedName(destNamespace, getSimpleName(draggedPath));
    _handleMove(context, dragged, initialPath: proposed);
  }

  /// The chip that follows the cursor during a drag.
  Widget _buildDragFeedback(_NodeNetworkTreeNode node) {
    return Material(
      color: Colors.transparent,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        decoration: BoxDecoration(
          color: AppColors.selectionBackground ?? Colors.blueGrey,
          borderRadius: BorderRadius.circular(4),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              node.isLeaf
                  ? (node.leafKind == _LeafKind.recordDef
                      ? Icons.data_object
                      : Icons.account_tree)
                  : Icons.folder,
              size: 14,
              color: AppColors.selectionForeground,
            ),
            const SizedBox(width: 6),
            Text(
              node.label,
              style: AppTextStyles.regular
                  .copyWith(color: AppColors.selectionForeground),
            ),
          ],
        ),
      ),
    );
  }

  /// The bottom "Move to top level" drop bar, shown only mid-drag.
  Widget _buildRootDropBar() {
    return DragTarget<_NodeNetworkTreeNode>(
      onWillAcceptWithDetails: (details) => _isValidDrop(details.data, ''),
      onAcceptWithDetails: (details) => _performDrop(context, details.data, ''),
      builder: (context, candidate, rejected) {
        final highlighted = candidate.isNotEmpty;
        final accent = AppColors.selectionBackground ?? Colors.blue;
        return Container(
          width: double.infinity,
          margin: const EdgeInsets.all(4),
          padding: const EdgeInsets.symmetric(vertical: 8),
          decoration: BoxDecoration(
            color: highlighted
                ? accent.withValues(alpha: 0.3)
                : Colors.transparent,
            border: Border.all(color: highlighted ? accent : Colors.grey),
            borderRadius: BorderRadius.circular(4),
          ),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(Icons.vertical_align_top,
                  size: 16,
                  color: highlighted
                      ? AppColors.selectionForeground
                      : Colors.grey),
              const SizedBox(width: 6),
              Text(
                'Move to top level',
                style: AppTextStyles.regular.copyWith(
                    color: highlighted
                        ? AppColors.selectionForeground
                        : Colors.grey),
              ),
            ],
          ),
        );
      },
    );
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
      _showDeleteConfirmation(
          context, node.fullName!, node.leafKind ?? _LeafKind.network);
    } else {
      final affectedItems = _collectLeafNames(node);
      _showNamespaceDeleteConfirmation(context, node.fullName!, affectedItems);
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
    List<String> affectedItems,
  ) async {
    final confirmed = await showDraggableAlertDialog<bool>(
      context: context,
      title: const Text('Delete Namespace'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
              'Delete "$prefix" and all ${affectedItems.length} item${affectedItems.length == 1 ? '' : 's'} within it?'),
          const SizedBox(height: 8),
          const Text('Items to be deleted:',
              style: TextStyle(fontWeight: FontWeight.bold)),
          const SizedBox(height: 4),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 300),
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: affectedItems
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

  /// Marks `namespacePath` and all of its ancestor prefixes as expanded so the
  /// folder is open after the tree rebuilds. `_restoreExpansionState` reads
  /// `_expandedNamespaces` on the next `_updateTree`, which is triggered by the
  /// model's `notifyListeners` once the new item is created.
  void _markNamespaceExpanded(String namespacePath) {
    if (namespacePath.isEmpty) return;
    final segments = getSegments(namespacePath);
    for (int i = 1; i <= segments.length; i++) {
      _expandedNamespaces.add(segments.sublist(0, i).join('.'));
    }
  }

  /// Creates a new empty subfolder inside the folder `node`. Prompts for the
  /// folder name (folders are about their name), then creates it and expands
  /// the parent so the new folder is visible. See `doc/design_empty_folders.md`.
  Future<void> _handleAddFolderIn(_NodeNetworkTreeNode node) async {
    final parent = node.fullName ?? '';
    final name =
        await showNewFolderNameDialog(context: context, parentPath: parent);
    if (name == null || name.trim().isEmpty || !mounted) return;
    final fullPath = combineQualifiedName(parent, name.trim());
    _markNamespaceExpanded(fullPath);
    final error = widget.model.addFolder(fullPath);
    if (error != null && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Could not create folder: $error')),
      );
    }
  }

  /// Creates a new node network or record def inside the folder `node` (a
  /// namespace). The simple name is auto-generated to be unique; the user can
  /// rename it afterwards. The folder is expanded so the new item is visible.
  void _handleAddInFolder(_NodeNetworkTreeNode node, {required bool isRecord}) {
    final namespace = node.fullName ?? '';
    _markNamespaceExpanded(namespace);
    if (isRecord) {
      widget.model.addNewRecordTypeDefInNamespace(namespace);
    } else {
      widget.model.addNewNodeNetworkInNamespace(namespace);
    }
  }

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
    final isLocked =
        node.fullName != null && widget.model.isCliWriteLocked(node.fullName!);

    final items = <PopupMenuEntry<String>>[
      // Navigation first, separated from the editing actions. Networks only —
      // record defs have no usage search yet (design "Non-goals").
      if (node.isLeaf && node.leafKind == _LeafKind.network) ...[
        const PopupMenuItem(
          value: 'find_usages',
          child: Text('Find Usages'),
        ),
        const PopupMenuDivider(),
      ],
      // Folders offer targeted creation: a new network/record def created here
      // lands inside this folder rather than at the root (the action-bar
      // buttons stay root-scoped). See issue on tree-view UX.
      if (!node.isLeaf) ...[
        const PopupMenuItem(
          value: 'add_folder_here',
          child: Text('New folder…'),
        ),
        const PopupMenuItem(
          value: 'add_network_here',
          child: Text('Add node network'),
        ),
        const PopupMenuItem(
          value: 'add_record_here',
          child: Text('Add record'),
        ),
        const PopupMenuDivider(),
      ],
      const PopupMenuItem(
        value: 'rename',
        child: Text('Rename'),
      ),
      // Namespaces and leaves (networks and record defs alike) offer a full
      // move/rename dialog: the inline rename only edits this segment in
      // place, while the dialog can change depth or parent (e.g. shift it
      // rootwards) in one atomic operation. See issue #309.
      const PopupMenuItem(
        value: 'move',
        child: Text('Move / rename…'),
      ),
      // Duplicate is network-only: it creates a shallow copy (inline zone
      // bodies copied, references to other networks kept as references) under
      // an auto-generated unique name. Not offered for record defs or folders.
      if (node.isLeaf && node.leafKind == _LeafKind.network)
        const PopupMenuItem(
          value: 'duplicate',
          child: Text('Duplicate'),
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
      if (!context.mounted) return;
      if (value == 'find_usages' && node.fullName != null) {
        // Same cursor position as the context menu it replaces, so the picker
        // opens where the user is already looking.
        findUsagesOfNetwork(
          context: context,
          model: widget.model,
          networkName: node.fullName!,
          position: position,
        );
      } else if (value == 'add_folder_here') {
        _handleAddFolderIn(node);
      } else if (value == 'add_network_here') {
        _handleAddInFolder(node, isRecord: false);
      } else if (value == 'add_record_here') {
        _handleAddInFolder(node, isRecord: true);
      } else if (value == 'rename') {
        _startRenaming(node);
      } else if (value == 'move') {
        _handleMove(context, node);
      } else if (value == 'duplicate' && node.fullName != null) {
        widget.model.duplicateNodeNetwork(node.fullName!);
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
    final folders = widget.model.folderNames;

    if (nodeNetworks.isEmpty && recordDefs.isEmpty && folders.isEmpty) {
      return const Center(
        child: Text('No user types defined'),
      );
    }

    final tree = AnimatedTreeView<_NodeNetworkTreeNode>(
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

        final Widget row = GestureDetector(
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
                            // Trailing badges, mirroring the list view: the
                            // validation-error badge (navigates to the offending
                            // node) then the Find Usages count. Networks only;
                            // each collapses to nothing when absent.
                            if (node.isLeaf &&
                                node.leafKind == _LeafKind.network) ...[
                              buildNetworkErrorBadge(
                                context: context,
                                model: widget.model,
                                networkName: node.fullName!,
                                errors: _errorsForNetwork(node.fullName!),
                              ),
                              buildNetworkUsageCountBadge(
                                context: context,
                                model: widget.model,
                                networkName: node.fullName!,
                                isActive: isActive,
                              ),
                            ],
                            // A collapsed folder hiding an errored network shows
                            // a roll-up dot, so errors are never fully hidden by
                            // collapse — the dot guides the expand path down.
                            if (!node.isLeaf) _buildFolderErrorDot(node),
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

        // Every row is a drop target. A folder accepts into itself; a leaf
        // accepts into its own containing namespace (file-explorer style).
        final Widget dropTarget = DragTarget<_NodeNetworkTreeNode>(
          onWillAcceptWithDetails: (details) {
            if (identical(details.data, node)) return false;
            return _isValidDrop(details.data, _dropNamespaceFor(node));
          },
          onAcceptWithDetails: (details) {
            _performDrop(context, details.data, _dropNamespaceFor(node));
          },
          builder: (context, candidate, rejected) {
            if (candidate.isEmpty) return row;
            final accent = AppColors.selectionBackground ?? Colors.blue;
            return Container(
              decoration: BoxDecoration(
                color: accent.withValues(alpha: 0.25),
                borderRadius: BorderRadius.circular(4),
              ),
              child: row,
            );
          },
        );

        // Drag the row to move it; disabled while inline-renaming so the text
        // field keeps its gestures.
        return Draggable<_NodeNetworkTreeNode>(
          data: node,
          maxSimultaneousDrags: isEditing ? 0 : 1,
          feedback: _buildDragFeedback(node),
          childWhenDragging: Opacity(opacity: 0.4, child: dropTarget),
          onDragStarted: () => setState(() => _dragging = true),
          onDragEnd: (_) => setState(() => _dragging = false),
          child: dropTarget,
        );
      },
    );

    return Column(
      children: [
        Expanded(child: tree),
        if (_dragging) _buildRootDropBar(),
      ],
    );
  }

  /// The validation errors of the network named [networkName], or an empty list
  /// if it has none. Looked up from the model's `nodeNetworkNames` (the tree's
  /// own node model doesn't carry errors), keyed by qualified name.
  List<APIValidationError> _errorsForNetwork(String networkName) {
    for (final n in widget.model.nodeNetworkNames) {
      if (n.name == networkName) return n.validationErrors;
    }
    return const [];
  }

  /// Whether the folder at [folderPath] has any descendant network with errors,
  /// and whether any of those errors is blocking (drives red-vs-amber). A
  /// descendant is any network whose qualified name is under `folderPath.`.
  (bool, bool) _folderErrorState(String folderPath) {
    final prefix = '$folderPath.';
    bool has = false;
    bool blocking = false;
    for (final n in widget.model.nodeNetworkNames) {
      if (n.validationErrors.isNotEmpty && n.name.startsWith(prefix)) {
        has = true;
        if (n.validationErrors.any((e) => e.blocking)) {
          blocking = true;
          break;
        }
      }
    }
    return (has, blocking);
  }

  /// A small roll-up dot on a **collapsed** folder that hides an errored
  /// network. Nothing is shown when the folder is expanded (its descendants
  /// then render their own badges) or when nothing under it has errors, so the
  /// dot points exactly at the branch worth expanding.
  Widget _buildFolderErrorDot(_NodeNetworkTreeNode node) {
    final path = node.fullName;
    if (path == null) return const SizedBox.shrink();
    // Expanded folders reveal their descendants' own badges — no roll-up needed.
    if (_treeController.getExpansionState(node)) return const SizedBox.shrink();
    final (has, blocking) = _folderErrorState(path);
    if (!has) return const SizedBox.shrink();
    final color = blocking ? Colors.red.shade600 : Colors.orange.shade700;
    return Tooltip(
      message: 'Contains validation errors',
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 6),
        child: Container(
          width: 8,
          height: 8,
          decoration: BoxDecoration(color: color, shape: BoxShape.circle),
        ),
      ),
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
