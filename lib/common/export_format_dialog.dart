import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import 'draggable_dialog.dart';

/// Shows the shared atom-export format chooser and returns the chosen
/// format's extension (without a leading dot, e.g. `"xyz"`), or `null` if the
/// user cancelled.
///
/// The list of formats is built from `get_atom_export_formats()` — the single
/// source of truth in `crystolecule::io::atom_export` — so a new format added
/// in Rust appears here with no Flutter edits. Used by both the *File → Export
/// visible* menu action and the `export_atoms` node editor's Browse button,
/// which is why the OS save dialog cannot carry the choice (its file-type
/// filter collapses multiple extensions into one combined filter; see the git
/// archaeology in `doc/design_export_atoms_node.md`).
Future<String?> showAtomExportFormatDialog(BuildContext context) {
  final formats = structure_designer_api.getAtomExportFormats();

  return showDraggableAlertDialog<String>(
    context: context,
    title: const Text('Select Export Format'),
    content: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text('Choose the file format for export:'),
        const SizedBox(height: 16),
        for (final format in formats)
          ListTile(
            leading: const Icon(Icons.description),
            title: Text('${format.label} (.${format.extension_})'),
            subtitle: Text(format.description),
            onTap: () => Navigator.of(context).pop(format.extension_),
          ),
      ],
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('Cancel'),
      ),
    ],
  );
}
