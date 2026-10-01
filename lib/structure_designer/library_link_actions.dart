/// Library linking — the UI actions shared by the File menu, the user-types
/// panel and the read-only canvas banner (`doc/design_library_linking.md`
/// §3, §5.3).
///
/// Presentation and triggers only. Rust decides every alias, path, refresh
/// and repair (`library_links_api.rs`) and refuses every edit of linked
/// content on its own; this file picks files, asks for confirmation, and
/// shows what Rust reports:
///
/// - [showRefreshReport] — the snackbar for an automatic check, a manual
///   refresh, a retarget or an open (D10): transient when the report is
///   clean, persistent with *Details* / *Undo* when something was
///   disconnected, "held" when redo history exists (D9).
/// - [showRefreshReportDialog] — *Details*: every dropped / flagged wire and
///   every frozen node as a row that jumps to the node.
/// - [linkLibraryInteractive], [changeLibraryFileInteractive],
///   [unlinkLibraryInteractive], [refreshLibraryInteractive],
///   [refreshAllDependenciesInteractive], [renameLibraryAliasInteractive],
///   [makeLibraryLocalInteractive].
/// - [openDesignInTab], [openLibraryFile] — open a file in a tab (or switch
///   to the tab that has it), showing what the open reported.
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// The file name of [path] (either separator).
String fileNameOf(String path) => path.split(RegExp(r'[\\/]')).last;

String _plural(int n, String one, [String? many]) =>
    '$n ${n == 1 ? one : (many ?? '${one}s')}';

/// The alias part of a `refreshed_mounts` entry (`alias (path)`).
String _aliasOf(String refreshedMount) {
  final i = refreshedMount.indexOf(' (');
  return i < 0 ? refreshedMount : refreshedMount.substring(0, i);
}

/// True when the report says nothing worth a snackbar: no refresh, no
/// problem, no status other than "loaded again".
bool refreshReportIsSilent(APIRefreshReport r) =>
    r.refreshedMounts.isEmpty &&
    r.refreshedDataFiles.isEmpty &&
    r.reconciledNodes.isEmpty &&
    r.held.isEmpty &&
    r.changedSinceSaved.isEmpty &&
    r.isClean;

/// True when the report is a failure with nothing applied (a refused refresh
/// or retarget).
bool refreshReportIsFailure(APIRefreshReport r) =>
    r.errors.isNotEmpty &&
    r.refreshedMounts.isEmpty &&
    r.refreshedDataFiles.isEmpty &&
    r.statusChanges.isEmpty;

/// The problems part of a report, e.g. "3 wires disconnected, 1 node refers
/// to a missing definition". Empty for a clean report.
String refreshReportProblems(APIRefreshReport r) {
  final parts = <String>[];
  if (r.droppedWires.isNotEmpty) {
    parts.add('${_plural(r.droppedWires.length, 'wire')} disconnected');
  }
  if (r.frozenNodes.isNotEmpty) {
    parts.add(r.frozenNodes.length == 1
        ? '1 node refers to a missing definition'
        : '${r.frozenNodes.length} nodes refer to missing definitions');
  }
  if (r.flaggedWires.isNotEmpty) {
    parts.add('${_plural(r.flaggedWires.length, 'wire')} changed type');
  }
  if (r.outputPinWarnings.isNotEmpty) {
    parts.add(
        '${_plural(r.outputPinWarnings.length, 'wire')} from moved output pins');
  }
  for (final c in r.statusChanges) {
    if (c.status != APIMountStatus.loaded) {
      parts.add('${c.mountPath}: ${c.message}');
    }
  }
  if (r.errors.isNotEmpty) parts.add(r.errors.join('; '));
  return parts.join(', ');
}

/// The one-line headline of a report. [opened] words it for a file open
/// (nothing was "refreshed" by the user there).
String refreshReportHeadline(APIRefreshReport r, {bool opened = false}) {
  final problems = refreshReportProblems(r);
  if (opened) {
    final changed = r.changedSinceSaved.join(', ');
    if (problems.isNotEmpty) {
      return r.reconciledNodes.isNotEmpty
          ? 'Libraries changed since this file was saved — $problems'
          : 'Opened with library problems — $problems';
    }
    if (r.reconciledNodes.isNotEmpty) {
      return 'Rewired ${_plural(r.reconciledNodes.length, 'node')} to changed '
          'libraries';
    }
    if (changed.isNotEmpty) return '$changed changed since this file was saved';
    return 'Opened';
  }
  if (problems.isEmpty) {
    // Clean: name each library with its file (D10's transient wording).
    final what = [
      ...r.refreshedMounts,
      ...r.refreshedDataFiles.map(fileNameOf),
    ].join(', ');
    return what.isEmpty ? 'Libraries are up to date' : 'Refreshed $what';
  }
  // With problems the line is long enough already: aliases only.
  final names = [
    ...r.refreshedMounts.map(_aliasOf),
    ...r.refreshedDataFiles.map(fileNameOf),
  ].join(', ');
  return names.isEmpty ? problems : 'Refreshed $names — $problems';
}

/// Shows what a check, refresh, retarget or open reported (D10). [canUndo]
/// offers *Undo* on a persistent report (a refresh is one undo step; an open
/// is not an undo step at all).
void showRefreshReport(
  BuildContext context,
  StructureDesignerModel model,
  APIRefreshReport report, {
  bool opened = false,
  bool canUndo = true,
  bool quietWhenClean = false,
}) {
  if (refreshReportIsFailure(report)) {
    showErrorSnackBar(context, report.errors.join('\n'));
    return;
  }
  // Held changes (D9) are reported once, when they are first held.
  final nothingApplied = report.refreshedMounts.isEmpty &&
      report.refreshedDataFiles.isEmpty &&
      report.reconciledNodes.isEmpty;
  if (report.held.isNotEmpty && nothingApplied) {
    final names = report.held.map(fileNameOf).join(', ');
    showActionSnackBar(
      context,
      '$names changed on disk — refresh held while redo is available',
      actions: [
        ('Refresh', () => refreshAllDependenciesInteractive(context, model)),
      ],
    );
    return;
  }
  if (report.isClean) {
    if (refreshReportIsSilent(report)) {
      if (!quietWhenClean) {
        showTransientSnackBar(
            context, opened ? 'Opened' : 'Libraries are up to date');
      }
      return;
    }
    showTransientSnackBar(
        context, refreshReportHeadline(report, opened: opened));
    return;
  }
  showActionSnackBar(
    context,
    refreshReportHeadline(report, opened: opened),
    persistent: true,
    actions: [
      ('Details', () => showRefreshReportDialog(context, model, report)),
      if (canUndo && !opened)
        (
          'Undo',
          () {
            final undone = model.undo();
            if (undone != null && context.mounted) {
              showTransientSnackBar(context, 'Undid: $undone');
            }
          }
        ),
    ],
  );
}

/// *Details* of a report (§7.2): navigable rows for every wire and node it
/// names. Clicking a row jumps to the node and closes the dialog.
Future<void> showRefreshReportDialog(
  BuildContext context,
  StructureDesignerModel model,
  APIRefreshReport report,
) {
  return showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (dialogContext) {
      void jump(String network, List<BigInt> scopePath, BigInt nodeId) {
        Navigator.of(dialogContext).pop();
        model.jumpToNode(network, scopePath, nodeId);
      }

      Widget wireRow(APIReportedWire w) => _ReportRow(
            title:
                '${w.network} › ${w.nodeLabel.isEmpty ? '#${w.nodeId}' : w.nodeLabel}'
                '.${w.pinName}'
                '${w.sourceLabel.isEmpty ? '' : '  ←  ${w.sourceLabel}'}',
            subtitle: w.reason,
            onTap: w.nodeLabel.isEmpty
                ? null
                : () => jump(w.network, w.scopePath.toList(), w.nodeId),
          );
      Widget nodeRow(APIReportedNode n) => _ReportRow(
            title:
                '${n.network} › ${n.nodeLabel.isEmpty ? '#${n.nodeId}' : n.nodeLabel}',
            subtitle: n.name,
            onTap: n.nodeLabel.isEmpty
                ? null
                : () => jump(n.network, n.scopePath.toList(), n.nodeId),
          );

      final sections = <(String, List<Widget>)>[
        (
          'Disconnected wires',
          report.droppedWires.map(wireRow).toList(),
        ),
        (
          'Nodes whose definition is missing (kept, with their wires)',
          report.frozenNodes.map(nodeRow).toList(),
        ),
        (
          'Wires whose parameter changed type',
          report.flaggedWires.map(wireRow).toList(),
        ),
        (
          'Wires from output pins that may have moved',
          report.outputPinWarnings.map(wireRow).toList(),
        ),
        (
          'Removed definitions still in use',
          report.removedNamesInUse
              .map((n) => _ReportRow(title: n, subtitle: null, onTap: null))
              .toList(),
        ),
        (
          'Libraries',
          report.statusChanges
              .map((c) => _ReportRow(
                  title: c.mountPath, subtitle: c.message, onTap: null))
              .toList(),
        ),
        (
          'Nodes rewired to a changed interface',
          report.reconciledNodes.map(nodeRow).toList(),
        ),
        (
          'Errors',
          report.errors
              .map((e) => _ReportRow(title: e, subtitle: null, onTap: null))
              .toList(),
        ),
      ].where((s) => s.$2.isNotEmpty).toList();

      return DraggableDialog(
        width: 560,
        dismissible: true,
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Library report',
                  style: Theme.of(dialogContext).textTheme.titleMedium),
              const SizedBox(height: 4),
              SelectableText(refreshReportHeadline(report)),
              const SizedBox(height: 12),
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 420),
                child: SingleChildScrollView(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      for (final (title, rows) in sections) ...[
                        Padding(
                          padding: const EdgeInsets.only(top: 8, bottom: 4),
                          child: Text('$title (${rows.length})',
                              style:
                                  const TextStyle(fontWeight: FontWeight.w600)),
                        ),
                        ...rows,
                      ],
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 12),
              Align(
                alignment: Alignment.centerRight,
                child: TextButton(
                  onPressed: () => Navigator.of(dialogContext).pop(),
                  child: const Text('Close'),
                ),
              ),
            ],
          ),
        ),
      );
    },
  );
}

class _ReportRow extends StatelessWidget {
  final String title;
  final String? subtitle;
  final VoidCallback? onTap;

  const _ReportRow(
      {required this.title, required this.subtitle, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 3, horizontal: 4),
        child: Row(
          children: [
            Icon(onTap == null ? Icons.remove : Icons.arrow_forward,
                size: 14, color: Colors.grey),
            const SizedBox(width: 6),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title, style: AppTextStyles.regular),
                  if (subtitle != null && subtitle!.isNotEmpty)
                    Text(subtitle!,
                        style: AppTextStyles.regular
                            .copyWith(color: Colors.grey, fontSize: 11)),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Link / change file / unlink / refresh
// ---------------------------------------------------------------------------

Future<String?> _pickLibraryFile(String title) async {
  final result = await FilePicker.platform.pickFiles(
    type: FileType.custom,
    allowedExtensions: ['cnnd'],
    dialogTitle: title,
    initialDirectory: initialDirectoryFor(APIFileDialogPurpose.libraryLink),
  );
  final path = result?.files.single.path;
  if (path == null) return null;
  rememberPickedFile(APIFileDialogPurpose.libraryLink, path);
  return path;
}

/// *File > Link library…*: pick a `.cnnd`, choose an alias, link.
Future<void> linkLibraryInteractive(
    BuildContext context, StructureDesignerModel model) async {
  if (model.filePath == null) {
    showErrorSnackBar(context,
        'Save the design first: a library is linked by a path relative to it');
    return;
  }
  final path = await _pickLibraryFile('Link library');
  if (path == null || !context.mounted) return;
  final alias = await showDialog<String>(
    context: context,
    barrierDismissible: false,
    builder: (_) => _LinkAliasDialog(model: model, libraryPath: path),
  );
  if (alias != null && context.mounted) {
    showTransientSnackBar(context, 'Linked $alias (${fileNameOf(path)})');
  }
}

/// Asks for the alias and links on OK; the dialog stays open on an error.
/// Pops with the alias on success.
class _LinkAliasDialog extends StatefulWidget {
  final StructureDesignerModel model;
  final String libraryPath;

  const _LinkAliasDialog({required this.model, required this.libraryPath});

  @override
  State<_LinkAliasDialog> createState() => _LinkAliasDialogState();
}

class _LinkAliasDialogState extends State<_LinkAliasDialog> {
  late final TextEditingController _controller;
  String? _aliasError;
  String? _linkError;

  /// The file has no path relative to the design (another drive): it can
  /// only be linked by copying it next to the design first (D6).
  late final bool _needsCopy =
      widget.model.libraryNeedsCopy(widget.libraryPath);

  @override
  void initState() {
    super.initState();
    _controller =
        TextEditingController(text: suggestLibraryAlias(widget.libraryPath));
    _validate();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _validate() {
    _aliasError = widget.model.checkLibraryAlias(_controller.text.trim());
  }

  void _link() {
    final alias = _controller.text.trim();
    final error = _needsCopy
        ? widget.model.linkLibraryCopying(
            widget.libraryPath, fileNameOf(widget.libraryPath), alias)
        : widget.model.linkLibrary(widget.libraryPath, alias);
    if (error == null) {
      Navigator.of(context).pop(alias);
    } else {
      setState(() => _linkError = error);
    }
  }

  @override
  Widget build(BuildContext context) {
    return DraggableDialog(
      width: 440,
      dismissible: true,
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Link library',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 4),
            Text(fileNameOf(widget.libraryPath),
                style: AppTextStyles.regular.copyWith(color: Colors.grey)),
            const SizedBox(height: 12),
            TextField(
              key: const Key('link_library_alias_field'),
              controller: _controller,
              autofocus: true,
              decoration: InputDecoration(
                labelText: 'Alias',
                helperText:
                    'Its networks appear as alias.name (read-only). Dots group '
                    'libraries in a folder.',
                helperMaxLines: 2,
                errorText: _aliasError,
              ),
              onChanged: (_) => setState(() {
                _validate();
                _linkError = null;
              }),
              onSubmitted: (_) {
                if (_aliasError == null) _link();
              },
            ),
            if (_needsCopy) ...[
              const SizedBox(height: 12),
              ErrorBanner(
                warning: true,
                message: 'This file has no path relative to the design '
                    '(another drive?). A library is linked by a relative '
                    'path, so it will be copied next to the design as '
                    '${fileNameOf(widget.libraryPath)}, together with the '
                    'libraries and data files it uses.',
              ),
            ],
            if (_linkError != null) ...[
              const SizedBox(height: 12),
              ErrorBanner(message: _linkError!),
            ],
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _aliasError == null ? _link : null,
                  child: Text(_needsCopy ? 'Copy and link' : 'Link'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// *Change file…* (retarget) on a direct mount folder.
Future<void> changeLibraryFileInteractive(BuildContext context,
    StructureDesignerModel model, APILibraryMount mount) async {
  final path = await _pickLibraryFile('Change file of ${mount.alias}');
  if (path == null || !context.mounted) return;
  final report = model.retargetLibrary(mount.alias, path);
  if (!context.mounted) return;
  showRefreshReport(context, model, report);
}

/// *Unlink* on a direct mount folder. Refused by Rust while anything uses
/// the library; the refusal names the users.
Future<void> unlinkLibraryInteractive(BuildContext context,
    StructureDesignerModel model, APILibraryMount mount) async {
  final error = model.unlinkLibrary(mount.alias);
  if (!context.mounted) return;
  if (error != null) {
    await showErrorDialog(
        context: context,
        title: 'Cannot unlink ${mount.alias}',
        message: error);
  } else {
    showTransientSnackBar(context, 'Unlinked ${mount.alias}');
  }
}

/// *Rename alias…* on a direct mount folder (D3): every reference to the
/// library moves to the new alias in one undoable step.
Future<void> renameLibraryAliasInteractive(BuildContext context,
    StructureDesignerModel model, APILibraryMount mount) async {
  final newAlias = await showDialog<String>(
    context: context,
    barrierDismissible: false,
    builder: (_) => _RenameAliasDialog(model: model, mount: mount),
  );
  if (newAlias != null && context.mounted) {
    showTransientSnackBar(context, 'Renamed ${mount.alias} to $newAlias');
  }
}

/// *Make local copy* on a direct mount folder (§10): after a confirmation,
/// the library's content becomes part of the design. Refused by Rust while
/// the library is not loaded or something refers to a name it lacks.
Future<void> makeLibraryLocalInteractive(BuildContext context,
    StructureDesignerModel model, APILibraryMount mount) async {
  final confirmed = await showDraggableAlertDialog<bool>(
    context: context,
    title: Text('Make ${mount.alias} local?'),
    content: Text(
        'The networks and record types of ${mount.fileName} (and of the '
        'libraries it links) become part of this design: editable, and saved '
        'in this file. ${mount.fileName} is no longer linked, so later '
        'changes to it do not reach this design. Undo reverses this.'),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(false),
        child: const Text('Cancel'),
      ),
      TextButton(
        onPressed: () => Navigator.of(context).pop(true),
        child: const Text('Make local'),
      ),
    ],
  );
  if (confirmed != true || !context.mounted) return;
  final error = model.makeLibraryLocal(mount.alias);
  if (!context.mounted) return;
  if (error != null) {
    await showErrorDialog(
        context: context,
        title: 'Cannot make ${mount.alias} local',
        message: error);
  } else {
    showTransientSnackBar(context, '${mount.alias} is now part of this design');
  }
}

/// Asks for the new alias; renames on OK and stays open on an error.
class _RenameAliasDialog extends StatefulWidget {
  final StructureDesignerModel model;
  final APILibraryMount mount;

  const _RenameAliasDialog({required this.model, required this.mount});

  @override
  State<_RenameAliasDialog> createState() => _RenameAliasDialogState();
}

class _RenameAliasDialogState extends State<_RenameAliasDialog> {
  late final TextEditingController _controller;
  String? _aliasError;
  String? _renameError;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.mount.alias);
    _validate();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  bool get _unchanged => _controller.text.trim() == widget.mount.alias;

  void _validate() {
    _aliasError = _unchanged
        ? null
        : widget.model.checkLibraryAlias(_controller.text.trim());
  }

  void _rename() {
    final alias = _controller.text.trim();
    final error = widget.model.renameLibraryAlias(widget.mount.alias, alias);
    if (error == null) {
      Navigator.of(context).pop(alias);
    } else {
      setState(() => _renameError = error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final canRename = _aliasError == null && !_unchanged;
    return DraggableDialog(
      width: 440,
      dismissible: true,
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Rename alias',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 4),
            Text(widget.mount.fileName,
                style: AppTextStyles.regular.copyWith(color: Colors.grey)),
            const SizedBox(height: 12),
            TextField(
              key: const Key('rename_library_alias_field'),
              controller: _controller,
              autofocus: true,
              decoration: InputDecoration(
                labelText: 'Alias',
                helperText: 'Every use of ${widget.mount.alias}.… in this '
                    'design is renamed with it. The library file is not '
                    'changed.',
                helperMaxLines: 2,
                errorText: _aliasError,
              ),
              onChanged: (_) => setState(() {
                _validate();
                _renameError = null;
              }),
              onSubmitted: (_) {
                if (canRename) _rename();
              },
            ),
            if (_renameError != null) ...[
              const SizedBox(height: 12),
              ErrorBanner(message: _renameError!),
            ],
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: canRename ? _rename : null,
                  child: const Text('Rename'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// *Refresh* on a mount folder, or a click on its "older than disk" marker.
void refreshLibraryInteractive(
    BuildContext context, StructureDesignerModel model, APILibraryMount mount) {
  final report = model.refreshLibrary(mount.mountPath);
  if (!context.mounted) return;
  showRefreshReport(context, model, report);
}

/// *File > Refresh all dependencies*.
void refreshAllDependenciesInteractive(
    BuildContext context, StructureDesignerModel model) {
  final report = model.refreshAllDependencies();
  if (!context.mounted) return;
  showRefreshReport(context, model, report);
}

// ---------------------------------------------------------------------------
// Opening files in tabs
// ---------------------------------------------------------------------------

/// Shows what the last open reported: the auto-repaired parameter ids
/// (F6 of `doc/design_parameter_wire_stability.md`) and the links (D13).
void showAfterLoadReports(BuildContext context, StructureDesignerModel model) {
  final repairs = model.lastLoadParamIdRepairs;
  final libraryReport = model.lastLoadLibraryReport;
  if (libraryReport != null) {
    showRefreshReport(context, model, libraryReport,
        opened: true, canUndo: false, quietWhenClean: true);
  }
  if (repairs.isEmpty) return;
  final n = repairs.length;
  showDraggableAlertDialog(
    context: context,
    title: const Text('Project auto-repaired'),
    content: SizedBox(
      width: 460,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Auto-repaired $n duplicate parameter id${n == 1 ? '' : 's'} left '
            'by an earlier bug. Existing connections were preserved, but some '
            'wiring in the affected networks may have been mis-connected before '
            'the fix and is worth a quick review. Full per-parameter details '
            'were written to the console.',
          ),
          const SizedBox(height: 12),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 200),
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: repairs
                    .map(
                      (m) => Padding(
                        padding: const EdgeInsets.only(bottom: 6),
                        child: Text('• $m'),
                      ),
                    )
                    .toList(),
              ),
            ),
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('OK'),
      ),
    ],
  );
}

/// Opens [path] in a new tab, or switches to the tab that has it open (D5
/// of `doc/design_multiple_documents.md`), and shows what the open reported:
/// a load error, the load's repairs and library report, the activation's
/// dependency check. Shared by *File > Open…*, *Open Recent* and *Open
/// library file*. Returns whether [path] is now the active document.
Future<bool> openDesignInTab(
    BuildContext context, StructureDesignerModel model, String path) async {
  final result = await model.openDocument(path);
  if (!context.mounted) return result.result.success;
  if (!result.result.success) {
    await showErrorDialog(
        context: context,
        title: 'Load Error',
        message: result.result.errorMessage);
    return false;
  }
  if (!result.alreadyOpen) showAfterLoadReports(context, model);
  final report = result.libraryReport;
  if (report != null) {
    showRefreshReport(context, model, report, quietWhenClean: true);
  }
  return true;
}

/// *Open library file*: opens the library in a tab of its own, or switches
/// to it when it is open already. The design stays open in its tab; a save
/// in the library tab is picked up when the design is activated again
/// (library linking D7).
Future<void> openLibraryFile(BuildContext context, StructureDesignerModel model,
    APILibraryMount mount) async {
  await openDesignInTab(context, model, mount.absPath);
}

/// The strip above the canvas of a linked network: where it comes from, that
/// it is read-only, and the way to edit it (§5.3).
class LinkedNetworkBanner extends StatelessWidget {
  final StructureDesignerModel model;
  final APILibraryMount mount;

  const LinkedNetworkBanner(
      {super.key, required this.model, required this.mount});

  @override
  Widget build(BuildContext context) {
    return Container(
      key: const Key('linked_network_banner'),
      color: Colors.blueGrey.shade50,
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      child: Row(
        children: [
          const Icon(Icons.link, size: 14, color: Colors.blueGrey),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              'Linked from ${mount.relPath} — read-only.',
              overflow: TextOverflow.ellipsis,
              style: AppTextStyles.regular.copyWith(color: Colors.blueGrey),
            ),
          ),
          TextButton(
            onPressed: () => openLibraryFile(context, model, mount),
            child: const Text('Open library file'),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Context-menu entries for linked content (list and tree views)
// ---------------------------------------------------------------------------

const String libMenuOpenFile = 'lib_open_file';
const String libMenuDuplicateLocal = 'lib_duplicate_local';
const String libMenuRefresh = 'lib_refresh';
const String libMenuChangeFile = 'lib_change_file';
const String libMenuUnlink = 'lib_unlink';
const String libMenuRenameAlias = 'lib_rename_alias';
const String libMenuMakeLocal = 'lib_make_local';

/// True when [mount] holds content that *Make local copy* can take over.
bool _hasContent(APILibraryMount mount) =>
    mount.status == APIMountStatus.loaded ||
    mount.status == APIMountStatus.olderThanDisk ||
    mount.status == APIMountStatus.changedOnDisk;

/// The menu of a row under a mount (§3): no Rename / Move / Delete / New.
/// [isNetwork] adds *Duplicate into my file*; [mountFolder] is set when the
/// row *is* the mount folder, which adds *Refresh* and, for a direct link,
/// *Change file…*, *Rename alias…*, *Make local copy* (when loaded) and
/// *Unlink*.
List<PopupMenuEntry<String>> linkedRowMenuItems({
  required bool isNetwork,
  APILibraryMount? mountFolder,
}) {
  return [
    if (mountFolder != null)
      const PopupMenuItem(value: libMenuRefresh, child: Text('Refresh')),
    const PopupMenuItem(
        value: libMenuOpenFile, child: Text('Open library file')),
    if (isNetwork)
      const PopupMenuItem(
          value: libMenuDuplicateLocal, child: Text('Duplicate into my file')),
    if (mountFolder != null && mountFolder.direct) ...[
      const PopupMenuDivider(),
      const PopupMenuItem(
          value: libMenuChangeFile, child: Text('Change file…')),
      const PopupMenuItem(
          value: libMenuRenameAlias, child: Text('Rename alias…')),
      if (_hasContent(mountFolder))
        const PopupMenuItem(
            value: libMenuMakeLocal, child: Text('Make local copy')),
      const PopupMenuItem(value: libMenuUnlink, child: Text('Unlink')),
    ],
  ];
}

/// Runs a [linkedRowMenuItems] choice. Returns false for a value that is not
/// one of them, so the caller can go on with its own dispatch.
bool handleLinkedRowMenuValue(
  BuildContext context,
  StructureDesignerModel model,
  String? value, {
  required APILibraryMount mount,
  String? name,
}) {
  switch (value) {
    case libMenuOpenFile:
      openLibraryFile(context, model, mount);
    case libMenuDuplicateLocal:
      if (name == null) return true;
      final error = model.duplicateNodeNetwork(name);
      if (error != null) showErrorSnackBar(context, error);
    case libMenuRefresh:
      refreshLibraryInteractive(context, model, mount);
    case libMenuChangeFile:
      changeLibraryFileInteractive(context, model, mount);
    case libMenuUnlink:
      unlinkLibraryInteractive(context, model, mount);
    case libMenuRenameAlias:
      renameLibraryAliasInteractive(context, model, mount);
    case libMenuMakeLocal:
      makeLibraryLocalInteractive(context, model, mount);
    default:
      return false;
  }
  return true;
}
