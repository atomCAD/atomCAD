import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';

/// A small info icon button that shows the node type description in a dialog.
/// Can be placed in any node editor header.
class NodeDescriptionButton extends StatelessWidget {
  final String nodeTypeName;
  final double iconSize;

  const NodeDescriptionButton({
    super.key,
    required this.nodeTypeName,
    this.iconSize = 18,
  });

  void _showDescriptionDialog(BuildContext context) {
    // Fetch description from Rust backend
    final description = getNetworkDescription(networkName: nodeTypeName);

    if (description == null) {
      // Fallback if description not available
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('No description available'),
          duration: Duration(seconds: 2),
        ),
      );
      return;
    }

    showDialog(
      context: context,
      barrierDismissible: true, // Click outside to dismiss
      builder: (context) => _DescriptionDialog(
        nodeTypeName: nodeTypeName,
        description: description,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return IconButton(
      icon: Icon(
        Icons.info_outline,
        size: iconSize,
        color: Colors.blue[300],
      ),
      tooltip: 'Show description',
      padding: EdgeInsets.zero,
      constraints: const BoxConstraints(),
      onPressed: () => _showDescriptionDialog(context),
    );
  }
}

/// Dialog that displays the node type description
class _DescriptionDialog extends StatelessWidget {
  final String nodeTypeName;
  final String description;

  const _DescriptionDialog({
    required this.nodeTypeName,
    required this.description,
  });

  @override
  Widget build(BuildContext context) {
    return DraggableDialog(
      backgroundColor: Colors.grey[900]!,
      width: 400,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Header with title and close button
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Expanded(
                  child: Text(
                    nodeTypeName,
                    style: const TextStyle(
                      color: Colors.white,
                      fontSize: 16,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.close, color: Colors.white70),
                  iconSize: 20,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(),
                  onPressed: () => Navigator.of(context).pop(),
                  tooltip: 'Close',
                ),
              ],
            ),
            const SizedBox(height: 12),
            // Description text
            Container(
              constraints: const BoxConstraints(maxHeight: 300),
              child: SingleChildScrollView(
                child: Text(
                  description.isNotEmpty
                      ? description
                      : 'No description available.',
                  style: const TextStyle(
                    color: Colors.white70,
                    fontSize: 14,
                    height: 1.5,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
