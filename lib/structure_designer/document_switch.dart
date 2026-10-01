/// Changing the active document safely (`doc/design_multiple_documents.md`
/// D11).
///
/// Input fields commit their value when they **lose focus**
/// (`lib/inputs/float_input.dart` and its siblings). That write goes through
/// the model to the API, which acts on whichever document is active *at that
/// moment*. If a switch ran first, a value typed into document A would be
/// written into document B — onto whatever node there has the same network
/// name and node id, which almost always exists. A silent edit of the wrong
/// design.
///
/// So every change of the active document — activate, new, open, close of the
/// active tab — runs its API call through [DocumentSwitcher.run], which
///
/// 1. unfocuses the primary focus,
/// 2. waits for the end of the current frame, by which time the focus
///    listeners have run (focus changes are applied in a microtask) and their
///    writes have reached the *outgoing* document, and
/// 3. only then runs the action.
///
/// Switches are **queued**: one requested while another is still waiting for
/// its frame runs after it, never interleaved. A relative target (`Ctrl+Tab`'s
/// "next tab") must therefore be resolved inside the action, when it runs, not
/// when it is requested.
///
/// The switcher takes its action as a plain callback and never reaches the
/// API itself, so its ordering is testable without the Rust library
/// (`test/document_switch_test.dart`).
library;

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

class DocumentSwitcher {
  DocumentSwitcher({
    Future<void> Function()? waitForFrame,
    void Function()? unfocus,
  })  : _waitForFrame = waitForFrame ?? _endOfFrame,
        _unfocus = unfocus ?? _unfocusPrimary;

  final Future<void> Function() _waitForFrame;
  final void Function() _unfocus;

  /// The last queued switch; the next one chains onto it.
  Future<void> _tail = Future<void>.value();

  int _pending = 0;

  /// True while a switch is queued or waiting for its frame.
  bool get isSwitching => _pending > 0;

  /// The end of the current frame — or, while frames are disabled (the
  /// window is minimized or hidden), the end of the current event: no frame
  /// would ever come, and a CLI `load` sent to a minimized window would hang.
  /// Focus changes are applied in a microtask, so the commit has happened by
  /// then either way.
  static Future<void> _endOfFrame() {
    final binding = SchedulerBinding.instance;
    if (binding.framesEnabled) return binding.endOfFrame;
    return Future<void>.delayed(Duration.zero);
  }

  static void _unfocusPrimary() =>
      FocusManager.instance.primaryFocus?.unfocus();

  /// Runs [action] once every pending edit has been committed to the document
  /// that is active now, and after every switch requested before it.
  ///
  /// Everything [action] does happens in one synchronous stretch, so nothing
  /// else (another switch, a CLI request resuming at an `await`) can run
  /// between its first API call and its last.
  Future<T> run<T>(T Function() action) {
    _pending++;
    final result = _tail.then((_) async {
      _unfocus();
      await _waitForFrame();
      return action();
    });
    _tail = result.then<void>((_) {}, onError: (Object _) {}).whenComplete(() {
      _pending--;
    });
    return result;
  }
}
