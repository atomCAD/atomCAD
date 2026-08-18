/// Remembering which folder each kind of file dialog was last used in
/// (issue #420).
///
/// Every `FilePicker` call in the application goes through this file: ask
/// [initialDirectoryFor] before opening the dialog, and call [rememberPickedFile]
/// with whatever the user chose. Nothing else is needed — the folder is
/// persisted Rust-side, per purpose, in
/// `<config_dir>/atomCAD/last_directories.json`.
///
/// ## Why the application has to do this at all
///
/// It looks like it already works, and on Windows it does — but by accident.
/// There, `file_picker` calls comdlg32's `GetOpenFileNameW` /
/// `GetSaveFileNameW`, which fall back to the shell's per-executable folder MRU
/// when the caller names no initial directory. On Linux the same package talks
/// to the XDG desktop portal, which shows a remembered folder only when the app
/// passes one explicitly, so the dialog reopened at `$HOME` every time. Passing
/// `initialDirectory` makes all three platforms behave the same way.
///
/// So: **never call `FilePicker.platform.pickFiles` / `.saveFile` directly.**
/// A new dialog that skips this helper regresses the issue on Linux while
/// looking perfectly fine on the maintainer's Windows box.
library;

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

export 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart'
    show APIFileDialogPurpose;

/// The folder a dialog of this [purpose] should open in, or `null` to let the
/// platform choose.
///
/// Pass the result straight to `FilePicker`'s `initialDirectory`; it already
/// handles the "nothing recorded yet" and "recorded folder has since been
/// deleted" cases by returning `null`.
String? initialDirectoryFor(APIFileDialogPurpose purpose) =>
    sd_api.getLastDirectory(purpose: purpose);

/// Records the folder containing [filePath] as the one to reopen for [purpose].
///
/// Call this with the path the dialog returned — the *file*, not its folder.
/// Passing `null` (the user cancelled) is a no-op, so this can wrap a picker
/// result without a null check at the call site.
void rememberPickedFile(APIFileDialogPurpose purpose, String? filePath) {
  if (filePath == null || filePath.isEmpty) return;
  sd_api.recordLastDirectory(purpose: purpose, filePath: filePath);
}
