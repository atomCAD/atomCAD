import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

// Helper function to get category display name
String getCategoryDisplayName(NodeTypeCategory category) {
  switch (category) {
    case NodeTypeCategory.mathAndProgramming:
      return 'Math and Programming';
    case NodeTypeCategory.geometry2D:
      return '2D Geometry';
    case NodeTypeCategory.geometry3D:
      return '3D Geometry';
    case NodeTypeCategory.atomicStructure:
      return 'Atomic Structure';
    case NodeTypeCategory.otherBuiltin:
      return 'Other';
    case NodeTypeCategory.custom:
      return 'Custom';
  }
}

class AddNodePopup extends StatefulWidget {
  const AddNodePopup({super.key});

  @override
  _AddNodePopupState createState() => _AddNodePopupState();
}

class _AddNodePopupState extends State<AddNodePopup> {
  final TextEditingController _filterController = TextEditingController();
  List<APINodeCategoryView> _allCategories = [];
  List<APINodeCategoryView> _filteredCategories = [];
  APINodeTypeView? _hoveredNode;

  @override
  void initState() {
    super.initState();
    final categories = getNodeTypeViews();
    if (categories != null) {
      _allCategories = categories;
    }
    _filteredCategories = List.from(_allCategories);
    _filterController.addListener(_filterNodes);
  }

  void _filterNodes() {
    setState(() {
      String query = _filterController.text.toLowerCase();
      if (query.isEmpty) {
        // No filter: show all categories with all nodes
        _filteredCategories = List.from(_allCategories);
      } else {
        // Filter: show only categories that have matching nodes
        _filteredCategories = _allCategories
            .map((category) {
              final filteredNodes = category.nodes
                  .where((node) => node.name.toLowerCase().contains(query))
                  .toList();
              if (filteredNodes.isEmpty) {
                return null; // Skip categories with no matching nodes
              }
              return APINodeCategoryView(
                category: category.category,
                nodes: filteredNodes,
              );
            })
            .whereType<APINodeCategoryView>() // Remove nulls
            .toList();
      }
    });
  }

  void _selectNode(APINodeTypeView node) {
    Navigator.of(context)
        .pop(node.name); // Close popup and return the selected node name
  }

  // Build a flat list of items for the ListView: category headers + nodes
  List<dynamic> _buildListItems() {
    List<dynamic> items = [];
    for (var category in _filteredCategories) {
      items.add(category.category); // Add category as header
      items.addAll(category.nodes); // Add all nodes in this category
    }
    return items;
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: Colors.black,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      child: Container(
        width: 560, // Wider to accommodate two panels
        padding: const EdgeInsets.all(16.0),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('Add Node',
                style: TextStyle(
                    color: Colors.white,
                    fontSize: 15,
                    fontWeight: FontWeight.bold)),
            SizedBox(height: 10),
            TextField(
              controller: _filterController,
              decoration: InputDecoration(
                hintText: 'Filter node types...',
                hintStyle: TextStyle(color: Colors.white54),
                filled: true,
                fillColor: Colors.grey[900],
                border:
                    OutlineInputBorder(borderRadius: BorderRadius.circular(8)),
              ),
              style: TextStyle(color: Colors.white),
            ),
            SizedBox(height: 10),
            SizedBox(
              height: 320, // Limit height for scrollability
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  // Left panel: Node list
                  SizedBox(
                    width: 200,
                    child: Container(
                      decoration: BoxDecoration(
                        border: Border.all(color: Colors.grey[800]!),
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: ListView.builder(
                        itemCount: _buildListItems().length,
                        itemBuilder: (context, index) {
                          final items = _buildListItems();
                          final item = items[index];

                          // Check if item is a category header or a node
                          if (item is NodeTypeCategory) {
                            // Render category header
                            return Container(
                              padding: EdgeInsets.fromLTRB(8, 12, 8, 4),
                              child: Text(
                                getCategoryDisplayName(item),
                                style: TextStyle(
                                  color: Colors.blue[300],
                                  fontSize: 12,
                                  fontWeight: FontWeight.bold,
                                  letterSpacing: 0.5,
                                ),
                              ),
                            );
                          } else if (item is APINodeTypeView) {
                            // Render node item
                            final nodeView = item;
                            return Builder(
                              builder: (itemContext) {
                                return MouseRegion(
                                  onEnter: (_) => setState(() {
                                    _hoveredNode = nodeView;
                                  }),
                                  onExit: (event) {
                                    // Get the render box to determine local position
                                    final RenderBox? box = itemContext
                                        .findRenderObject() as RenderBox?;
                                    if (box != null) {
                                      final localPosition =
                                          box.globalToLocal(event.position);
                                      final size = box.size;

                                      // Only clear hover if exiting to the left, top, or bottom
                                      // If exiting to the right (toward description panel), keep the hover
                                      if (localPosition.dx < size.width * 0.9) {
                                        setState(() {
                                          _hoveredNode = null;
                                        });
                                      }
                                    }
                                  },
                                  child: ListTile(
                                    contentPadding: EdgeInsets.symmetric(
                                        vertical: 0, horizontal: 8),
                                    dense: true,
                                    visualDensity: VisualDensity(vertical: -4),
                                    title: Text(nodeView.name,
                                        style: TextStyle(
                                            color: Colors.white,
                                            fontSize: 15,
                                            height: 1.0)),
                                    onTap: () => _selectNode(nodeView),
                                  ),
                                );
                              },
                            );
                          }
                          return SizedBox
                              .shrink(); // Fallback for unknown types
                        },
                      ),
                    ),
                  ),
                  SizedBox(width: 12),
                  // Right panel: Description
                  Expanded(
                    child: Container(
                      padding: EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: Colors.grey[900],
                        borderRadius: BorderRadius.circular(4),
                        border: Border.all(color: Colors.grey[800]!),
                      ),
                      child: _hoveredNode != null
                          ? MouseRegion(
                              onExit: (_) => setState(() {
                                _hoveredNode = null;
                              }),
                              child: SingleChildScrollView(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      _hoveredNode!.name,
                                      style: TextStyle(
                                        color: Colors.white,
                                        fontSize: 14,
                                        fontWeight: FontWeight.bold,
                                      ),
                                    ),
                                    if (_hoveredNode!
                                        .description.isNotEmpty) ...[
                                      SizedBox(height: 8),
                                      Text(
                                        _hoveredNode!.description,
                                        style: TextStyle(
                                          color: Colors.white70,
                                          fontSize: 13,
                                          height: 1.4,
                                        ),
                                      ),
                                    ],
                                  ],
                                ),
                              ),
                            )
                          : Center(
                              child: Text(
                                'Hover over a node type\nto see its description',
                                textAlign: TextAlign.center,
                                style: TextStyle(
                                  color: Colors.white38,
                                  fontSize: 13,
                                ),
                              ),
                            ),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// Function to show the modal popup
Future<String?> showAddNodePopup(BuildContext context) {
  return showDialog<String>(
    context: context,
    barrierDismissible: true, // Close when tapping outside
    builder: (context) => AddNodePopup(),
  );
}
