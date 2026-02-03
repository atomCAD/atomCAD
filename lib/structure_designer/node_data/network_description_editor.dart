import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';

/// Editor for the active node network's description and summary
/// Displayed when no node is selected
class NetworkDescriptionEditor extends StatefulWidget {
  const NetworkDescriptionEditor({super.key});

  @override
  State<NetworkDescriptionEditor> createState() =>
      _NetworkDescriptionEditorState();
}

class _NetworkDescriptionEditorState extends State<NetworkDescriptionEditor> {
  late TextEditingController _descriptionController;
  late TextEditingController _summaryController;
  bool _hasChanges = false;
  String? _errorMessage;

  @override
  void initState() {
    super.initState();
    final description = getActiveNetworkDescription() ?? '';
    final summary = getActiveNetworkSummary() ?? '';
    _descriptionController = TextEditingController(text: description);
    _summaryController = TextEditingController(text: summary);
    _descriptionController.addListener(_onTextChanged);
    _summaryController.addListener(_onTextChanged);
  }

  @override
  void dispose() {
    _descriptionController.removeListener(_onTextChanged);
    _summaryController.removeListener(_onTextChanged);
    _descriptionController.dispose();
    _summaryController.dispose();
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
    final newDescription = _descriptionController.text;
    final newSummary = _summaryController.text;

    try {
      setActiveNetworkDescription(description: newDescription);
      // Pass null for empty summary to clear it
      setActiveNetworkSummary(
          summary: newSummary.isEmpty ? null : newSummary);
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
      padding: const EdgeInsets.fromLTRB(16.0, 8.0, 16.0, 16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Title
          Text(
            'Network Properties',
            style: Theme.of(context).textTheme.titleMedium?.copyWith(
                  fontWeight: FontWeight.bold,
                ),
          ),
          const SizedBox(height: 12),

          // Summary label with info tooltip
          Row(
            children: [
              Text(
                'Summary',
                style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                      fontWeight: FontWeight.w500,
                    ),
              ),
              const SizedBox(width: 4),
              Tooltip(
                message: 'A short summary for CLI verbose listings (optional).',
                child: Icon(
                  Icons.info_outline,
                  size: 16,
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),

          // Summary text field
          Container(
            decoration: BoxDecoration(
              border: Border.all(color: Theme.of(context).colorScheme.outline),
              borderRadius: BorderRadius.circular(4.0),
            ),
            child: TextField(
              controller: _summaryController,
              maxLines: 2,
              minLines: 1,
              decoration: InputDecoration(
                hintText: 'Enter a short summary...',
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
          const SizedBox(height: 16),

          // Description label with info tooltip
          Row(
            children: [
              Text(
                'Description',
                style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                      fontWeight: FontWeight.w500,
                    ),
              ),
              const SizedBox(width: 4),
              Tooltip(
                message: 'A detailed description of the node network.',
                child: Icon(
                  Icons.info_outline,
                  size: 16,
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),

          // Description text editor
          Container(
            decoration: BoxDecoration(
              border: Border.all(color: Theme.of(context).colorScheme.outline),
              borderRadius: BorderRadius.circular(4.0),
            ),
            child: TextField(
              controller: _descriptionController,
              maxLines: 18,
              minLines: 18,
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
