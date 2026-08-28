import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_cad/common/color_field_widget.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/field_distribution_api.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/isosurface_histogram.dart';
import 'package:flutter_cad/structure_designer/node_data/isosurface_span_histogram.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor for the `isosurface` node.
///
/// Four groups, in the order a user reaches for them: the isolevel, the two
/// phase colors, the opacity, and the colormap domain.
///
/// **The swap button is not a convenience.** An orbital's global sign is
/// arbitrary — the same calculation run twice can hand back `psi` or `-psi` —
/// so exchanging the two colors is how a user matches a published figure. It
/// swaps the colors and nothing else.
///
/// **The colormap controls are disabled unless `color_field` is wired**,
/// because they describe a domain that nothing reads while the surface is
/// painted per sign. They stay visible rather than hidden so the stored values
/// are still inspectable, and so the panel does not change shape when a wire is
/// made.
///
/// **The level group does not follow that rule, and the difference is the
/// point.** A dormant *colormap domain* is a value nothing reads right now but
/// which a wire would make live again, so it stays visible. A dormant *level
/// coordinate* is not that: `level_mode` picks the **unit** one quantity is
/// expressed in, and switching converts, so at any moment there is exactly one
/// level and one number for it. The row for the coordinate that is not live is
/// therefore **hidden, not greyed** — showing it put a second, differently-valued
/// "Absolute" on screen beside the readout's, which read as a stale field. Both
/// coordinates are in the readout in every mode, so nothing is lost.
///
/// The two stored properties survive as *storage*: with no field wired, or an
/// analytic one, there is no distribution to convert through, and the parked
/// number is the only sane fallback. They are no longer a UI concept.
/// See `doc/design_isosurface_level.md` §The mode is a unit.
///
/// # Continuous controls commit on release, not per tick
///
/// The drag controls here — the fraction slider, the histogram marker, the
/// opacity slider and the colour-domain span handles — write node data **once,
/// when the drag ends**. In between, the dragged value lives in this widget's
/// state and the panel renders from it (§`_preview…`).
///
/// **The fit button is not one of them** and needs no bracket: it is a single
/// write, so it is already a single undo entry.
///
/// Writing per tick is what the design originally called for, and it made the
/// application unusable. Every write goes through
/// `set_isosurface_data` → `refresh_structure_designer_auto`, which re-evaluates
/// and **re-extracts the surface** (marching cubes over the whole grid) on the
/// UI thread, because the FFI is `frb(sync)` — `CAD_INSTANCE` has no
/// synchronization, so there is no worker thread to move it to. At the ~0.1 s a
/// modest field costs, a pointer stream at 60 Hz cannot be serviced: the app
/// freezes and the intermediate surfaces are never even painted.
///
/// Evaluating *during* the drag without freezing needs the whole domain made
/// `Send + Sync`, a real lock around the global, snapshot evaluation and a
/// worker thread — `doc/design_background_evaluation.md`, estimated at 5–8
/// weeks. Until that lands, commit-on-release is the honest behaviour, and the
/// preview is what keeps the drag from feeling dead.
class IsosurfaceEditor extends StatefulWidget {
  /// The fraction slider's window, from `f = 1 - 10^-(0.155 + 2.845 * s)`.
  ///
  /// The travel is linear in the **number of nines**, not in `f`, which is what
  /// puts about 70 % of it in 0.9–0.999 where densities live while still
  /// reaching down past the orbital band. Deliberately **smaller than the
  /// property's legal range** of `0 < f < 1` — see [sliderPositionForFraction].
  ///
  /// The offset cannot be zero: `1 - 10^0` is exactly `0`, which validation
  /// rejects and which means "enclose nothing". `0.155` puts the bottom stop at
  /// the `0.30` the density-viz handoff's flood animation starts from; the span
  /// is then whatever keeps the top stop at `0.999`.
  static const double SLIDER_LOG_OFFSET = 0.155;
  static const double SLIDER_LOG_SPAN = 2.845;

  /// Width of the numeric box on the two level rows, and of their labels.
  static const double LEVEL_BOX_WIDTH = 92.0;
  static const double LEVEL_LABEL_WIDTH = 62.0;

  /// How far inside `(0, 1)` a *derived* fraction is kept.
  ///
  /// `level_fraction` is validated as **strictly** between 0 and 1, but the
  /// fraction a level encloses can legitimately be exactly `1.0` — any level at
  /// or below the smallest sample encloses everything — and exactly `0.0` for a
  /// level above every sample. Neither is a legal property value, so a
  /// conversion or a marker drag that lands on one has to be nudged inside.
  ///
  /// Clamping is sound rather than a fudge at the top: `iso_for_fraction` is a
  /// step function, so a whole interval of fractions resolves to the same
  /// isovalue and `1 - ε` draws the same surface as `1`. At the bottom it is not
  /// — `0` means the surface encloses nothing, and `ε` gives a speck at the
  /// field's maximum. That is the already-broken "level above anything in the
  /// field" state the reference guide warns about, and a speck is a more
  /// legible answer than an empty viewport.
  static const double FRACTION_EPSILON = 1e-6;

  /// A derived fraction, forced into the property's legal open interval.
  static double clampFraction(double f) =>
      f.clamp(FRACTION_EPSILON, 1.0 - FRACTION_EPSILON);

  final BigInt nodeId;
  final APIIsosurfaceData? data;
  final StructureDesignerModel model;

  /// Whether the `color_field` input pin currently has a wire. Gates the
  /// colormap group.
  final bool colorFieldConnected;

  /// Whether the `level` input pin currently has a wire. A wire owns the value,
  /// so the live coordinate's row goes disabled and shows what is drawn — the
  /// same response the colormap group gives to an unwired `color_field`, and the
  /// same one `get_subtitle` gives by returning `None`.
  final bool levelConnected;

  /// The field's value distribution, refetched on every rebuild so a rewire or a
  /// reloaded `.cube` cannot leave a correct-looking histogram belonging to the
  /// previous field. Null only when the node could not be resolved at all.
  final APIValueDistribution? distribution;

  /// The colour field's distribution **over the extracted surface**, refetched
  /// alongside it. Where [distribution] describes the field, this one describes
  /// the picture: it exists only while the node is displayed, because the mesh
  /// it is measured on is produced by the display conversion.
  final APISurfaceValueDistribution? colorDistribution;

  const IsosurfaceEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
    required this.colorFieldConnected,
    required this.levelConnected,
    required this.distribution,
    required this.colorDistribution,
  });

  static double get sliderFractionMin =>
      1.0 - math.pow(10.0, -SLIDER_LOG_OFFSET).toDouble();

  static double get sliderFractionMax =>
      1.0 - math.pow(10.0, -(SLIDER_LOG_OFFSET + SLIDER_LOG_SPAN)).toDouble();

  /// `s -> f`.
  static double fractionForSlider(double s) =>
      1.0 -
      math
          .pow(10.0, -(SLIDER_LOG_OFFSET + SLIDER_LOG_SPAN * s.clamp(0.0, 1.0)))
          .toDouble();

  /// `f -> s`, or null when `f` falls outside the slider's window.
  ///
  /// A value outside `[0.30, 0.999]` arrives easily — from the text format,
  /// the CLI, a `.cnnd` written by an older build. Rendering it at the left stop
  /// would make `0.1` indistinguishable from `0.30` and the next drag would
  /// silently double it, so the slider is **disabled** instead, with its handle
  /// parked at the nearer stop. The box stays live and authoritative.
  static double? sliderPositionForFraction(double f) {
    if (!(f > 0.0) || !(f < 1.0)) return null;
    final s = (-(math.log(1.0 - f) / math.ln10) - SLIDER_LOG_OFFSET) /
        SLIDER_LOG_SPAN;
    if (s < 0.0 || s > 1.0) return null;
    return s;
  }

  @override
  State<IsosurfaceEditor> createState() => _IsosurfaceEditorState();
}

class _IsosurfaceEditorState extends State<IsosurfaceEditor> {
  /// The value under the pointer while a drag is in flight, or null when no
  /// drag is running. Exactly one of the three is non-null at a time.
  ///
  /// While set, the panel renders from these instead of from the node data, and
  /// **nothing is written to the kernel** — see the class doc for why. The drag
  /// end writes once and clears them; the kernel's authoritative numbers arrive
  /// on the rebuild that follows.
  double? _previewFraction;
  double? _previewLevel;
  double? _previewAlpha;

  /// The colour domain under the pointer while a span-handle drag is in flight.
  /// A pair rather than two independents: the handles are the two ends of one
  /// value, and the drag writes both at once so the `max > min` guard can never
  /// be violated by a half-applied edit.
  (double, double)? _previewColorSpan;

  bool get _dragging =>
      _previewFraction != null ||
      _previewLevel != null ||
      _previewAlpha != null ||
      _previewColorSpan != null;

  @override
  void dispose() {
    // A drag whose end never arrives — the panel torn down mid-gesture, the
    // node deselected — would otherwise leave the kernel's coalescing session
    // open and swallow the next node-data undo entry for this node.
    if (_dragging) widget.model.endNodeDataDrag();
    super.dispose();
  }

  void _beginDrag() {
    widget.model.beginNodeDataDrag(widget.nodeId);
  }

  /// Commit whatever the drag arrived at: one write, one evaluation, one undo
  /// entry. Clearing the preview *before* the write matters — the write
  /// notifies listeners synchronously, and the rebuild that follows must read
  /// the node data, not a stale preview.
  void _endDrag() {
    final fraction = _previewFraction;
    final level = _previewLevel;
    final alpha = _previewAlpha;
    final colorSpan = _previewColorSpan;
    setState(() {
      _previewFraction = null;
      _previewLevel = null;
      _previewAlpha = null;
      _previewColorSpan = null;
    });
    if (fraction != null) {
      _update(levelFraction: fraction);
    } else if (level != null) {
      _update(level: level);
    } else if (alpha != null) {
      _update(alpha: alpha);
    } else if (colorSpan != null) {
      _update(colorMin: colorSpan.$1, colorMax: colorSpan.$2);
    }
    widget.model.endNodeDataDrag();
  }

  /// Write the fitted colour domain.
  ///
  /// **One write, no bracket.** The fit is a single edit, so it is already a
  /// single undo entry — the `begin`/`end` coalescing the drags need would only
  /// wrap it in an empty session. It routes through the same
  /// `_update` -> `model.setIsosurfaceData` path the two text fields use, which
  /// is how it becomes undoable without being special.
  void _fitColorRange(APISurfaceValueDistribution distribution) {
    if (!distribution.canFit) return;
    _update(colorMin: distribution.fitMin, colorMax: distribution.fitMax);
  }

  void _update({
    APILevelMode? levelMode,
    double? levelFraction,
    double? level,
    APIVec3? positiveColor,
    APIVec3? negativeColor,
    double? alpha,
    APIColormap? colormap,
    double? colorMin,
    double? colorMax,
  }) {
    final current = widget.data;
    if (current == null) return;
    widget.model.setIsosurfaceData(
      widget.nodeId,
      APIIsosurfaceData(
        levelMode: levelMode ?? current.levelMode,
        levelFraction: levelFraction ?? current.levelFraction,
        level: level ?? current.level,
        positiveColor: positiveColor ?? current.positiveColor,
        negativeColor: negativeColor ?? current.negativeColor,
        alpha: alpha ?? current.alpha,
        colormap: colormap ?? current.colormap,
        colorMin: colorMin ?? current.colorMin,
        colorMax: colorMax ?? current.colorMax,
      ),
    );
  }

  void _swapColors() {
    final current = widget.data;
    if (current == null) return;
    _update(
      positiveColor: current.negativeColor,
      negativeColor: current.positiveColor,
    );
  }

  /// Switching the mode **converts**: the coordinate the new mode makes live is
  /// filled from the level currently drawn, so `Absolute ⇄ Fraction` never moves
  /// the surface. The mode is a unit, not a second parked value.
  ///
  /// Two exceptions, both deliberate:
  ///
  /// - **Switching *to* `Auto` converts nothing.** Auto re-derives the level
  ///   from the field on every evaluation and consults neither stored number, so
  ///   this is the one switch that *does* move the surface — back to what the
  ///   field itself suggests. That is what picking `Auto` asks for.
  /// - **A stored number that already describes the current surface is kept
  ///   verbatim.** `iso → f → iso` is not a bijection: `iso_for_fraction` can
  ///   only return a stored sample, so converting unconditionally would turn a
  ///   typed `0.002` into `0.00200034…` on a round trip through fraction mode.
  ///   The kernel answers whether the parked number still encloses what the
  ///   surface encloses (`storedLevelMatches` / `storedFractionMatches`); when
  ///   it does, the user's constant survives untouched.
  ///
  /// With no field wired — or an analytic one — there is no resolved value to
  /// convert from, and the parked number is used as-is. That is the whole reason
  /// the node keeps two properties in storage.
  void _changeMode(APILevelMode mode) {
    final current = widget.data;
    if (current == null || mode == current.levelMode) return;
    final d = widget.distribution;
    switch (mode) {
      case APILevelMode.fraction:
        final converted =
            (d != null && d.hasResolvedFraction && !d.storedFractionMatches)
                ? d.resolvedFraction
                : current.levelFraction;
        _update(levelMode: mode, levelFraction: converted);
      case APILevelMode.absolute:
        final converted = (d != null &&
                d.hasResolvedLevel &&
                d.resolvedLevel > 0 &&
                !d.storedLevelMatches)
            ? d.resolvedLevel
            : current.level;
        _update(levelMode: mode, level: converted);
      case APILevelMode.auto:
        _update(levelMode: mode);
    }
  }

  /// One line, identical in all three modes — same wording, same position, only
  /// the numbers change. **It is the group's answer of record**, not an aid: it
  /// is the one place both coordinates appear in every mode, which is what makes
  /// hiding the non-live row lossless.
  ///
  /// **`encloses X% of ∫|v|` is load-bearing wording, never "% of the electron
  /// density".** On an orbital amplitude the conventionally enclosed quantity is
  /// `∫|psi|²`, so the friendlier paraphrase would be false. No unit suffix
  /// either: nothing here converts field values or assumes what they are, and
  /// two of the fields this must serve (ELF, RDG) are dimensionless.
  static String _readoutLine(double? level, double? fraction) {
    if (level == null) return '';
    final magnitude = '|v| = ${formatNatural(level, 5)}';
    if (fraction == null) return magnitude;
    return '$magnitude  ·  encloses '
        '${(fraction * 100).toStringAsFixed(1)}% of ∫|v|';
  }

  /// The `(isovalue, fraction)` pair the readout and the marker should show.
  ///
  /// Normally the kernel's resolved pair, straight off the distribution. **While
  /// a level drag is in flight it is the pointer's value plus the *other*
  /// coordinate previewed off the cumulative curve** — the kernel has not been
  /// asked yet, and its answer only arrives on release. The preview is accurate
  /// to one histogram bin; the exact pair replaces it the moment the drag ends.
  (double?, double?) _shownPair(APIValueDistribution? d) {
    final previewLevel = _previewLevel;
    final previewFraction = _previewFraction;
    if (d != null && previewLevel != null) {
      return (
        previewLevel,
        IsosurfaceHistogram.previewFractionForMagnitude(d, previewLevel),
      );
    }
    if (d != null && previewFraction != null) {
      return (
        IsosurfaceHistogram.previewMagnitudeForFraction(d, previewFraction),
        previewFraction,
      );
    }
    if (d == null) return (null, null);
    return (
      d.hasResolvedLevel ? d.resolvedLevel : null,
      d.hasResolvedFraction ? d.resolvedFraction : null,
    );
  }

  /// The one caption under the level group — **at most a line**.
  ///
  /// The panel explains only what is non-obvious *and* sayable in a clause: the
  /// two-lobe consequence of a magnitude, what a fraction is a fraction *of*,
  /// and that auto silently re-picks. Everything else — why the swap button
  /// exists, why the colour domain is not fitted, what the histogram's axes
  /// mean, why a drag commits on release — belongs in the node description (the
  /// ⓘ button, authored as Markdown in `nodes/isosurface.rs`) and the reference
  /// guide. A property panel that argues with the reader is one nobody reads.
  String _levelCaption(APIValueDistribution? d, APILevelMode mode) {
    if (widget.levelConnected && mode != APILevelMode.auto) {
      return 'Driven by the `level` wire.';
    }
    if (d == null) return '';
    switch (d.state) {
      case APIDistributionState.noField:
        return 'Wire a field into `field`.';
      case APIDistributionState.analyticField:
      case APIDistributionState.notEvaluated:
        // The kernel's own wording: for `notEvaluated` this is the upstream
        // error, which has to reach the user verbatim.
        return d.message;
      case APIDistributionState.available:
        switch (mode) {
          case APILevelMode.auto:
            return 'Re-picked from the field on every evaluation — switch mode '
                'to freeze it.';
          case APILevelMode.fraction:
            return 'Share of the field\'s total |v| inside the surface, not of '
                'volume.';
          case APILevelMode.absolute:
            return 'Drawn at +level and -level, so a signed field shows both '
                'lobes.';
        }
    }
  }

  /// The fit button's tooltip — the only place the *why* of the fit is sayable
  /// in the panel, and the only place a refusal can explain itself.
  ///
  /// The button is beside a heading, so there is no room for a caption; a
  /// tooltip costs no layout, which §The panel says a clause makes the
  /// governing constraint. The paragraphs — what the two fits do, why the
  /// vertex floor exists, the ramp-orientation caveat — are in the node
  /// description.
  String _fitTooltip(APISurfaceValueDistribution? d) {
    if (!widget.colorFieldConnected) {
      return 'Fit the colour range.\n'
          'Wire `color_field` first — nothing reads the range until you do.';
    }
    if (d == null) return 'Fit the colour range to the surface.';
    if (!d.canFit) {
      return 'Fit the colour range to the surface.\n'
          '${d.fitBlockedReason.isEmpty ? d.message : d.fitBlockedReason}';
    }
    final shape = d.isSigned
        ? 'Symmetric, so the ramp\'s white stays on zero'
        : 'The 2nd to 98th percentile';
    return 'Fit the colour range to this surface.\n'
        '$shape: ${formatNatural(d.fitMin, 3)} … ${formatNatural(d.fitMax, 3)}.\n'
        'Measured on the surface, not on the whole box — the field\'s volume '
        'extrema are one or two orders of magnitude wider.';
  }

  /// The one caption under the colour group — at most a line, same rule as the
  /// level group's.
  ///
  /// **Empty in every empty state**, because the plot's own box already carries
  /// that sentence and the panel must not say a thing twice.
  String _colorCaption(APISurfaceValueDistribution? d) {
    if (d == null || d.state != APISurfaceDistributionState.available) {
      return '';
    }
    return d.isSigned
        ? 'Fit is symmetric, so the ramp\'s white sits on zero.'
        : 'Fit spans the 2nd–98th percentile on the surface.';
  }

  @override
  Widget build(BuildContext context) {
    final current = widget.data;
    if (current == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final captionStyle = Theme.of(context).textTheme.bodySmall;
    final headingStyle = Theme.of(context).textTheme.titleSmall;
    final d = widget.distribution;
    final hasField = d != null && d.state == APIDistributionState.available;

    // **Exactly one numeric row is on screen, and it is the live coordinate's.**
    // Under `Auto` neither is live, so there is no row at all — the readout
    // below carries both numbers in every mode, which is what makes hiding the
    // other one lossless.
    final isAuto = current.levelMode == APILevelMode.auto;
    final rowsEditable = !isAuto && !widget.levelConnected && hasField;
    // A wire on `level` owns the value, so the row shows what is *drawn* rather
    // than a stored number nothing is reading.
    final showResolved = widget.levelConnected;
    final (shownLevel, shownFraction) = _shownPair(d);

    final fractionShown = showResolved
        ? (shownFraction ?? 0.0)
        : (_previewFraction ?? current.levelFraction);
    final levelShown =
        showResolved ? (shownLevel ?? 0.0) : (_previewLevel ?? current.level);
    // Blank rather than wrong: with no field there is no resolved pair, and the
    // parked number is not what would be used either.
    final fractionBlank = showResolved ? shownFraction == null : !hasField;
    final levelBlank = showResolved ? shownLevel == null : !hasField;
    final alphaShown = _previewAlpha ?? current.alpha;

    final sliderPosition =
        IsosurfaceEditor.sliderPositionForFraction(fractionShown);
    final readout = _readoutLine(shownLevel, shownFraction);

    final colorDistribution = widget.colorDistribution;
    final colorMinShown = _previewColorSpan?.$1 ?? current.colorMin;
    final colorMaxShown = _previewColorSpan?.$2 ?? current.colorMax;
    // The fit needs a wire *and* a surface: without the wire nothing reads the
    // domain, and without the surface there is nothing to measure it on.
    final canFit = widget.colorFieldConnected &&
        colorDistribution != null &&
        colorDistribution.canFit;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const NodeEditorHeader(
              title: 'Isosurface Properties',
              nodeTypeName: 'isosurface',
            ),
            const SizedBox(height: 8),
            Text('Level', style: headingStyle),
            const SizedBox(height: 4),
            DropdownButtonFormField<APILevelMode>(
              key: const Key('isosurface_level_mode'),
              decoration: const InputDecoration(
                labelText: 'Mode',
                border: OutlineInputBorder(),
                isDense: true,
                contentPadding:
                    EdgeInsets.symmetric(horizontal: 8, vertical: 8),
              ),
              value: current.levelMode,
              items: const [
                DropdownMenuItem(
                  value: APILevelMode.auto,
                  child: Text('Auto'),
                ),
                DropdownMenuItem(
                  value: APILevelMode.absolute,
                  child: Text('Absolute'),
                ),
                DropdownMenuItem(
                  value: APILevelMode.fraction,
                  child: Text('Fraction'),
                ),
              ],
              // Live even with no field wired: the mode is the user's, not the
              // field's.
              onChanged: (value) {
                if (value != null) _changeMode(value);
              },
            ),
            // Exactly one row, for the live coordinate. `Auto` gets none.
            if (!isAuto) const SizedBox(height: 8),
            if (current.levelMode == APILevelMode.fraction)
              Row(
                children: [
                  SizedBox(
                    width: IsosurfaceEditor.LEVEL_LABEL_WIDTH,
                    child: Text(
                      'Fraction',
                      style: captionStyle?.copyWith(
                        color: rowsEditable
                            ? null
                            : Theme.of(context).disabledColor,
                      ),
                    ),
                  ),
                  Expanded(
                    child: Slider(
                      key: const Key('isosurface_fraction_slider'),
                      value: sliderPosition ??
                          (fractionShown <= IsosurfaceEditor.sliderFractionMin
                              ? 0.0
                              : 1.0),
                      divisions: 100,
                      label: fractionShown.toStringAsFixed(4),
                      // Disabled outside its window rather than lying about
                      // where the value sits.
                      onChanged: rowsEditable && sliderPosition != null
                          ? (value) => setState(() => _previewFraction =
                              IsosurfaceEditor.fractionForSlider(value))
                          : null,
                      onChangeStart: (value) {
                        _beginDrag();
                        setState(() => _previewFraction =
                            IsosurfaceEditor.fractionForSlider(value));
                      },
                      onChangeEnd: (_) => _endDrag(),
                    ),
                  ),
                  SizedBox(
                    width: IsosurfaceEditor.LEVEL_BOX_WIDTH,
                    child: FloatInput(
                      label: '',
                      value: fractionShown,
                      blank: fractionBlank,
                      enabled: rowsEditable,
                      onChanged: (value) => _update(levelFraction: value),
                    ),
                  ),
                ],
              ),
            if (current.levelMode == APILevelMode.absolute)
              Row(
                children: [
                  SizedBox(
                    width: IsosurfaceEditor.LEVEL_LABEL_WIDTH,
                    child: Text(
                      'Absolute',
                      style: captionStyle?.copyWith(
                        color: rowsEditable
                            ? null
                            : Theme.of(context).disabledColor,
                      ),
                    ),
                  ),
                  const Spacer(),
                  SizedBox(
                    width: IsosurfaceEditor.LEVEL_BOX_WIDTH,
                    child: FloatInput(
                      label: '',
                      value: levelShown,
                      blank: levelBlank,
                      enabled: rowsEditable,
                      onChanged: (value) => _update(level: value),
                    ),
                  ),
                ],
              ),
            const SizedBox(height: 8),
            // Always present, so the group keeps its height when there is
            // nothing resolved to report.
            Tooltip(
              message: 'The fraction is the share of the field\'s total '
                  'integrated magnitude that lies inside the surface. It is '
                  'not a share of volume.',
              child: Text(readout.isEmpty ? '—' : readout, style: captionStyle),
            ),
            if (isAuto && d != null && d.autoBasis.isNotEmpty)
              Text('auto: ${d.autoBasis}', style: captionStyle),
            const SizedBox(height: 8),
            if (d != null)
              IsosurfaceHistogram(
                distribution: d,
                // Inert under `Auto`, where nothing in the group is editable.
                draggable: rowsEditable,
                // Nothing has been written yet mid-drag, so the distribution's
                // resolved level still describes the pre-drag surface.
                markerMagnitude: _dragging ? shownLevel : null,
                onDragStart: _beginDrag,
                onDragUpdate: (magnitude, fraction) {
                  // The marker edits whichever property is live — the isovalue
                  // directly in `Absolute`, the enclosed fraction in `Fraction`.
                  if (current.levelMode == APILevelMode.absolute) {
                    setState(() => _previewLevel = magnitude);
                  } else if (current.levelMode == APILevelMode.fraction) {
                    setState(() => _previewFraction =
                        IsosurfaceEditor.clampFraction(fraction));
                  }
                },
                onDragEnd: _endDrag,
              ),
            const SizedBox(height: 4),
            Text(_levelCaption(d, current.levelMode), style: captionStyle),
            const SizedBox(height: 16),
            Row(
              children: [
                Text('Phase colors', style: headingStyle),
                const Spacer(),
                Tooltip(
                  message: 'Swap the phase colors.\n'
                      "An orbital's overall sign is arbitrary, so this is how "
                      'you match a published figure.',
                  child: IconButton(
                    key: const Key('isosurface_swap_colors'),
                    icon: const Icon(Icons.swap_vert),
                    iconSize: 18,
                    visualDensity: VisualDensity.compact,
                    onPressed: _swapColors,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text('Positive (+level)', style: captionStyle),
            const SizedBox(height: 4),
            ColorFieldWidget(
              keyPrefix: 'isosurface_positive_color',
              value: current.positiveColor,
              onChanged: (value) => _update(positiveColor: value),
            ),
            const SizedBox(height: 8),
            Text('Negative (-level)', style: captionStyle),
            const SizedBox(height: 4),
            ColorFieldWidget(
              keyPrefix: 'isosurface_negative_color',
              value: current.negativeColor,
              onChanged: (value) => _update(negativeColor: value),
            ),
            const SizedBox(height: 16),
            Text('Opacity', style: headingStyle),
            Row(
              children: [
                Expanded(
                  child: Slider(
                    value: alphaShown.clamp(0.0, 1.0),
                    min: 0.0,
                    max: 1.0,
                    divisions: 100,
                    label: alphaShown.toStringAsFixed(2),
                    onChanged: (value) =>
                        setState(() => _previewAlpha = value.clamp(0.0, 1.0)),
                    onChangeStart: (value) {
                      _beginDrag();
                      setState(() => _previewAlpha = value.clamp(0.0, 1.0));
                    },
                    onChangeEnd: (_) => _endDrag(),
                  ),
                ),
                SizedBox(
                  width: 80,
                  child: FloatInput(
                    label: '',
                    value: alphaShown,
                    onChanged: (value) => _update(alpha: value.clamp(0.0, 1.0)),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                Text(
                  'Colormap',
                  style: headingStyle?.copyWith(
                    color: widget.colorFieldConnected
                        ? null
                        : Theme.of(context).disabledColor,
                  ),
                ),
                const Spacer(),
                // Beside the heading, the same placement as the swap button
                // beside "Phase colors" — the panel's existing idiom for an
                // action belonging to a group. The tooltip is where the *why*
                // lives: it costs no layout, which is what makes it the right
                // home for a sentence the panel itself has no room for.
                Tooltip(
                  message: _fitTooltip(colorDistribution),
                  child: IconButton(
                    key: const Key('isosurface_fit_color_range'),
                    icon: const Icon(Icons.straighten),
                    iconSize: 18,
                    visualDensity: VisualDensity.compact,
                    onPressed:
                        canFit ? () => _fitColorRange(colorDistribution) : null,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            DropdownButtonFormField<APIColormap>(
              key: const Key('isosurface_colormap'),
              decoration: const InputDecoration(
                border: OutlineInputBorder(),
                isDense: true,
                contentPadding:
                    EdgeInsets.symmetric(horizontal: 8, vertical: 8),
              ),
              value: current.colormap,
              items: const [
                DropdownMenuItem(
                  value: APIColormap.blueWhiteRed,
                  child: Text('Blue - White - Red'),
                ),
              ],
              onChanged: widget.colorFieldConnected
                  ? (value) {
                      if (value != null) _update(colormap: value);
                    }
                  : null,
            ),
            const SizedBox(height: 8),
            // **These two rows stay visible and greyed. They are not hidden.**
            // The level group hides its dormant row, and that reversal must not
            // be generalised: a dormant colour domain is a value nothing reads
            // right now *but which a wire would make live again unchanged*, so
            // it stays inspectable and the panel keeps its shape when the wire
            // is made. A dormant level coordinate was the same quantity in the
            // other unit, which is why hiding it lost nothing.
            Row(
              children: [
                Expanded(
                  child: FloatInput(
                    label: 'Range min',
                    value: colorMinShown,
                    enabled: widget.colorFieldConnected,
                    onChanged: (value) => _update(colorMin: value),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: FloatInput(
                    label: 'Range max',
                    value: colorMaxShown,
                    enabled: widget.colorFieldConnected,
                    onChanged: (value) => _update(colorMax: value),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            if (colorDistribution != null)
              IsosurfaceSpanHistogram(
                distribution: colorDistribution,
                colorMin: colorMinShown,
                colorMax: colorMaxShown,
                draggable: widget.colorFieldConnected &&
                    colorDistribution.state ==
                        APISurfaceDistributionState.available,
                onDragStart: _beginDrag,
                onDragUpdate: (min, max) =>
                    setState(() => _previewColorSpan = (min, max)),
                onDragEnd: _endDrag,
                // With no wire the scene may report "display the node", which is
                // true but not the thing to do about it.
                emptyMessage: widget.colorFieldConnected
                    ? null
                    : 'Wire `color_field` to paint the surface by a second '
                        'quantity.',
              ),
            const SizedBox(height: 4),
            Text(_colorCaption(colorDistribution), style: captionStyle),
          ],
        ),
      ),
    );
  }
}
