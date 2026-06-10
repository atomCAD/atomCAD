import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/structure_designer/identifier_validation.dart';

/// Prompts for a new folder's name. Returns the entered name (a single segment,
/// or a dotted path to nest several levels), or null on cancel. The caller
/// combines it with the parent namespace and calls `model.addFolder`.
///
/// Unlike networks/records (which are auto-named and renamed later), a folder
/// is *about* its name, so we prompt for it. See `doc/design_empty_folders.md`.
///
/// [parentPath] is shown in the subtitle so the user knows where the folder
/// will be created (null / empty = the top level).
Future<String?> showNewFolderNameDialog({
  required BuildContext context,
  String? parentPath,
}) {
  final controller = TextEditingController();
  final formKey = GlobalKey<FormState>();
  final location = (parentPath == null || parentPath.isEmpty)
      ? 'the top level'
      : '"$parentPath"';
  return showDialog<String?>(
    context: context,
    barrierDismissible: false,
    builder: (context) => DraggableDialog(
      width: 360,
      dismissible: true,
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text('New Folder',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
            const SizedBox(height: 4),
            Text('Created in $location.',
                style: TextStyle(
                    fontSize: 12, color: Theme.of(context).hintColor)),
            const SizedBox(height: 16),
            Form(
              key: formKey,
              child: TextFormField(
                controller: controller,
                autofocus: true,
                decoration: const InputDecoration(
                  labelText: 'Folder name',
                  border: OutlineInputBorder(),
                ),
                validator: (value) => validateUserName(value ?? ''),
                onFieldSubmitted: (_) {
                  if (formKey.currentState!.validate()) {
                    Navigator.of(context).pop(controller.text);
                  }
                },
              ),
            ),
            const SizedBox(height: 20),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(null),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: () {
                    if (formKey.currentState!.validate()) {
                      Navigator.of(context).pop(controller.text);
                    }
                  },
                  child: const Text('Create'),
                ),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}
