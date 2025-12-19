import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';

/// Editor for the active node network's description
/// Displayed when no node is selected
class NetworkDescriptionEditor extends StatefulWidget {
  const NetworkDescriptionEditor({super.key});

  @override
  State<NetworkDescriptionEditor> createState() =>
      _NetworkDescriptionEditorState();
}

class _NetworkDescriptionEditorState extends State<NetworkDescriptionEditor> {
  late TextEditingController _controller;
  bool _hasChanges = false;
  String? _errorMessage;

  @override
  void initState() {
    super.initState();
    final description = getActiveNetworkDescription() ?? '';
    _controller = TextEditingController(text: description);
    _controller.addListener(_onTextChanged);
  }

  @override
  void dispose() {
    _controller.removeListener(_onTextChanged);
    _controller.dispose();
    super.dispose();
  }

  void _onTextChanged() {
    if (!_hasChanges) {
      setState(() {
        _hasChanges = true;
        _errorMessage = null;
      });
    }
  }

  void _applyChanges() {
    final newDescription = _controller.text;

    try {
      setActiveNetworkDescription(description: newDescription);
      setState(() {
        _hasChanges = false;
        _errorMessage = null;
      });

      // Show success feedback
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Network description updated'),
          duration: Duration(seconds: 2),
        ),
      );
    } catch (e) {
      setState(() {
        _errorMessage = 'Failed to update description: $e';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Title
          Text(
            'Network Description',
            style: Theme.of(context).textTheme.titleMedium?.copyWith(
                  fontWeight: FontWeight.bold,
                ),
          ),
          const SizedBox(height: 8),

          // Subtitle
          Text(
            'Edit the description of the active node network.',
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
          ),
          const SizedBox(height: 16),

          // Text editor
          Container(
            decoration: BoxDecoration(
              border: Border.all(color: Theme.of(context).colorScheme.outline),
              borderRadius: BorderRadius.circular(4.0),
            ),
            child: TextField(
              controller: _controller,
              maxLines: 10,
              minLines: 10,
              decoration: InputDecoration(
                hintText: 'Enter a description for this node network...',
                border: InputBorder.none,
                contentPadding: const EdgeInsets.all(12.0),
                hintStyle: TextStyle(
                  color: Theme.of(context)
                      .colorScheme
                      .onSurfaceVariant
                      .withValues(alpha: 0.5),
                ),
              ),
              style: Theme.of(context).textTheme.bodyMedium,
            ),
          ),
          const SizedBox(height: 12),

          // Apply button
          SizedBox(
            width: double.infinity,
            child: ElevatedButton(
              onPressed: _hasChanges ? _applyChanges : null,
              child: const Text('Apply'),
            ),
          ),

          // Error message display
          if (_errorMessage != null)
            Padding(
              padding: const EdgeInsets.only(top: 12.0),
              child: Container(
                width: double.infinity,
                padding: const EdgeInsets.all(12.0),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.errorContainer,
                  borderRadius: BorderRadius.circular(4.0),
                  border: Border.all(
                    color: Theme.of(context).colorScheme.error,
                    width: 1.0,
                  ),
                ),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Icon(
                      Icons.error_outline,
                      color: Theme.of(context).colorScheme.error,
                      size: 20.0,
                    ),
                    const SizedBox(width: 8.0),
                    Expanded(
                      child: Text(
                        _errorMessage!,
                        style: TextStyle(
                          color: Theme.of(context).colorScheme.onErrorContainer,
                          fontSize: 14.0,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
        ],
      ),
    );
  }
}
