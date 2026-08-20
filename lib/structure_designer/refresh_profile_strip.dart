/// The always-on refresh status strip: a ~20 px bar at the bottom of the
/// shell that says where the last refresh's time went
/// (`doc/design_eval_profiling.md` D8a — Phase 1).
///
/// ```
/// refresh 1.83 s — eval 1.61 · tess 0.15 · gpu 0.02 · view 0.05   (Partial)
/// refresh 0.04 s — eval —    · tess 0.02 · gpu 0.01 · view 0.01   (Lightweight)
/// ```
///
/// Two properties are load-bearing rather than cosmetic:
///
/// - **It listens to `model.refreshProfile`, not to the model itself.** A
///   gadget drag marks a lightweight refresh on every pointer move, and an
///   always-on measurement widget that forced a model-wide rebuild per tick
///   would be a self-inflicted regression measured by itself. The notifier is
///   written without `notifyListeners()`, and lightweight ticks are throttled
///   to ~5 Hz on the model side.
/// - **A lightweight refresh renders `eval —`, never `eval 0.00`.** It runs no
///   evaluation pass at all (`evalMs == null`), and printing a zero there
///   would read as "evaluation is free" — the single most misleading thing
///   this strip could say.
library;

import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/profiling_api.dart';
import 'package:provider/provider.dart';
import 'structure_designer_model.dart';

/// Height of the strip, in logical pixels.
const double REFRESH_PROFILE_STRIP_HEIGHT = 20.0;

/// Bottom-docked, always-on refresh phase readout.
class RefreshProfileStrip extends StatelessWidget {
  const RefreshProfileStrip({super.key});

  @override
  Widget build(BuildContext context) {
    // `read`, not `watch`: the strip must not rebuild with the model. The
    // ValueListenableBuilder below is its only rebuild trigger.
    final notifier = context.read<StructureDesignerModel>().refreshProfile;
    return Container(
      height: REFRESH_PROFILE_STRIP_HEIGHT,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      decoration: const BoxDecoration(
        color: Color(0xFF2A2A2A),
        border: Border(top: BorderSide(color: Colors.black54, width: 1)),
      ),
      alignment: Alignment.centerLeft,
      child: ValueListenableBuilder<RefreshProfileSample?>(
        valueListenable: notifier,
        builder: (context, sample, _) => Text(
          _formatSample(sample),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(
            color: Colors.white54,
            fontSize: 11,
            fontFeatures: [FontFeature.tabularFigures()],
          ),
        ),
      ),
    );
  }
}

/// Renders one sample as the single line the strip shows. Kept a free function
/// so it is trivially testable without a widget tree.
String _formatSample(RefreshProfileSample? sample) {
  if (sample == null) return 'refresh —';
  final view = _seconds(sample.viewMs);
  final kernel = sample.kernel;
  if (kernel == null) {
    // A view rebuild with no kernel refresh behind it (only possible before
    // the session's first refresh). Report what was actually measured.
    return 'refresh —  ·  view $view';
  }
  final eval = kernel.evalMs == null ? '—' : _seconds(kernel.evalMs!);
  final total = _seconds(kernel.totalMs + sample.viewMs);
  return 'refresh $total s — eval $eval · tess ${_seconds(kernel.tessellateMs)}'
      ' · gpu ${_seconds(kernel.gpuUploadMs)} · view $view'
      '   (${_modeLabel(kernel.mode)})';
}

String _seconds(double ms) => (ms / 1000.0).toStringAsFixed(2);

String _modeLabel(APIRefreshMode mode) => switch (mode) {
      APIRefreshMode.full => 'Full',
      APIRefreshMode.partial => 'Partial',
      APIRefreshMode.lightweight => 'Lightweight',
    };
