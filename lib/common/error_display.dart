/// Shared surfaces for showing an error message to the user — and for getting
/// it back out again (issue #359: "make UI side error messages cursor
/// markable").
///
/// The rule this file encodes: **transient surfaces get a copy _action_,
/// persistent surfaces get real _selection_ (plus a copy button on top).**
///
/// Flutter's `Tooltip` and `SnackBar` are overlay entries that vanish on
/// pointer-exit / on a timer, and a tooltip does not accept hit-testing — so
/// there is no drag-select to be had on either, no matter what widget is put
/// inside them. Making those "cursor markable" is not a styling fix, it is a
/// replacement of the affordance. So they get a **Copy** button instead, and
/// only the surfaces that hold still ([ErrorBanner], the error dialogs, the
/// error picker rows) are made selectable.
///
/// Three affordances live here:
/// - [ErrorBanner] — the persistent inline red box used by the property
///   editors and dialogs. Selectable text + a copy button. It replaces ~8
///   hand-rolled red `Container`s that had drifted apart in styling.
/// - [showErrorSnackBar] — the transient bottom toast, with a `Copy` action
///   and a longer-than-default duration so there is time to hit it.
/// - [copyTextToClipboard] / [CopyTextButton] — the primitives the above, the
///   node context menu, and the panel's error picker all share.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';

/// Puts [text] on the clipboard and confirms with a short floating snackbar,
/// matching the transient-confirmation style used by undo/redo and quick save.
///
/// The messenger is resolved **before** the `await` — after it, `context` may
/// be defunct (a copy button inside a popup menu routinely outlives its route).
Future<void> copyTextToClipboard(
  BuildContext context,
  String text, {
  String confirmation = 'Copied to clipboard',
}) async {
  final messenger = ScaffoldMessenger.maybeOf(context);
  await Clipboard.setData(ClipboardData(text: text));
  messenger
    ?..hideCurrentSnackBar()
    ..showSnackBar(
      SnackBar(
        content: Text(confirmation),
        duration: const Duration(seconds: 2),
        behavior: SnackBarBehavior.floating,
        width: 300,
      ),
    );
}

/// A small copy-to-clipboard icon button, sized to sit inside a dense row
/// (an error banner, a popup-menu row) without changing its height.
class CopyTextButton extends StatelessWidget {
  const CopyTextButton({
    super.key,
    required this.text,
    this.tooltip = 'Copy message',
    this.confirmation = 'Copied to clipboard',
    this.size = 14,
    this.color,
    this.onCopied,
  });

  /// The text placed on the clipboard.
  final String text;
  final String tooltip;
  final String confirmation;
  final double size;
  final Color? color;

  /// Extra work after the copy — used by the error picker, whose rows must not
  /// pop their menu route when the copy button (rather than the row) is hit.
  final VoidCallback? onCopied;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: tooltip,
      child: InkWell(
        borderRadius: BorderRadius.circular(4),
        onTap: () {
          copyTextToClipboard(context, text, confirmation: confirmation);
          onCopied?.call();
        },
        child: Padding(
          padding: const EdgeInsets.all(2),
          child: Icon(Icons.copy, size: size, color: color ?? Colors.grey),
        ),
      ),
    );
  }
}

/// The transient error toast: red, with a **Copy** action.
///
/// Use this for every *failure* message; the plain informational snackbars
/// ("Saved foo.cnnd", "Undo: …") deliberately keep their own neutral styling
/// and short duration — there is nothing there worth reporting.
///
/// The duration is longer than the Material default because the point of the
/// action is that the user gets to press it. Tapping `Copy` dismisses the bar,
/// which is fine: the text is already on the clipboard.
void showErrorSnackBar(
  BuildContext context,
  String message, {
  Duration duration = const Duration(seconds: 6),
}) {
  final messenger = ScaffoldMessenger.maybeOf(context);
  if (messenger == null) return;
  showErrorSnackBarOn(messenger, message, duration: duration);
}

/// [showErrorSnackBar] for callers that captured their `ScaffoldMessengerState`
/// before an async gap — the modal-placard Execute path does this because its
/// `BuildContext` is routinely defunct by the time the failure is known.
void showErrorSnackBarOn(
  ScaffoldMessengerState messenger,
  String message, {
  Duration duration = const Duration(seconds: 6),
}) {
  messenger
    ..hideCurrentSnackBar()
    ..showSnackBar(_copyableSnackBar(
      message,
      backgroundColor: Colors.red.shade700,
      duration: duration,
    ));
}

/// A neutral transient message that nonetheless carries error text worth
/// keeping — today the `Root cause: … — <error>` flash a cross-network jump
/// leaves behind. Same `Copy` action as [showErrorSnackBar], without claiming
/// that something just failed.
void showCopyableSnackBar(
  BuildContext context,
  String message, {
  Duration duration = const Duration(seconds: 6),
}) {
  final messenger = ScaffoldMessenger.maybeOf(context);
  if (messenger == null) return;
  messenger
    ..hideCurrentSnackBar()
    ..showSnackBar(_copyableSnackBar(message, duration: duration));
}

SnackBar _copyableSnackBar(
  String message, {
  Color? backgroundColor,
  required Duration duration,
}) {
  return SnackBar(
    content: Text(message),
    backgroundColor: backgroundColor,
    duration: duration,
    action: SnackBarAction(
      label: 'Copy',
      textColor: Colors.white,
      onPressed: () => Clipboard.setData(ClipboardData(text: message)),
    ),
  );
}

/// The modal failure dialog: **selectable** message text, a scroll region for
/// long kernel messages, and a `Copy` action next to `OK`.
///
/// Replaces five hand-rolled `showDraggableAlertDialog(content: Text(...))`
/// call sites (Load / Save / Import errors, delete-refused, folder-create
/// refused) that were the single most report-worthy messages in the app and
/// the least extractable.
///
/// `Copy` deliberately leaves the dialog open — the confirmation snackbar
/// appears behind the (transparent) dialog barrier, and dismissal stays the
/// user's decision.
Future<void> showErrorDialog({
  required BuildContext context,
  required String title,
  required String message,
  double width = 460,
}) {
  final maxHeight = MediaQuery.of(context).size.height * 0.5;
  return showDraggableAlertDialog(
    context: context,
    width: width,
    title: Text(title),
    content: ConstrainedBox(
      constraints: BoxConstraints(maxHeight: maxHeight),
      child: SingleChildScrollView(
        child: SelectableText(message),
      ),
    ),
    actions: [
      TextButton.icon(
        onPressed: () => copyTextToClipboard(context, message),
        icon: const Icon(Icons.copy, size: 16),
        label: const Text('Copy'),
      ),
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('OK'),
      ),
    ],
  );
}

/// The persistent inline error box: an icon, **selectable** message text, and a
/// copy button.
///
/// This is the shared replacement for the red `Container` + plain `Text` that
/// each property editor used to hand-roll (`expr`, `switch`, `array`,
/// `atom_edit`, the importers, the network-description editor, the library
/// import dialog). Besides making the text selectable it removes the styling
/// drift between them.
///
/// [warning] switches to the amber palette for advisory messages that are not
/// failures.
class ErrorBanner extends StatelessWidget {
  const ErrorBanner({
    super.key,
    required this.message,
    this.warning = false,
    this.icon,
  });

  final String message;
  final bool warning;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final background = warning ? Colors.orange.shade100 : scheme.errorContainer;
    final border = warning ? Colors.orange.shade700 : scheme.error;
    final foreground =
        warning ? Colors.orange.shade900 : scheme.onErrorContainer;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: background,
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: border),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 1),
            child: Icon(
              icon ??
                  (warning ? Icons.warning_amber_rounded : Icons.error_outline),
              size: 14,
              color: foreground,
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: SelectableText(
              message,
              style: TextStyle(color: foreground, fontSize: 12),
            ),
          ),
          const SizedBox(width: 4),
          CopyTextButton(text: message, color: foreground),
        ],
      ),
    );
  }
}
