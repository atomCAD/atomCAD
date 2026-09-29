/// Save As dependency copy and *Export project bundle…*
/// (`doc/design_library_linking.md` D11, §5.3).
///
/// A design and every file it depends on — linked libraries, the data files
/// its nodes and its libraries' nodes read — form a fixed relative layout.
/// Rust computes where each dependency goes when the design moves, which
/// ones would need copying and which conflict, and refuses a move that would
/// overwrite anything not in the plan; this file only shows the plan, asks,
/// and reports:
///
/// - [saveAsWithDependenciesInteractive] — the dialog, shown only when
///   something would be copied or conflicts (moving inside a workspace costs
///   nothing);
/// - [exportProjectBundleInteractive] — the zip of the design and its
///   dependencies.
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/library_link_actions.dart'
    show fileNameOf;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

String _plural(int n, String one, [String? many]) =>
    '$n ${n == 1 ? one : (many ?? '${one}s')}';

/// The folder part of [path] (either separator).
String _folderOf(String path) {
  final i = path.lastIndexOf(RegExp(r'[\\/]'));
  return i < 0 ? '' : path.substring(0, i);
}

APISaveAsResult _failed(String message) => APISaveAsResult(
      success: false,
      errorMessage: message,
      copied: const [],
      kept: const [],
      missing: const [],
      external_: const [],
    );

/// *Save As* to [path] with the dependency copy of D11. Shows the dialog
/// when something would be copied or conflicts; otherwise saves at once.
/// Returns `null` when the user cancelled.
Future<APISaveAsResult?> saveAsWithDependenciesInteractive(
    BuildContext context, StructureDesignerModel model, String path) async {
  final plan = model.collectFileDependencies(path);
  if (plan.error != null) return _failed(plan.error!);
  if (!plan.needsConfirmation) {
    return model.saveAsWithDependencies(path, copy: true);
  }
  final choice = await showDialog<_SaveAsChoice>(
    context: context,
    barrierDismissible: false,
    builder: (_) => _SaveAsDependenciesDialog(plan: plan, path: path),
  );
  if (choice == null) return null;
  final result = model.saveAsWithDependencies(
    path,
    copy: choice.copy,
    overwriteTargets: choice.overwriteTargets,
  );
  if (result.success && context.mounted) {
    final parts = <String>[
      if (result.copied.isNotEmpty)
        'copied ${_plural(result.copied.length, 'dependency', 'dependencies')}',
      if (result.kept.isNotEmpty)
        'kept ${_plural(result.kept.length, 'existing file')}',
      if (!choice.copy && result.missing.isNotEmpty)
        '${_plural(result.missing.length, 'dependency', 'dependencies')} '
            'missing at the new location',
    ];
    showTransientSnackBar(
      context,
      'Saved ${fileNameOf(path)}'
      '${parts.isEmpty ? '' : ' — ${parts.join(', ')}'}',
    );
  }
  return result;
}

class _SaveAsChoice {
  final bool copy;
  final List<String> overwriteTargets;
  const _SaveAsChoice({required this.copy, this.overwriteTargets = const []});
}

/// The D11 dialog: three groups (inside the destination folder, outside it
/// with full target paths, external), a status per entry, an explicit
/// overwrite / keep choice when anything conflicts.
class _SaveAsDependenciesDialog extends StatefulWidget {
  final APIDependencyPlan plan;
  final String path;

  const _SaveAsDependenciesDialog({required this.plan, required this.path});

  @override
  State<_SaveAsDependenciesDialog> createState() =>
      _SaveAsDependenciesDialogState();
}

class _SaveAsDependenciesDialogState extends State<_SaveAsDependenciesDialog> {
  /// `null` until the user chooses (only asked when there are conflicts).
  bool? _overwrite;

  List<APIDependency> _group(APIDependencyGroup group) =>
      widget.plan.entries.where((e) => e.group == group).toList();

  List<APIDependency> get _conflicts => widget.plan.entries
      .where((e) => e.status == APIDependencyStatus.conflict)
      .toList();

  /// Libraries and data files the design will not find if nothing is copied.
  (int, int) get _wouldBeMissing {
    var libs = 0, data = 0;
    for (final e in widget.plan.entries) {
      if (e.status == APIDependencyStatus.willCopy ||
          e.status == APIDependencyStatus.conflict) {
        if (e.kind == APIDependencyKind.library_) {
          libs++;
        } else {
          data++;
        }
      }
    }
    return (libs, data);
  }

  static String _statusLabel(APIDependencyStatus s) => switch (s) {
        APIDependencyStatus.willCopy => 'will copy',
        APIDependencyStatus.alreadyThere => 'already there',
        APIDependencyStatus.conflict => 'different file already there',
        APIDependencyStatus.sourceMissing => 'missing now too',
        APIDependencyStatus.external_ => 'not copied',
      };

  static Color _statusColor(APIDependencyStatus s) => switch (s) {
        APIDependencyStatus.willCopy => Colors.blue.shade700,
        APIDependencyStatus.conflict => Colors.orange.shade800,
        APIDependencyStatus.sourceMissing => Colors.red.shade700,
        _ => Colors.grey.shade600,
      };

  Widget _row(APIDependency e, {required bool fullTarget}) {
    final label = switch (e.group) {
      APIDependencyGroup.external_ => e.sourceAbs,
      _ when fullTarget => e.targetAbs ?? e.sourceAbs,
      _ => e.relPath ?? e.sourceAbs,
    };
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            e.kind == APIDependencyKind.library_
                ? Icons.link
                : Icons.insert_drive_file_outlined,
            size: 14,
            color: Colors.grey.shade600,
          ),
          const SizedBox(width: 6),
          Expanded(
            child: Tooltip(
              message: 'From ${e.sourceAbs}'
                  '${e.targetAbs == null ? '' : '\nTo ${e.targetAbs}'}',
              child: Text(label, style: AppTextStyles.regular),
            ),
          ),
          const SizedBox(width: 8),
          Text(
            _statusLabel(e.status),
            style: AppTextStyles.small.copyWith(color: _statusColor(e.status)),
          ),
        ],
      ),
    );
  }

  Widget _section(String title, List<APIDependency> entries,
      {bool fullTarget = false, String? note}) {
    if (entries.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title,
              style:
                  AppTextStyles.regular.copyWith(fontWeight: FontWeight.w600)),
          if (note != null)
            Text(note, style: AppTextStyles.small.copyWith(color: Colors.grey)),
          const SizedBox(height: 4),
          for (final e in entries) _row(e, fullTarget: fullTarget),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final conflicts = _conflicts;
    final (missingLibs, missingData) = _wouldBeMissing;
    final canCopy = conflicts.isEmpty || _overwrite != null;
    return DraggableDialog(
      width: 620,
      dismissible: true,
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Save As — dependencies',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 4),
            Text(
              'This design links libraries or reads data files by relative '
              'paths. To keep every path working at '
              '${_folderOf(widget.path)}, they are copied into the same '
              'relative layout.',
              style: AppTextStyles.regular,
            ),
            ConstrainedBox(
              constraints: BoxConstraints(
                  maxHeight: MediaQuery.of(context).size.height * 0.45),
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    _section('Inside the destination folder',
                        _group(APIDependencyGroup.inside)),
                    _section('Outside the destination folder',
                        _group(APIDependencyGroup.outside),
                        fullTarget: true,
                        note: 'Reached through "..": these copies land '
                            'outside the folder you picked.'),
                    _section('External (absolute paths)',
                        _group(APIDependencyGroup.external_),
                        note: 'Not copied; the saved design keeps pointing '
                            'at them.'),
                  ],
                ),
              ),
            ),
            if (widget.plan.hasWiredPaths) ...[
              const SizedBox(height: 12),
              const ErrorBanner(
                warning: true,
                message: 'Some nodes take their file path through a wire. '
                    'Those files are only known when the network runs, so '
                    'they are not listed and not copied.',
              ),
            ],
            if (conflicts.isNotEmpty) ...[
              const SizedBox(height: 12),
              Text(
                '${_plural(conflicts.length, 'target already holds', 'targets already hold')} '
                'a different file:',
                style: AppTextStyles.regular,
              ),
              RadioListTile<bool>(
                key: const Key('save_as_overwrite_conflicts'),
                dense: true,
                contentPadding: EdgeInsets.zero,
                value: true,
                groupValue: _overwrite,
                onChanged: (v) => setState(() => _overwrite = v),
                title: const Text('Overwrite them with the copies'),
              ),
              RadioListTile<bool>(
                key: const Key('save_as_keep_conflicts'),
                dense: true,
                contentPadding: EdgeInsets.zero,
                value: false,
                groupValue: _overwrite,
                onChanged: (v) => setState(() => _overwrite = v),
                title: const Text('Keep the existing files'),
                subtitle: const Text('The saved design will then use a '
                    'different version of those files.'),
              ),
            ],
            const SizedBox(height: 16),
            Row(
              children: [
                Tooltip(
                  message: 'Writes only the design. At the new location '
                      '${_plural(missingLibs, 'library', 'libraries')} and '
                      '${_plural(missingData, 'data file')} will be missing.',
                  child: TextButton(
                    key: const Key('save_as_without_dependencies'),
                    onPressed: () => Navigator.of(context)
                        .pop(const _SaveAsChoice(copy: false)),
                    child: const Text('Save without dependencies'),
                  ),
                ),
                const Spacer(),
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  key: const Key('save_as_copy_dependencies'),
                  onPressed: canCopy
                      ? () => Navigator.of(context).pop(_SaveAsChoice(
                            copy: true,
                            overwriteTargets: _overwrite == true
                                ? [
                                    for (final c in conflicts)
                                      if (c.targetAbs != null) c.targetAbs!
                                  ]
                                : const [],
                          ))
                      : null,
                  child: const Text('Copy dependencies'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// *File > Export project bundle…*: a `.zip` of the design (as it is now,
/// unsaved edits included) and every relative dependency. Unzipping it
/// anywhere reproduces the layout; external files are listed, not included.
Future<void> exportProjectBundleInteractive(
    BuildContext context, StructureDesignerModel model) async {
  final designPath = model.filePath;
  if (designPath == null) {
    showErrorSnackBar(context, 'Save the design before exporting a bundle');
    return;
  }
  final stem = fileNameOf(designPath).replaceAll(RegExp(r'\.cnnd$'), '');
  final picked = await FilePicker.platform.saveFile(
    dialogTitle: 'Export project bundle',
    fileName: '${stem}_bundle.zip',
    type: FileType.custom,
    allowedExtensions: ['zip'],
    initialDirectory: initialDirectoryFor(APIFileDialogPurpose.design),
  );
  if (picked == null || !context.mounted) return;
  final zipPath =
      picked.toLowerCase().endsWith('.zip') ? picked : '$picked.zip';
  final result = model.exportProjectBundle(zipPath);
  if (!result.success) {
    showErrorSnackBar(context, result.errorMessage);
    return;
  }
  final leftOut = [...result.external_, ...result.missing];
  final summary = 'Exported ${_plural(result.files.length, 'file')} to '
      '${fileNameOf(zipPath)}';
  if (leftOut.isEmpty) {
    showTransientSnackBar(context, summary);
    return;
  }
  showActionSnackBar(
    context,
    '$summary — ${_plural(leftOut.length, 'file')} left out',
    persistent: true,
    actions: [
      (
        'Details',
        () => showErrorDialog(
              context: context,
              title: 'Not in the bundle',
              message: [
                if (result.external_.isNotEmpty)
                  'External (absolute paths):\n${result.external_.join('\n')}',
                if (result.missing.isNotEmpty)
                  'Missing:\n${result.missing.join('\n')}',
              ].join('\n\n'),
            ),
      ),
    ],
  );
}
