/// Writing the AI edit log to a file — Phase 4 of
/// `doc/design_ai_edit_history.md`.
///
/// The log is memory-only (D10), so **export is how a session leaves the
/// process**, and that is the feature rather than an afterthought: every
/// downstream use of the log — refining the `atomcad` skill, refining the CLI
/// and the text format, comparing one model against another on the same task —
/// happens outside the application.
///
/// Two forms, because those uses want different things:
///
/// * **JSON** is canonical. Every field of every entry, both text-format
///   snapshots included, so an export is machine-comparable across sessions and
///   models and an entry can be replayed through `edit --replace`.
/// * **Markdown** is readable, for pasting a session into a skill-refinement
///   conversation. It drops the snapshots, which are exactly what would make a
///   chat message unreadable.
///
/// The formatting itself is Rust-side (`ai_edit_export.rs`); this file is the
/// dialog, the write and the reporting. It goes through
/// `lib/common/file_dialog_directory.dart` like every other file dialog in the
/// application — a picker that skips that helper reopens at `$HOME` on Linux
/// while looking perfectly fine on Windows (issue #420).
library;

import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/ai_history_api.dart'
    as ai_history_api;

/// Which of the two export forms to write.
enum AiHistoryExportFormat {
  json,
  markdown;

  String get extension => this == AiHistoryExportFormat.json ? 'json' : 'md';

  String get label => this == AiHistoryExportFormat.json ? 'JSON' : 'Markdown';
}

/// Asks for a file and writes the whole session log to it.
///
/// The text is fetched from the kernel **before** the dialog opens, so an edit
/// arriving from the AI while the dialog is up cannot land half-way into the
/// file — an export is a snapshot of the moment it was asked for.
Future<void> exportAiHistory(
  BuildContext context,
  AiHistoryExportFormat format,
) async {
  final messenger = ScaffoldMessenger.maybeOf(context);
  final text = format == AiHistoryExportFormat.json
      ? ai_history_api.aiHistoryExportJson()
      : ai_history_api.aiHistoryExportMarkdown();

  final outputFile = await FilePicker.platform.saveFile(
    dialogTitle: 'Export AI edit history (${format.label})',
    fileName: 'ai_edit_history.${format.extension}',
    type: FileType.custom,
    allowedExtensions: [format.extension],
    initialDirectory: initialDirectoryFor(APIFileDialogPurpose.aiHistory),
  );
  if (outputFile == null) return;

  final path = outputFile.toLowerCase().endsWith('.${format.extension}')
      ? outputFile
      : '$outputFile.${format.extension}';
  rememberPickedFile(APIFileDialogPurpose.aiHistory, path);

  try {
    await File(path).writeAsString(text, flush: true);
  } catch (e) {
    if (messenger != null) {
      showErrorSnackBarOn(messenger, 'Could not write $path: $e');
    }
    return;
  }

  messenger
    ?..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(
      content: Text('Exported the AI edit history to ${_baseName(path)}'),
      duration: const Duration(seconds: 4),
    ));
}

String _baseName(String path) {
  final parts =
      path.split(RegExp(r'[\\/]')).where((p) => p.isNotEmpty).toList();
  return parts.isEmpty ? path : parts.last;
}
