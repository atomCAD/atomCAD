import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

class AddNodePopup extends StatefulWidget {
  const AddNodePopup({super.key});

  @override
  _AddNodePopupState createState() => _AddNodePopupState();
}

class _AddNodePopupState extends State<AddNodePopup> {
  final TextEditingController _filterController = TextEditingController();
  List<APINodeTypeView> _allNodes = [];
  List<APINodeTypeView> _filteredNodes = [];

  @override
  void initState() {
    super.initState();
    final allNodes = getNodeTypeViews();
    if (allNodes != null) {
      _allNodes = allNodes;
    }
    _filteredNodes = List.from(_allNodes);
    _filterController.addListener(_filterNodes);
  }

  void _filterNodes() {
    setState(() {
      String query = _filterController.text.toLowerCase();
      _filteredNodes = _allNodes
          .where((node) => node.name.toLowerCase().contains(query))
          .toList();
    });
  }

  void _selectNode(APINodeTypeView node) {
    Navigator.of(context)
        .pop(node.name); // Close popup and return the selected node name
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: Colors.black,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      child: Container(
        width: 240, // Fixed width
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
              height: 240, // Limit height for scrollability
              child: ListView.builder(
                itemCount: _filteredNodes.length,
                itemBuilder: (context, index) {
                  final nodeView = _filteredNodes[index];
                  return ListTile(
                    contentPadding: EdgeInsets.symmetric(
                        vertical: 0, horizontal: 8), // Reduce gap
                    dense: true,
                    visualDensity: VisualDensity(vertical: -4), // to compact
                    title: Text(nodeView.name,
                        style: TextStyle(
                            color: Colors.white, fontSize: 15, height: 1.0)),
                    onTap: () => _selectNode(nodeView),
                  );
                },
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
