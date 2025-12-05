import 'package:flutter/material.dart';
import 'package:flutter_fancy_tree_view/flutter_fancy_tree_view.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// Tree node representing either a namespace (folder) or a node network (leaf).
class _NodeNetworkTreeNode {
  final String label; // Simple name (last segment)
  final String?
      fullName; // Qualified name for leafs, namespace path for namespaces
  final List<_NodeNetworkTreeNode> children;
  final bool isLeaf; // True if this is an actual node network

  _NodeNetworkTreeNode({
    required this.label,
    this.fullName,
    this.children = const [],
    required this.isLeaf,
  });
}

/// Builds a tree structure from a flat list of qualified names.
List<_NodeNetworkTreeNode> _buildTreeFromNames(List<String> qualifiedNames) {
  // Map to track namespace nodes: full namespace path -> node
  final Map<String, _NodeNetworkTreeNode> namespaceNodes = {};
  final List<_NodeNetworkTreeNode> roots = [];

  // Sort names to process parent namespaces before children
  final sortedNames = List<String>.from(qualifiedNames)..sort();

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

  return roots;
}

/// Tree view widget for node networks with hierarchical namespace display.
class NodeNetworkTreeView extends StatefulWidget {
  final StructureDesignerModel model;

  const NodeNetworkTreeView({super.key, required this.model});

  @override
  State<NodeNetworkTreeView> createState() => _NodeNetworkTreeViewState();
}

class _NodeNetworkTreeViewState extends State<NodeNetworkTreeView> {
  late TreeController<_NodeNetworkTreeNode> _treeController;
  final Set<String> _expandedNamespaces = {}; // Track expanded namespace paths
  List<String>? _lastNetworkNames; // For change detection

  @override
  void initState() {
    super.initState();
    _updateTree();
  }

  @override
  void didUpdateWidget(NodeNetworkTreeView oldWidget) {
    super.didUpdateWidget(oldWidget);

    // Only rebuild if network list actually changed
    final currentNames =
        widget.model.nodeNetworkNames.map((n) => n.name).toList();
    if (_lastNetworkNames != null &&
        _listsEqual(_lastNetworkNames!, currentNames)) {
      return; // Skip rebuild - just a selection change
    }

    _updateTree();
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
    _lastNetworkNames = List.from(qualifiedNames);

    final roots = _buildTreeFromNames(qualifiedNames);

    _treeController = TreeController<_NodeNetworkTreeNode>(
      roots: roots,
      childrenProvider: (node) => node.children,
    );

    // Restore expansion state for existing namespaces
    _restoreExpansionState(roots);
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

  @override
  void dispose() {
    _treeController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final nodeNetworks = widget.model.nodeNetworkNames;

    if (nodeNetworks.isEmpty) {
      return const Center(
        child: Text('No node networks available'),
      );
    }

    return AnimatedTreeView<_NodeNetworkTreeNode>(
      treeController: _treeController,
      nodeBuilder: (context, entry) {
        final node = entry.node;
        final activeNetworkName = widget.model.nodeNetworkView?.name;
        final isActive = node.isLeaf && node.fullName == activeNetworkName;

        return InkWell(
          onTap: () {
            if (node.isLeaf) {
              // Leaf node - activate the network
              widget.model.setActiveNodeNetwork(node.fullName!);
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
                              Icons.account_tree,
                              size: 16,
                              color: isActive
                                  ? AppColors.selectionForeground
                                  : Colors.grey,
                            ),
                            const SizedBox(width: 6),
                          ],
                          Text(
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
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}
