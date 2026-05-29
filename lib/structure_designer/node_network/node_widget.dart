import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_painter.dart'
    show
        GRID_MINOR_SPACING,
        GRID_MAJOR_SPACING,
        GRID_MINOR_COLOR,
        GRID_MAJOR_COLOR;
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/factor_into_subnetwork_dialog.dart';

/// Key constants for node widget testing
class NodeWidgetKeys {
  /// Returns a Key for a node widget container by its scope chain + ID.
  ///
  /// The scope chain matters because body nodes render as **siblings** of
  /// top-level nodes in the same `Stack`, and per-body `next_node_id` counters
  /// mean a body node and a top-level node can share a numeric id. Keying by
  /// bare id would produce duplicate keys among siblings — which Flutter
  /// mis-reconciles (stale/orphaned widgets that linger across rebuilds,
  /// network switches, and zoom). Empty scope keeps the original top-level key
  /// format unchanged.
  static Key nodeWidget(BigInt id, {List<BigInt> scopeChain = const []}) => Key(
      'node_widget_${scopeChain.isEmpty ? '' : '${scopeChain.join('_')}_'}$id');

  /// Returns a Key for a node's visibility toggle button by its ID
  static Key visibilityButton(BigInt id) => Key('node_visibility_$id');

  /// Returns a Key for an input pin by node ID and pin index
  static Key inputPin(BigInt nodeId, int pinIndex) =>
      Key('node_${nodeId}_input_$pinIndex');

  /// Returns a Key for an output pin by node ID and pin index
  static Key outputPin(BigInt nodeId, [int pinIndex = 0]) =>
      Key('node_${nodeId}_output_$pinIndex');

  /// Returns a Key for an output pin visibility button by node ID and pin index
  static Key outputPinVisibility(BigInt nodeId, int pinIndex) =>
      Key('node_${nodeId}_output_vis_$pinIndex');

  /// Returns a Key for the function pin by node ID
  static Key functionPin(BigInt nodeId) => Key('node_${nodeId}_function');
}

// Pin appearance constants
const double PIN_SIZE = 14.0;
const double PIN_BORDER_WIDTH = 5.0;
const double PIN_HIT_AREA_WIDTH = 24.0; // Larger hit area for easier dragging
const double PIN_HIT_AREA_HEIGHT = 22.0;

// Node appearance constants
const Color NODE_BACKGROUND_COLOR = Color(0xFF212121); // Colors.grey[900]
const Color NODE_COLOR_ACTIVE = Color(0xFFD84315); // Active node border & title
const Color NODE_COLOR_SELECTED =
    Color(0xFFE08000); // Selected node border & title
const Color NODE_BORDER_COLOR_NORMAL = Colors.blueAccent;
const Color NODE_BORDER_COLOR_ERROR = Colors.red;
const double NODE_BORDER_WIDTH_ACTIVE = 3.0;
const double NODE_BORDER_WIDTH_SELECTED = 2.0;
const double NODE_BORDER_WIDTH_NORMAL = 2.0;
const double NODE_BORDER_RADIUS = 8.0;
const Color NODE_TITLE_COLOR_NORMAL = Color(0xFF37474F); // Colors.blueGrey[800]
const Color NODE_TITLE_COLOR_RETURN = Color(0xFF0D47A1); // Dark blue
const Color NODE_TITLE_COLOR_PARAMETER = Color(0xFF1B5E20); // Dark green

// HOF / closure body region appearance. The body renders as a light "canvas"
// surface matching the main network background, so the embedding hierarchy
// reads consistently — the top level and every nested body share the same
// figure/ground instead of inverting to dark from level 2 down. Body nodes
// stay dark-on-light (identical to the top-level look), and the inner-edge
// decorations below are dark-on-light so they survive on the light fill.
//
// The fill itself is read at build time from `Theme.of(context).colorScheme
// .surface` (the same color the Scaffold paints behind the main canvas — M3
// baseline `#FEF7FF`, deterministic from the theme), so the body matches the
// canvas exactly under any theme. The live body region also paints a grid
// (see `_BodyGridPainter`) to complete the "this is a sub-canvas" parity with
// the top level; collapsed / `f`-driven placeholders stay flat (a closed body
// shouldn't read as an active canvas).
const Color HOF_BODY_BORDER_COLOR = Color(0x4D000000); // black @ ~0.30
// Amber-tinted border for the "driven by `f`" placeholder, echoing the
// Function wire color; opaque enough to read against the light body fill.
const Color HOF_BODY_FUNCTION_OVERRIDE_BORDER_COLOR = Color(0xB3FFA726); // amber @ ~0.70
// Italic note text on the collapsed / function-override placeholders.
const Color HOF_BODY_PLACEHOLDER_TEXT_COLOR = Colors.black54;

const double WIRE_GLOW_BLUR_RADIUS = 8.0;
const double WIRE_GLOW_SPREAD_RADIUS = 2.0;

// Alignment tooltip colors — see doc/design_blueprint_alignment.md §6.2.
// Tooltips render on a dark background, so we use light tints rather than
// the saturated brown/orange used by the warning-triangle pin icon.
const Color ALIGNMENT_MOTIF_UNALIGNED_TOOLTIP_COLOR =
    Color(0xFFD7B49E); // light tan
const Color ALIGNMENT_LATTICE_UNALIGNED_TOOLTIP_COLOR =
    Color(0xFFFFAB91); // light salmon
// Saturated brown used by the warning-triangle pin painter; readable against
// the light node background.
const Color ALIGNMENT_MOTIF_UNALIGNED_COLOR = Color(0xFF6D4C41);

class PinViewWidget extends StatelessWidget {
  final String dataType;
  final bool multi;

  /// Input pins may declare an abstract data type (`HasAtoms`, `HasStructure`,
  /// `HasFreeLinOps`); those render as an N-sliced pie of concrete-satisfier
  /// colors. Output pins are always concrete and render single-colored.
  final bool isInput;
  final String? outputString;
  final String? pinName;
  final APIAlignment? alignment;
  final String? alignmentReason;

  /// For polymorphic output pins (`SameAsInput(...)` / abstract `Fixed`), this
  /// is the declared type string while [dataType] holds the concrete type the
  /// pin resolves to. The tooltip renders both so the polymorphism stays
  /// visible after resolution. Null for non-polymorphic pins (or when
  /// resolution didn't change the type).
  final String? declaredDataType;

  /// `true` when [dataType] came from a `SameAsInput` fallback because the
  /// named input has zero connections. The tooltip surfaces this as
  /// "default — no input connected".
  final bool resolvedViaFallback;

  /// Optional node-specific tooltip line, appended after the type label and
  /// any AnyFunction description. Used by `apply` / `map` to describe how the
  /// wired function value will be consumed ("apply will call it on the wired
  /// arguments" / "applied per element of the stream"). See
  /// `doc/design_function_pin_unification.md` (Phase D).
  final String? extraTooltipLine;

  const PinViewWidget(
      {super.key,
      required this.dataType,
      required this.multi,
      required this.isInput,
      this.outputString,
      this.pinName,
      this.alignment,
      this.alignmentReason,
      this.declaredDataType,
      this.resolvedViaFallback = false,
      this.extraTooltipLine});

  /// Friendly description for an AnyFunction pin type string, or `null` when
  /// [typeName] is not an AnyFunction. AnyFunction pins surface as `Function*`
  /// (no constraint) or `Function(T1, ..., *)` (starts-with constraint). The
  /// description is appended to the tooltip's type label to make the
  /// `Function*` / `*` notation self-explanatory. Wording mirrors
  /// `doc/design_function_pin_unification.md` (Phase D, §"Pin colors /
  /// tooltips").
  static String? _anyFunctionDescription(String typeName) {
    if (typeName == 'Function*') {
      return 'any function value';
    }
    const prefix = 'Function(';
    const suffix = ',*)';
    if (typeName.startsWith(prefix) && typeName.endsWith(suffix)) {
      final inner = typeName.substring(
          prefix.length, typeName.length - suffix.length);
      if (inner.isEmpty) return null;
      // `inner` is a flat list of leading parameter types separated by `,`.
      // For nested commas (e.g. `(Int,Bool) -> Float`) the simple split would
      // mis-tokenise — but those wrap in `()` so we still get the leading-
      // param list as the user authored it for display. Worst case the
      // description shows a slightly noisier signature; the type label above
      // it is authoritative.
      if (!inner.contains(',')) {
        return 'function whose first parameter is `$inner` '
            '(extra parameters allowed)';
      }
      // Approximate the leading-params count for the multi-leading-param
      // case; the heuristic is a comma split that's correct for flat types.
      final parts = inner.split(',');
      return 'function whose first ${parts.length} parameters are `$inner` '
          '(extra parameters allowed)';
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final List<Color> sliceColors;
    final String typeLabel;
    if (isInput && isAbstractDataType(dataType)) {
      final concretes = ABSTRACT_TYPE_CONCRETES[dataType]!;
      sliceColors = concretes.map(getDataTypeColor).toList(growable: false);
      typeLabel = '$dataType (${concretes.join(' or ')})';
    } else {
      sliceColors = [getDataTypeColor(dataType)];
      // For record types, expand the type label to include the resolved
      // schema. Named records get authored-order fields (`Name { f: T, ...}`);
      // anonymous records keep the inline canonical form (`{x: Int, y: Int}`)
      // already produced by Rust's Display impl.
      final expandedDataType = _expandRecordTypeLabel(dataType);
      // For polymorphic output pins, show "Concrete (declared: SameAsInput(name))"
      // and append "default — no input connected" when the concrete type came
      // from the disconnected-input fallback.
      if (declaredDataType != null && declaredDataType != dataType) {
        final fallbackNote =
            resolvedViaFallback ? ', default — no input connected' : '';
        typeLabel =
            '$expandedDataType  (declared: $declaredDataType$fallbackNote)';
      } else {
        typeLabel = expandedDataType;
      }
    }

    final List<InlineSpan> spans = [];
    if (pinName != null && pinName!.isNotEmpty) {
      spans.add(TextSpan(text: '── $pinName ──  $typeLabel'));
    } else {
      spans.add(TextSpan(text: typeLabel));
    }
    // AnyFunction friendly description ("any function value" / "function whose
    // first parameter is `T` (extra parameters allowed)") — appended so the
    // `Function*` / `*` notation reads as English. See
    // `doc/design_function_pin_unification.md` (Phase D).
    final anyFnDesc = _anyFunctionDescription(dataType);
    if (anyFnDesc != null) {
      spans.add(TextSpan(
        text: '\n$anyFnDesc',
        style: const TextStyle(color: Colors.white70),
      ));
    }
    if (extraTooltipLine != null && extraTooltipLine!.isNotEmpty) {
      spans.add(TextSpan(
        text: '\n${extraTooltipLine!}',
        style: const TextStyle(color: Colors.white70),
      ));
    }
    Color? reasonColor;
    if (alignment == APIAlignment.motifUnaligned) {
      spans.add(const TextSpan(
        text: '\nAlignment: motif-unaligned',
        style: TextStyle(color: ALIGNMENT_MOTIF_UNALIGNED_TOOLTIP_COLOR),
      ));
      reasonColor = ALIGNMENT_MOTIF_UNALIGNED_TOOLTIP_COLOR;
    } else if (alignment == APIAlignment.latticeUnaligned) {
      spans.add(const TextSpan(
        text: '\nAlignment: lattice-unaligned',
        style: TextStyle(color: ALIGNMENT_LATTICE_UNALIGNED_TOOLTIP_COLOR),
      ));
      reasonColor = ALIGNMENT_LATTICE_UNALIGNED_TOOLTIP_COLOR;
    }
    if (reasonColor != null &&
        alignmentReason != null &&
        alignmentReason!.isNotEmpty) {
      spans.add(TextSpan(
        text: '\n  ($alignmentReason)',
        style: TextStyle(color: reasonColor),
      ));
    }
    if (outputString != null && outputString!.isNotEmpty) {
      const maxLines = 15;
      const maxChars = 500;
      // For named-record pins, reorder the top-level fields of the runtime
      // value to match the def's authored field order so the preview lines
      // up with the schema shown in the type label. Anonymous and non-record
      // pins use the runtime string as-is.
      var preview = _reorderRecordPreview(dataType, outputString!);
      var truncated = false;
      if (preview.length > maxChars) {
        preview = preview.substring(0, maxChars);
        truncated = true;
      }
      final lines = preview.split('\n');
      if (lines.length > maxLines) {
        preview = lines.take(maxLines).join('\n');
        truncated = true;
      }
      if (truncated) {
        preview = '$preview\n...';
      }
      spans.add(TextSpan(text: '\n$preview'));
    }

    final bool unaligned = alignment == APIAlignment.motifUnaligned ||
        alignment == APIAlignment.latticeUnaligned;

    return Tooltip(
      richMessage: TextSpan(children: spans),
      preferBelow: false,
      child: Center(
        child: SizedBox(
          width: PIN_SIZE,
          height: PIN_SIZE,
          child: CustomPaint(
            painter: unaligned
                ? _WarningTrianglePinPainter(color: sliceColors.first)
                : _PinPainter(
                    sliceColors: sliceColors,
                    hollow: multi,
                    borderWidth: PIN_BORDER_WIDTH,
                  ),
          ),
        ),
      ),
    );
  }
}

/// Paints a pin as either a filled/ringed disk (single-color) or a pie-sliced
/// disk/ring (multi-color). Slices start at 12 o'clock and sweep clockwise.
class _PinPainter extends CustomPainter {
  final List<Color> sliceColors;
  final bool hollow;
  final double borderWidth;

  const _PinPainter({
    required this.sliceColors,
    required this.hollow,
    required this.borderWidth,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final outerRadius = size.width / 2;

    if (hollow) {
      // Match Flutter's BoxDecoration border behavior: the border is drawn
      // inside the shape bounds, so the stroke centerline sits at
      // outerRadius - borderWidth/2.
      final strokeRadius = outerRadius - borderWidth / 2;
      final strokeRect = Rect.fromCircle(center: center, radius: strokeRadius);
      final paint = Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = borderWidth;

      if (sliceColors.length == 1) {
        paint.color = sliceColors.first;
        canvas.drawCircle(center, strokeRadius, paint);
      } else {
        final sweep = 2 * math.pi / sliceColors.length;
        double startAngle = -math.pi / 2;
        for (final color in sliceColors) {
          paint.color = color;
          canvas.drawArc(strokeRect, startAngle, sweep, false, paint);
          startAngle += sweep;
        }
      }
    } else {
      final paint = Paint()..style = PaintingStyle.fill;

      if (sliceColors.length == 1) {
        paint.color = sliceColors.first;
        canvas.drawCircle(center, outerRadius, paint);
      } else {
        final rect = Rect.fromCircle(center: center, radius: outerRadius);
        final sweep = 2 * math.pi / sliceColors.length;
        double startAngle = -math.pi / 2;
        for (final color in sliceColors) {
          paint.color = color;
          canvas.drawArc(rect, startAngle, sweep, true, paint);
          startAngle += sweep;
        }
      }
    }
  }

  @override
  bool shouldRepaint(covariant _PinPainter oldDelegate) {
    return !listEquals(oldDelegate.sliceColors, sliceColors) ||
        oldDelegate.hollow != hollow ||
        oldDelegate.borderWidth != borderWidth;
  }
}

/// Paints an output pin as a filled warning triangle with an exclamation mark
/// for Blueprint/Crystal pins whose alignment is MotifUnaligned or
/// LatticeUnaligned. The triangle fits within the pin's circle bounding box so
/// wire-endpoint math and hit testing stay unchanged. See
/// `doc/design_blueprint_alignment.md` §6.3.
class _WarningTrianglePinPainter extends CustomPainter {
  final Color color;

  const _WarningTrianglePinPainter({required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    final double w = size.width;
    final double h = size.height;
    final path = Path()
      ..moveTo(w / 2, 0)
      ..lineTo(w, h)
      ..lineTo(0, h)
      ..close();

    final fillPaint = Paint()
      ..style = PaintingStyle.fill
      ..color = color;
    canvas.drawPath(path, fillPaint);

    // Exclamation mark: a short black bar above a small black dot, centered
    // in the lower half of the triangle where it has room.
    final barPaint = Paint()
      ..style = PaintingStyle.fill
      ..color = Colors.black;
    final double barWidth = w * 0.14;
    final double barTop = h * 0.32;
    final double barBottom = h * 0.68;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTRB(
            w / 2 - barWidth / 2, barTop, w / 2 + barWidth / 2, barBottom),
        Radius.circular(barWidth / 2),
      ),
      barPaint,
    );
    canvas.drawCircle(Offset(w / 2, h * 0.83), barWidth * 0.75, barPaint);
  }

  @override
  bool shouldRepaint(covariant _WarningTrianglePinPainter oldDelegate) {
    return oldDelegate.color != color;
  }
}

/// Stringifies a top-level [APIDataType] for tooltip display. Named records
/// are emitted as `Record(Name)` (no recursive expansion — one level is enough
/// for a tooltip; users can hover the next pin to drill down). Anonymous
/// record fields are wrapped through `Custom` and round-trip through their
/// `customDataType` string.
String _apiDataTypeToString(APIDataType dt) {
  String base;
  switch (dt.dataTypeBase) {
    case APIDataTypeBase.none:
      base = 'None';
      break;
    case APIDataTypeBase.bool:
      base = 'Bool';
      break;
    case APIDataTypeBase.string:
      base = 'String';
      break;
    case APIDataTypeBase.int:
      base = 'Int';
      break;
    case APIDataTypeBase.float:
      base = 'Float';
      break;
    case APIDataTypeBase.vec2:
      base = 'Vec2';
      break;
    case APIDataTypeBase.vec3:
      base = 'Vec3';
      break;
    case APIDataTypeBase.iVec2:
      base = 'IVec2';
      break;
    case APIDataTypeBase.iVec3:
      base = 'IVec3';
      break;
    case APIDataTypeBase.iMat3:
      base = 'IMat3';
      break;
    case APIDataTypeBase.mat3:
      base = 'Mat3';
      break;
    case APIDataTypeBase.latticeVecs:
      base = 'LatticeVecs';
      break;
    case APIDataTypeBase.drawingPlane:
      base = 'DrawingPlane';
      break;
    case APIDataTypeBase.geometry2D:
      base = 'Geometry2D';
      break;
    case APIDataTypeBase.blueprint:
      base = 'Blueprint';
      break;
    case APIDataTypeBase.hasAtoms:
      base = 'HasAtoms';
      break;
    case APIDataTypeBase.crystal:
      base = 'Crystal';
      break;
    case APIDataTypeBase.molecule:
      base = 'Molecule';
      break;
    case APIDataTypeBase.hasStructure:
      base = 'HasStructure';
      break;
    case APIDataTypeBase.hasFreeLinOps:
      base = 'HasFreeLinOps';
      break;
    case APIDataTypeBase.motif:
      base = 'Motif';
      break;
    case APIDataTypeBase.structure:
      base = 'Structure';
      break;
    case APIDataTypeBase.unit:
      base = 'Unit';
      break;
    case APIDataTypeBase.record:
      base = 'Record(${dt.customDataType ?? ''})';
      break;
    case APIDataTypeBase.iter:
      // `children[0]` is the element type — see
      // `doc/design_structural_function_and_iter_types.md`.
      final element =
          dt.children.isNotEmpty ? _apiDataTypeToString(dt.children[0]) : '?';
      base = 'Iter[$element]';
      break;
    case APIDataTypeBase.function:
      // `children = [p0, ..., pN-1, R]` — rightmost is the return type.
      if (dt.children.isEmpty) {
        base = 'Function';
      } else {
        final params = dt.children
            .sublist(0, dt.children.length - 1)
            .map(_apiDataTypeToString)
            .join(', ');
        final ret = _apiDataTypeToString(dt.children.last);
        base = '($params) -> $ret';
      }
      break;
    case APIDataTypeBase.custom:
      // Custom carries the full type string in customDataType (already
      // includes any array brackets). Return it verbatim and ignore the
      // outer `array` flag, matching the behavior of
      // `data_type_to_api_data_type` on the Rust side.
      return dt.customDataType ?? 'Custom';
  }
  return dt.array ? '[$base]' : base;
}

/// If [typeName] denotes a record type (named, anonymous, or array-wrapped
/// either way), returns a tooltip-friendly string with the schema expanded:
/// `Name { f: T, ... }` in authored order for named, `{x: Int, y: Int}` in
/// canonical order for anonymous. Non-record types are returned unchanged.
///
/// Dangling named references render as `Name (missing)`.
String _expandRecordTypeLabel(String typeName) {
  if (typeName.startsWith('[') && typeName.endsWith(']')) {
    return '[${_expandRecordTypeLabel(typeName.substring(1, typeName.length - 1))}]';
  }
  if (typeName.startsWith('{')) {
    // Anonymous — already canonical.
    return typeName;
  }
  const prefix = 'Record(';
  if (typeName.startsWith(prefix) && typeName.endsWith(')')) {
    final name = typeName.substring(prefix.length, typeName.length - 1);
    if (name.isEmpty) {
      return '(no record type chosen)';
    }
    final def = sd_api.getRecordTypeDef(name: name);
    if (def == null) {
      return '$name (missing)';
    }
    if (def.fields.isEmpty) {
      return '$name {}';
    }
    final fieldStrs = def.fields
        .map((f) => '${f.name}: ${_apiDataTypeToString(f.dataType)}')
        .join(', ');
    return '$name { $fieldStrs }';
  }
  return typeName;
}

/// Splits a record-value string `{a: 1, b: 2, c: {x: 3, y: 4}}` into a list
/// of `(fieldName, valueString)` pairs at the top level only. Nested
/// `{...}`, `[...]`, and `(...)` are passed through untouched. The input
/// must be the entire record value (the leading `{` and trailing `}` are
/// stripped by this function).
///
/// Returns null if [s] is not a balanced record literal — the caller falls
/// back to leaving the string unchanged.
List<MapEntry<String, String>>? _splitTopLevelRecordFields(String s) {
  if (!s.startsWith('{') || !s.endsWith('}')) {
    return null;
  }
  final body = s.substring(1, s.length - 1);
  final result = <MapEntry<String, String>>[];
  var depth = 0;
  var fieldStart = 0;
  var nameEnd = -1;
  for (var i = 0; i < body.length; i++) {
    final c = body[i];
    if (depth == 0) {
      if (c == ':' && nameEnd == -1) {
        nameEnd = i;
        continue;
      }
      if (c == ',') {
        if (nameEnd == -1) return null; // malformed
        final name = body.substring(fieldStart, nameEnd).trim();
        final value = body.substring(nameEnd + 1, i).trim();
        result.add(MapEntry(name, value));
        fieldStart = i + 1;
        nameEnd = -1;
        continue;
      }
    }
    if (c == '{' || c == '[' || c == '(') {
      depth++;
    } else if (c == '}' || c == ']' || c == ')') {
      depth--;
      if (depth < 0) return null;
    }
  }
  if (depth != 0) return null;
  if (fieldStart < body.length) {
    if (nameEnd == -1) {
      // No `:` in the trailing segment; not a record literal, just one
      // value (e.g. an empty record `{}` would have fieldStart >= length).
      final tail = body.substring(fieldStart).trim();
      if (tail.isEmpty) return result;
      return null;
    }
    final name = body.substring(fieldStart, nameEnd).trim();
    final value = body.substring(nameEnd + 1).trim();
    result.add(MapEntry(name, value));
  }
  return result;
}

/// If [typeName] denotes a named-record type and [preview] is a record
/// literal that matches that schema, returns a new preview with top-level
/// fields reordered to authored order. Otherwise returns [preview] unchanged.
///
/// Only the top-level fields are reordered; nested record values keep their
/// own (canonical) field order, since the type names that would justify
/// reordering them are not in scope at this hover. Extras carried by
/// pass-through width subtyping (fields on the value but not on the schema)
/// are preserved in their original order, after the schema fields.
String _reorderRecordPreview(String typeName, String preview) {
  final defName = extractNamedRecordDefName(typeName);
  if (defName == null) return preview;
  final def = sd_api.getRecordTypeDef(name: defName);
  if (def == null || def.fields.isEmpty) return preview;
  final fields = _splitTopLevelRecordFields(preview);
  if (fields == null) return preview;
  final schemaNames = {for (final f in def.fields) f.name};
  final valueByName = <String, String>{for (final e in fields) e.key: e.value};
  final orderedParts = <String>[];
  for (final f in def.fields) {
    final v = valueByName[f.name];
    if (v == null) {
      // Schema field missing from the runtime value — defensive guard, not
      // expected under pass-through. Bail out rather than emit a confusing
      // preview.
      return preview;
    }
    orderedParts.add('${f.name}: $v');
  }
  // Extras (width subtyping pass-through): append in the value's original
  // field order, after the schema fields.
  for (final e in fields) {
    if (schemaNames.contains(e.key)) continue;
    orderedParts.add('${e.key}: ${e.value}');
  }
  return '{${orderedParts.join(', ')}}';
}

class PinWidget extends StatelessWidget {
  final PinReference pinReference;
  final bool multi;
  final String? outputString;
  final String? pinName;
  final APIAlignment? alignment;
  final String? alignmentReason;

  /// Declared type for polymorphic output pins (`SameAsInput(...)`/abstract
  /// `Fixed`). Forwarded to [PinViewWidget] so the tooltip shows declared +
  /// resolved together. Null for non-polymorphic pins.
  final String? declaredDataType;

  /// `true` when the resolved type came from a `SameAsInput` disconnected-input
  /// fallback. Forwarded to [PinViewWidget] for tooltip annotation.
  final bool resolvedViaFallback;

  /// Optional node-specific tooltip line forwarded to [PinViewWidget]. Used
  /// for AnyFunction `f` pins on `apply` / `map` to describe how the wired
  /// function value will be consumed. See
  /// `doc/design_function_pin_unification.md` (Phase D).
  final String? extraTooltipLine;

  PinWidget(
      {required this.pinReference,
      required this.multi,
      this.outputString,
      this.pinName,
      this.alignment,
      this.alignmentReason,
      this.declaredDataType,
      this.resolvedViaFallback = false,
      this.extraTooltipLine})
      : super(
            key: ValueKey(
                pinReference.pinIndex + (pinReference.isOutput ? 1000 : 0)));

  RenderBox? _findNodeNetworkRenderBox(BuildContext context) {
    RenderBox? result;
    context.visitAncestorElements((element) {
      if (element.widget is NodeNetwork) {
        result = element.renderObject as RenderBox?;
        return false; // Stop visiting
      }
      return true; // Continue visiting
    });
    return result;
  }

  @override
  Widget build(BuildContext context) {
    final bool isInput = pinReference.isInput;
    return SizedBox(
      width: PIN_HIT_AREA_WIDTH,
      height: PIN_HIT_AREA_HEIGHT,
      child: DragTarget<PinReference>(
        builder: (context, candidateData, rejectedData) {
          return Draggable<PinReference>(
            data: pinReference,
            feedback: SizedBox.shrink(),
            childWhenDragging: SizedBox(
              width: PIN_HIT_AREA_WIDTH,
              height: PIN_HIT_AREA_HEIGHT,
              child: Center(
                child: PinViewWidget(
                    dataType: pinReference.dataType,
                    multi: multi,
                    isInput: isInput,
                    outputString: outputString,
                    pinName: pinName,
                    alignment: alignment,
                    alignmentReason: alignmentReason,
                    declaredDataType: declaredDataType,
                    resolvedViaFallback: resolvedViaFallback,
                    extraTooltipLine: extraTooltipLine),
              ),
            ),
            child: SizedBox(
              width: PIN_HIT_AREA_WIDTH,
              height: PIN_HIT_AREA_HEIGHT,
              child: Center(
                child: PinViewWidget(
                    dataType: pinReference.dataType,
                    multi: multi,
                    isInput: isInput,
                    outputString: outputString,
                    pinName: pinName,
                    alignment: alignment,
                    alignmentReason: alignmentReason,
                    declaredDataType: declaredDataType,
                    resolvedViaFallback: resolvedViaFallback,
                    extraTooltipLine: extraTooltipLine),
              ),
            ),
            onDragUpdate: (details) {
              final nodeNetworkBox = _findNodeNetworkRenderBox(context);
              if (nodeNetworkBox != null) {
                final position =
                    nodeNetworkBox.globalToLocal(details.globalPosition);
                Provider.of<StructureDesignerModel>(context, listen: false)
                    .dragWire(pinReference, position);
              }
            },
            onDragEnd: (details) {
              final model =
                  Provider.of<StructureDesignerModel>(context, listen: false);
              // Store the drag info before clearing
              final dragInfo = model.draggedWire;
              if (dragInfo != null) {
                // Notify parent to handle the drop (will show popup if in empty space)
                model.handleWireDropInEmptySpace(
                  dragInfo.startPin,
                  dragInfo.wireEndPosition,
                );
              }
              model.cancelDragWire();
            },
          );
        },
        onWillAcceptWithDetails: (details) {
          return Provider.of<StructureDesignerModel>(context, listen: false)
              .canConnectPins(details.data, pinReference);
        },
        onAcceptWithDetails: (details) {
          //print("Connected pin ${details.data} to pin $pinReference");
          Provider.of<StructureDesignerModel>(context, listen: false)
              .connectPins(details.data, pinReference);
        },
      ),
    );
  }
}

/// Widget representing a single draggable node.
class NodeWidget extends StatelessWidget {
  final NodeView node;
  final Offset panOffset;
  final ZoomLevel zoomLevel;

  /// Scope of the network the node lives in. Empty = top-level (the only
  /// possibility in phase U1; later phases use this for nodes inside an
  /// HOF's inline body). The node's `position` is interpreted in this scope's
  /// body-local coordinate frame.
  final List<BigInt> scopeChain;

  /// Root view used by [ScopeResolver] to walk the scope chain. In U1 always
  /// equal to `Provider.of(StructureDesignerModel).nodeNetworkView`. Passed in
  /// rather than re-fetched so the widget tree stays a pure function of its
  /// inputs.
  final NodeNetworkView rootView;

  NodeWidget({
    required this.node,
    required this.panOffset,
    required this.zoomLevel,
    required this.rootView,
    this.scopeChain = const [],
  }) : super(key: NodeWidgetKeys.nodeWidget(node.id, scopeChain: scopeChain));

  /// A resolver for this widget's current frame. Cheap to construct; the heavy
  /// layout pass lands in phase U3.
  ScopeResolver get _resolver => ScopeResolver(
        root: rootView,
        panOffset: panOffset,
        scale: getZoomScale(zoomLevel),
        zoomLevel: zoomLevel,
      );

  @override
  Widget build(BuildContext context) {
    final resolver = _resolver;

    // Choose rendering mode based on zoom level
    final Widget nodeContent = zoomLevel == ZoomLevel.normal
        ? _buildNormalNodeContent(context, resolver)
        : _buildZoomedOutNodeContent(context);

    // For HOFs use the cache-aware effective size so the outer container
    // grows when an inner body cascades past its stored size (U6). For
    // non-HOFs the cache call falls through to `getNodeSize`.
    final nodeSize = node.zone != null
        ? resolver.effectiveNodeSizeScreen(node, scopeChain)
        : getNodeSize(node, zoomLevel);

    // Create container with node appearance
    // For normal zoom, don't set explicit height - let content determine it (for subtitle)
    // For zoomed-out, set explicit height for fixed compact size
    Widget nodeWidget = Container(
      width: nodeSize.width,
      height: zoomLevel == ZoomLevel.normal ? null : nodeSize.height,
      decoration: _getNodeDecoration(),
      child: nodeContent,
    );

    // Add tooltip for nodes with errors
    if (node.error != null && node.error!.isNotEmpty) {
      nodeWidget = _wrapWithErrorTooltip(nodeWidget);
    }

    // Position the node via the scope resolver: node.position lives in
    // [scopeChain]'s body-local frame; the resolver maps it to screen.
    final screenPos =
        resolver.scopedToScreen(scopeChain, apiVec2ToOffset(node.position));
    return Positioned(
      left: screenPos.dx,
      top: screenPos.dy,
      child: nodeWidget,
    );
  }

  /// Builds the zoomed-out compact node showing only title
  Widget _buildZoomedOutNodeContent(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque, // Make entire node area interactive
      onTapDown: (details) => _handleNodeTap(context),
      onPanStart: (details) {
        _handleNodeTap(context);
        _handleNodeDragStart(context);
      },
      onPanUpdate: (details) => _handleNodeDrag(context, details),
      onPanEnd: (details) => _handleNodeDragEnd(context),
      onSecondaryTapDown: (details) => _handleContextMenu(context, details),
      child: Center(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
          child: Text(
            // A closure's user-supplied label, when present, is a much better
            // identifier at zoomed-out scale than the bare type name "closure".
            (node.closureCustomLabel ?? '').isNotEmpty
                ? node.closureCustomLabel!
                : getSimpleName(node.nodeTypeName),
            style: TextStyle(
              color: Colors.white,
              fontWeight: FontWeight.bold,
              fontSize: getNodeTitleFontSize(zoomLevel),
            ),
            overflow: TextOverflow.ellipsis,
            maxLines: 3,
            textAlign: TextAlign.center,
          ),
        ),
      ),
    );
  }

  /// Builds the normal detailed node with all pins and controls.
  ///
  /// For HOF nodes (`node.zone != null`) the inner row inserts a translucent
  /// body region between the external input and output columns; the body
  /// region carries the zone-input/zone-output pins on its inner edges and a
  /// centered `[N nodes]` placeholder until U4 lands body-node rendering. See
  /// `doc/design_zones_ui.md` §"Phase U3".
  ///
  /// [resolver] is threaded down so the HOF body can read the cache-aware
  /// effective size and collapse status (U6) without constructing a second
  /// resolver mid-build.
  Widget _buildNormalNodeContent(BuildContext context, ScopeResolver resolver) {
    final isHof = node.zone != null;
    // An effectively-collapsed (compact) HOF renders its body as an ordinary
    // node — input column (xs, f, …) + output column — instead of the body
    // region. `_buildRegularMainBody` already lays out the external input pins
    // (including `f`, an ordinary `parameters` entry) and output pins, so the
    // compact HOF gets correct, fully interactive pins with no new pin code.
    // The title-bar Row is unchanged: it still suppresses the legacy function
    // pin on every HOF. See `doc/design_hof_node_collapse.md`.
    final bool compactHof = isHof && node.zone!.collapsable && node.zone!.collapsed;
    // Closure: its single Function output pin renders in the title bar (the
    // legacy function-pin slot) instead of a right-edge output column, and the
    // body extends into the freed gutter. Only when the body is actually shown
    // (never compact — `closure` isn't collapsable). Kept in lockstep with the
    // `externalOutput` endpoint math via `hofOutputPinInTitleBar`. See
    // `doc/design_closures.md` Proposal 2.
    final bool outputInTitleBar =
        isHof && !compactHof && hofOutputPinInTitleBar(node);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // Title Bar
        GestureDetector(
          onTapDown: (details) => _handleNodeTap(context),
          onPanStart: (details) {
            _handleNodeTap(context);
            _handleNodeDragStart(context);
          },
          onPanUpdate: (details) => _handleNodeDrag(context, details),
          onPanEnd: (details) => _handleNodeDragEnd(context),
          onSecondaryTapDown: (details) => _handleContextMenu(context, details),
          child: Container(
            padding:
                const EdgeInsets.only(top: 4, bottom: 4, left: 8, right: 2),
            decoration: BoxDecoration(
              color: _getSpecialNodeColor() ?? _getTitleColor(),
              borderRadius: BorderRadius.vertical(
                  top: Radius.circular(NODE_BORDER_RADIUS - 2)),
            ),
            child: Row(
              children: [
                if (outputInTitleBar) ...[
                  // No node-type label for a closure — its distinctive shape
                  // (output pin in the title bar, wide body, no input column)
                  // already identifies it, and the function type is the
                  // instance-specific headline, so it takes the title's place
                  // and gets the full width. "closure" stays recoverable via
                  // the hover tooltip and the zoomed-out compact view. The
                  // Expanded pushes the output pin to the node's right edge —
                  // matching the title-bar `externalOutput` endpoint math in
                  // scope_resolver (x == nodeWidth).
                  //
                  // A non-empty `closureCustomLabel` on the NodeView prepends a
                  // user-supplied display label as `<label> · ƒ <sig>`. The
                  // label is free-form (no identifier restrictions) and lives
                  // on `ClosureData.custom_label`; absence falls back to the
                  // signature-only title.
                  Expanded(
                    child: Tooltip(
                      message: node.nodeTypeName,
                      waitDuration: const Duration(milliseconds: 500),
                      preferBelow: false,
                      child: node.outputPins.isNotEmpty
                          ? Text.rich(
                              TextSpan(children: [
                                if ((node.closureCustomLabel ?? '').isNotEmpty) ...[
                                  TextSpan(
                                    text: node.closureCustomLabel!,
                                    style: const TextStyle(
                                      color: Colors.white,
                                      fontWeight: FontWeight.bold,
                                      fontSize: 14,
                                    ),
                                  ),
                                  const TextSpan(
                                    text: ' · ',
                                    style: TextStyle(
                                      color: Colors.white70,
                                      fontWeight: FontWeight.bold,
                                      fontSize: 14,
                                    ),
                                  ),
                                ],
                                TextSpan(
                                  text: 'ƒ ',
                                  // Amber Function color, tying the glyph to
                                  // the output pin and its wires. Falls back to
                                  // white on the orange selected/active title
                                  // bar, where amber would lose contrast.
                                  style: TextStyle(
                                    color: (node.active || node.selected)
                                        ? Colors.white
                                        : getDataTypeColor(node
                                            .outputPins.first.effectiveDataType),
                                    fontWeight: FontWeight.bold,
                                    fontSize: 15,
                                    fontStyle: FontStyle.italic,
                                  ),
                                ),
                                TextSpan(
                                  text:
                                      node.outputPins.first.effectiveDataType,
                                  style: const TextStyle(
                                    color: Colors.white,
                                    fontWeight: FontWeight.bold,
                                    fontSize: 14,
                                  ),
                                ),
                              ]),
                              overflow: TextOverflow.ellipsis,
                            )
                          : Text(
                              getSimpleName(node.nodeTypeName),
                              style: const TextStyle(
                                color: Colors.white,
                                fontWeight: FontWeight.bold,
                                fontSize: 14,
                              ),
                              overflow: TextOverflow.ellipsis,
                            ),
                    ),
                  ),
                  const SizedBox(width: 4),
                  // The closure's single Function output pin. No eye toggle —
                  // a Function value has no viewport display. Endpoint resolved
                  // by scope_resolver's `externalOutput` arm at the title-bar
                  // vertical offset.
                  if (node.outputPins.isNotEmpty)
                    PinWidget(
                      pinReference: PinReference(
                        nodeId: node.id,
                        scopeChain: scopeChain,
                        pinKind: PinKind.externalOutput,
                        pinIndex: node.outputPins.first.index,
                        dataType: node.outputPins.first.effectiveDataType,
                      ),
                      multi: false,
                    ),
                ] else ...[
                  Expanded(
                    child: Tooltip(
                      message: node.nodeTypeName,
                      waitDuration: const Duration(milliseconds: 500),
                      preferBelow: false,
                      child: Text(
                        getSimpleName(node.nodeTypeName),
                        style: const TextStyle(
                          color: Colors.white,
                          fontWeight: FontWeight.bold,
                          fontSize: 14,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ),
                  const SizedBox(width: 4),
                  // Function pin — suppressed on HOFs. The legacy function pin
                  // is never wired meaningfully on zone-owning nodes (their
                  // body replaces the closure abstraction), and showing it
                  // confuses users into wiring closures into HOFs. Full
                  // function-pin removal lives in the parent Rust design's
                  // cleanup phase per `doc/design_zones_ui.md` §U7. Until then
                  // we render it only on non-HOF nodes.
                  if (!isHof)
                    PinWidget(
                      pinReference: PinReference(
                        nodeId: node.id,
                        scopeChain: scopeChain,
                        pinKind: PinKind.functionPin,
                        pinIndex: -1,
                        dataType: node.functionType,
                      ),
                      multi: false,
                      // The `-1` pin's type is wiring-aware
                      // (`doc/design_node_function_pin_captures.md`): its
                      // parameters are the node's unwired input pins; wired
                      // inputs are frozen as captures and drop out of the
                      // signature. The label flowing into `node.functionType`
                      // already reflects this; this line names the rule.
                      extraTooltipLine: 'function of the unwired inputs; '
                          'wired inputs are captured',
                    ),
                ],
              ],
            ),
          ),
        ),
        // Main Body — different layout for expanded HOFs (body region between
        // input/output columns) vs. regular nodes and compact HOFs (just
        // input/output columns).
        if (compactHof)
          _buildRegularMainBody(context)
        else if (isHof)
          _buildHofMainBody(context, resolver)
        else
          _buildRegularMainBody(context),
        // Subtitle (if present)
        if (node.subtitle != null && node.subtitle!.isNotEmpty)
          Container(
            width: double.infinity,
            padding: const EdgeInsets.only(left: 8, right: 8, bottom: 4),
            child: Tooltip(
              message: node.subtitle!,
              preferBelow: true,
              child: Text(
                node.subtitle!,
                style: const TextStyle(
                  color: Colors.white70,
                  fontSize: 12,
                  fontStyle: FontStyle.italic,
                ),
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
              ),
            ),
          ),
      ],
    );
  }

  /// Original (non-HOF) main body: input column on the left, output column
  /// on the right. Preserved verbatim from the pre-zones layout.
  Widget _buildRegularMainBody(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Left Side (Inputs)
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: node.inputPins
                  .asMap()
                  .entries
                  .map((entry) => _buildInputPin(
                        entry.value.name,
                        PinReference(
                          nodeId: node.id,
                          scopeChain: scopeChain,
                          pinKind: PinKind.externalInput,
                          pinIndex: entry.key,
                          dataType: entry.value.dataType,
                        ),
                        entry.value.multi,
                        extraTooltipLine:
                            _extraTooltipForInputPin(entry.value.name),
                      ))
                  .toList(),
            ),
          ),
          // Right Side (Outputs)
          Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: node.outputPins
                .map((pin) => _buildOutputPin(context, pin))
                .toList(),
          ),
        ],
      ),
    );
  }

  /// HOF main body: external input column on the left, translucent body
  /// region in the middle (carrying inner-edge zone pins + body nodes),
  /// external output column on the right.
  ///
  /// The body region grows with its content: width/height are read from the
  /// scope resolver's layout cache, which uses `max(stored, content_bbox +
  /// padding)`. Live drag of a body node grows the body live (the cache
  /// reads the model's live positions every frame).
  ///
  /// **U6 collapse.** When the resolver reports this body's chain as
  /// collapsed (its rendered screen-space height is below the readability
  /// threshold, typically because of deep nesting + far zoom), the body
  /// region renders as a simple "[N nodes]" placeholder without the inner
  /// zone-pin widgets or resize handle. Body nodes/wires are skipped
  /// elsewhere in the recursive walks. See `doc/design_zones_ui.md`
  /// §"Zoom levels".
  Widget _buildHofMainBody(BuildContext context, ScopeResolver resolver) {
    final zone = node.zone!;
    final bodyChain = [...scopeChain, node.id];
    final cachedSize = resolver.layout.lookupSize(bodyChain);
    final effectiveSize = cachedSize ?? Size(zone.storedWidth, zone.storedHeight);
    final collapsed = resolver.isBodyCollapsed(bodyChain);
    final fOverridden = resolver.isBodyFunctionOverridden(bodyChain);
    // Closure trims its chrome: no external input column to reserve (it has no
    // input pins) and its Function output renders in the title bar, so the
    // right gutter holds nothing. Both gutters shrink to small pads via the
    // shared helpers, letting the body span the freed width. See
    // `doc/design_closures.md` Proposal 2.
    final bool outputInTitleBar = hofOutputPinInTitleBar(node);
    return SizedBox(
      width: hofBodyLeftOffset(node) +
          effectiveSize.width +
          hofBodyRightGutter(node),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // External input column. Width matched to the left offset so its
          // right edge meets the body region's left edge.
          SizedBox(
            width: hofBodyLeftOffset(node),
            child: Padding(
              padding: const EdgeInsets.only(left: 2, top: 2),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: node.inputPins
                    .asMap()
                    .entries
                    .map((entry) => _buildInputPin(
                          entry.value.name,
                          PinReference(
                            nodeId: node.id,
                            scopeChain: scopeChain,
                            pinKind: PinKind.externalInput,
                            pinIndex: entry.key,
                            dataType: entry.value.dataType,
                          ),
                          entry.value.multi,
                          extraTooltipLine:
                              _extraTooltipForInputPin(entry.value.name),
                        ))
                    .toList(),
              ),
            ),
          ),
          // Translucent body region. Falls back to a minimal placeholder
          // when the body is collapsed — its content is hidden elsewhere by
          // the recursive walks in node_network.dart / node_network_painter.
          //
          // When the HOF's `f` pin is wired the inline body is ignored at
          // eval time, so we render a distinct "driven by `f`" placeholder
          // (the scope resolver also flags it collapsed so the body content
          // is skipped, giving one obvious source of truth). See
          // `doc/design_closures.md`.
          if (fOverridden)
            _ZoneFunctionOverridePlaceholder(effectiveSize: effectiveSize)
          else if (collapsed)
            _ZoneCollapsedPlaceholder(
              nodeCount: zone.nodes.length,
              effectiveSize: effectiveSize,
            )
          else
            _ZoneBodyRegion(
              nodeId: node.id,
              scopeChain: scopeChain,
              zone: zone,
              effectiveSize: effectiveSize,
              onResize: (newSize) {
                final model = Provider.of<StructureDesignerModel>(context,
                    listen: false);
                model.setZoneSize(
                    scopeChain, node.id, newSize.width, newSize.height);
              },
              onResizeStart: () {
                Provider.of<StructureDesignerModel>(context, listen: false)
                    .beginZoneResize(scopeChain, node.id);
              },
              onResizeEnd: () {
                Provider.of<StructureDesignerModel>(context, listen: false)
                    .endZoneResize();
              },
            ),
          // External output column. For a closure the single Function output
          // lives in the title bar, so the gutter is just a small pad with no
          // output pins; the other HOFs render their result pin(s) here.
          if (outputInTitleBar)
            SizedBox(width: hofBodyRightGutter(node))
          else
            SizedBox(
              width: hofBodyRightGutter(node),
              child: Padding(
                padding: const EdgeInsets.only(right: 2, top: 2),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: node.outputPins
                      .map((pin) => _buildOutputPin(context, pin))
                      .toList(),
                ),
              ),
            ),
        ],
      ),
    );
  }

  /// Creates a labeled input pin. [extraTooltipLine], when non-null, appends
  /// a node-specific line to the pin's tooltip (e.g. apply.f → "apply will
  /// call it on the wired arguments"). See
  /// `doc/design_function_pin_unification.md` (Phase D).
  Widget _buildInputPin(String label, PinReference pinReference, bool multi,
      {String? extraTooltipLine}) {
    return Row(
      children: [
        PinWidget(
          pinReference: pinReference,
          multi: multi,
          extraTooltipLine: extraTooltipLine,
        ),
        SizedBox(width: 2),
        Expanded(
          child: Text(
            label,
            style: TextStyle(color: Colors.white, fontSize: 14),
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ],
    );
  }

  /// Returns the node-specific secondary tooltip line for the `f` AnyFunction
  /// input pin on `apply` and `map`, or `null` for every other pin / node.
  /// Wording per `doc/design_function_pin_unification.md` (Phase D).
  String? _extraTooltipForInputPin(String pinName) {
    if (pinName != 'f') return null;
    switch (node.nodeTypeName) {
      case 'apply':
        return 'apply will call it on the wired arguments';
      case 'map':
        return 'applied per element of the stream';
      default:
        return null;
    }
  }

  /// Creates an output pin row with eye icon and pin dot.
  /// Pin name is shown in the hover tooltip, not inline.
  Widget _buildOutputPin(BuildContext context, OutputPinView pin) {
    final bool isPinDisplayed = node.displayedPins.contains(pin.index);
    // Unit pins carry no displayable value (effect-node return type), so the
    // visibility toggle is hidden — they can never be the lowest displayed
    // pin for hit testing either. See `doc/design_node_execution.md`
    // ("Display semantics" of the Unit type).
    final bool isUnitPin = pin.effectiveDataType == 'Unit';
    // Function-mode display suppression. When this node's function pin is
    // consumed (wired into an HOF `f` or `apply.f`) the node is a function
    // value, not a value source: the Rust scene builder skips it entirely
    // (`function_pin_consumed`), so its output pins can never render. Disable
    // the eye toggle (non-interactive, greyed) and redirect the user to `apply`
    // for a sampled preview. Derived — disconnecting `f` restores the eye for
    // free. See `doc/design_function_pins.md` §"Display in function mode".
    final bool functionConsumed = node.functionPinConsumed;

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (!isUnitPin && functionConsumed)
          Tooltip(
            message: 'Used as a function — wire into `apply` to preview it.',
            preferBelow: false,
            child: const Icon(
              Icons.visibility_off,
              color: Colors.white24,
              size: 16,
            ),
          )
        else if (!isUnitPin)
          GestureDetector(
            key: NodeWidgetKeys.outputPinVisibility(node.id, pin.index),
            onTap: () {
              final model =
                  Provider.of<StructureDesignerModel>(context, listen: false);
              model.toggleOutputPinDisplay(node.id, pin.index);
            },
            child: Icon(
              isPinDisplayed ? Icons.visibility : Icons.visibility_off,
              color: Colors.white70,
              size: 16,
            ),
          ),
        if (!isUnitPin) const SizedBox(width: 2),
        PinWidget(
          pinReference: PinReference(
            nodeId: node.id,
            scopeChain: scopeChain,
            pinKind: PinKind.externalOutput,
            pinIndex: pin.index,
            dataType: pin.effectiveDataType,
          ),
          multi: false,
          outputString: pin.index < node.outputPinStrings.length
              ? node.outputPinStrings[pin.index]
              : null,
          pinName: node.outputPins.length > 1 ? pin.name : null,
          alignment: pin.alignment,
          alignmentReason: pin.alignmentReason,
          // Show the polymorphic declaration alongside the resolved type
          // whenever they differ (e.g. `SameAsInput(molecule)` → `Molecule`).
          declaredDataType: pin.resolvedDataType != null ? pin.dataType : null,
          resolvedViaFallback: pin.resolvedViaFallback,
        ),
      ],
    );
  }

  /// Returns the special color for return/parameter nodes, or null for regular nodes
  Color? _getSpecialNodeColor() {
    if (node.returnNode) {
      return NODE_TITLE_COLOR_RETURN;
    } else if (node.nodeTypeName == "parameter") {
      return NODE_TITLE_COLOR_PARAMETER;
    }
    return null;
  }

  /// Returns the title bar color based on selection state
  Color _getTitleColor() {
    if (node.active) {
      return NODE_COLOR_ACTIVE;
    } else if (node.selected) {
      return NODE_COLOR_SELECTED;
    }
    return NODE_TITLE_COLOR_NORMAL;
  }

  /// Returns the decoration for the node container
  BoxDecoration _getNodeDecoration() {
    // Use colored background for special nodes in zoomed-out modes
    final backgroundColor = (zoomLevel != ZoomLevel.normal)
        ? (_getSpecialNodeColor() ?? NODE_BACKGROUND_COLOR)
        : NODE_BACKGROUND_COLOR;

    // Determine border color and width based on state:
    // Priority: error > active > selected > normal
    Color borderColor;
    double borderWidth;
    List<BoxShadow>? boxShadow;

    if (node.error != null) {
      borderColor = NODE_BORDER_COLOR_ERROR;
      borderWidth = NODE_BORDER_WIDTH_NORMAL;
      boxShadow = [
        BoxShadow(
            color: NODE_BORDER_COLOR_ERROR.withValues(alpha: WIRE_GLOW_OPACITY),
            blurRadius: WIRE_GLOW_BLUR_RADIUS,
            spreadRadius: WIRE_GLOW_SPREAD_RADIUS)
      ];
    } else if (node.active) {
      // Active node: thicker border, full glow
      borderColor = NODE_COLOR_ACTIVE;
      borderWidth = NODE_BORDER_WIDTH_ACTIVE;
      boxShadow = [
        BoxShadow(
            color: NODE_COLOR_ACTIVE.withValues(alpha: WIRE_GLOW_OPACITY),
            blurRadius: WIRE_GLOW_BLUR_RADIUS,
            spreadRadius: WIRE_GLOW_SPREAD_RADIUS)
      ];
    } else if (node.selected) {
      // Selected but not active
      borderColor = NODE_COLOR_SELECTED;
      borderWidth = NODE_BORDER_WIDTH_SELECTED;
      boxShadow = [
        BoxShadow(
            color:
                NODE_COLOR_SELECTED.withValues(alpha: WIRE_GLOW_OPACITY * 0.5),
            blurRadius: WIRE_GLOW_BLUR_RADIUS * 0.7,
            spreadRadius: WIRE_GLOW_SPREAD_RADIUS * 0.5)
      ];
    } else {
      // Normal (not selected)
      borderColor = NODE_BORDER_COLOR_NORMAL;
      borderWidth = NODE_BORDER_WIDTH_NORMAL;
      boxShadow = null;
    }

    return BoxDecoration(
      color: backgroundColor,
      borderRadius: BorderRadius.circular(NODE_BORDER_RADIUS),
      border: Border.all(color: borderColor, width: borderWidth),
      boxShadow: boxShadow,
    );
  }

  /// Wraps a widget with error tooltip
  Widget _wrapWithErrorTooltip(Widget child) {
    return Tooltip(
      message: node.error!,
      textStyle: const TextStyle(fontSize: 14, color: Colors.white),
      decoration: BoxDecoration(
        color: Colors.red.shade700,
        borderRadius: BorderRadius.circular(4),
      ),
      waitDuration: const Duration(milliseconds: 500),
      showDuration: const Duration(seconds: 5),
      padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 12),
      preferBelow: true,
      verticalOffset: 35.0,
      child: child,
    );
  }

  /// Handles node tap for selection with modifier key support.
  /// Clicking a node makes its body the active scope for subsequent keyboard
  /// shortcuts (per `doc/design_zones_ui.md` §"The active body").
  void _handleNodeTap(BuildContext context) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    model.setActiveScopeChain(scopeChain);

    if (HardwareKeyboard.instance.isControlPressed) {
      model.toggleNodeSelection(node.id, scopeChain: scopeChain);
    } else if (HardwareKeyboard.instance.isShiftPressed) {
      model.addNodeToSelection(node.id, scopeChain: scopeChain);
    } else if (node.selected && !node.active) {
      model.addNodeToSelection(node.id, scopeChain: scopeChain);
    } else {
      model.setSelectedNode(node.id, scopeChain: scopeChain);
    }
  }

  /// Handles the start of a node drag - captures positions for undo coalescing
  void _handleNodeDragStart(BuildContext context) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    model.beginMoveNodes(scopeChain: scopeChain);
  }

  /// Handles node drag for positioning - moves all selected nodes if this node is selected
  void _handleNodeDrag(BuildContext context, DragUpdateDetails details) {
    final scale = getZoomScale(zoomLevel);
    final logicalDelta = details.delta / scale;
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    if (node.selected) {
      model.dragSelectedNodes(logicalDelta, scopeChain: scopeChain);
    } else {
      model.dragNodePosition(node.id, logicalDelta, scopeChain: scopeChain);
    }
  }

  /// Handles end of node drag - commits position of all moved nodes
  void _handleNodeDragEnd(BuildContext context) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    if (node.selected) {
      model.updateSelectedNodesPosition(scopeChain: scopeChain);
    } else {
      model.updateNodePosition(node.id, scopeChain: scopeChain);
    }
    model.endMoveNodes(scopeChain: scopeChain);
  }

  /// Handles context menu for node
  void _handleContextMenu(BuildContext context, TapDownDetails details) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    // Only change selection if this node isn't already selected
    // This preserves multi-selection for operations like "Factor out to Subnetwork"
    if (!node.selected) {
      model.setSelectedNode(node.id);
    }

    final RenderBox overlay =
        Overlay.of(context).context.findRenderObject() as RenderBox;
    final RelativeRect position = RelativeRect.fromRect(
      Rect.fromPoints(
        details.globalPosition,
        details.globalPosition,
      ),
      Offset.zero & overlay.size,
    );

    final bool isCustomNode = isCustomNodeType(nodeTypeName: node.nodeTypeName);

    // Check if the selection can be factored into a subnetwork
    final factorInfo = getFactorSelectionInfo();
    final bool canFactor = factorInfo.canFactor;

    // The Body collapse-mode group is offered only for collapsable HOFs
    // (`map`/`filter`/`fold`/`foreach`); `closure` and every non-HOF leave
    // `collapsable` false. See `doc/design_hof_node_collapse.md`.
    final bool isCollapsableHof = node.zone != null && node.zone!.collapsable;

    // Explicit `<String>` so the heterogeneous items list (the value-bearing
    // items, the disabled "Body" header, and the `PopupMenuDivider`) infers
    // `List<PopupMenuEntry<String>>` rather than collapsing to a `StatefulWidget`
    // LUB — needed once the divider/header are mixed in.
    showMenu<String>(
      context: context,
      position: position,
      items: [
        if (isCustomNode)
          PopupMenuItem(
            value: 'go_to_definition',
            child: Text('Go to Definition'),
          ),
        PopupMenuItem(
          value: 'execute',
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: const [
              Icon(Icons.play_arrow, size: 18),
              SizedBox(width: 8),
              Text('Execute'),
            ],
          ),
        ),
        PopupMenuItem(
          value: 'return',
          child: Text(
              node.returnNode ? 'Unset as return node' : 'Set as return node'),
        ),
        PopupMenuItem(
          value: 'duplicate',
          child: Text('Duplicate node (Ctrl+D)'),
        ),
        PopupMenuItem(
          value: 'copy',
          child: Text('Copy (Ctrl+C)'),
        ),
        PopupMenuItem(
          value: 'cut',
          child: Text('Cut (Ctrl+X)'),
        ),
        // Top-level only: "parameter" is a network-interface concept, and the
        // Rust `promote_node_to_parameter` operates on the active top-level
        // network by bare id. Offering it on a body node would mis-target a
        // colliding top-level id (per-body id counters). Bodies expose zone
        // inputs, not arbitrary parameters.
        if (scopeChain.isEmpty)
          PopupMenuItem(
            value: 'promote_to_parameter',
            child: Text('Promote to Parameter'),
          ),
        if (canFactor)
          PopupMenuItem(
            value: 'factor_into_subnetwork',
            child: Text('Factor out to Subnetwork...'),
          ),
        // Body collapse-mode radio group (collapsable HOFs only). The
        // check-mark sits on the current `collapseMode`; picking "Auto" is the
        // "stop overriding" path. No dialog/submenu — the flat `showMenu` has
        // no native cascade and view state doesn't warrant a dialog.
        if (isCollapsableHof) ...[
          const PopupMenuDivider(),
          const PopupMenuItem<String>(enabled: false, child: Text('Body')),
          _collapseModeItem(
              'collapse_auto', 'Auto (follow f)', node.zone!.collapseMode),
          _collapseModeItem('collapse_expanded', 'Always expanded',
              node.zone!.collapseMode),
          _collapseModeItem('collapse_collapsed', 'Always collapsed',
              node.zone!.collapseMode),
        ],
      ],
    ).then((value) {
      if (!context.mounted) return;
      if (value == 'go_to_definition') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.setActiveNodeNetwork(node.nodeTypeName);
      } else if (value == 'execute') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        runExecuteWithPlacard(context, model, node.id, scopeChain: scopeChain);
      } else if (value == 'return') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        if (node.returnNode) {
          model.setReturnNodeId(null);
        } else {
          model.setReturnNodeId(node.id);
        }
      } else if (value == 'duplicate') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.duplicateNode(node.id);
      } else if (value == 'copy') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.copySelection();
      } else if (value == 'cut') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.cutSelection();
      } else if (value == 'promote_to_parameter') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        final result = model.promoteNodeToParameter(node.id);
        if (!context.mounted) return;
        if (!result.success) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text(result.error ?? 'Could not promote node'),
              backgroundColor: Colors.red.shade700,
            ),
          );
        }
      } else if (value == 'factor_into_subnetwork') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        showFactorIntoSubnetworkDialog(context, model);
      } else if (value == 'collapse_auto') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.setCollapseMode(scopeChain, node.id, APICollapseMode.auto);
      } else if (value == 'collapse_expanded') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.setCollapseMode(scopeChain, node.id, APICollapseMode.expanded);
      } else if (value == 'collapse_collapsed') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.setCollapseMode(scopeChain, node.id, APICollapseMode.collapsed);
      }
    });
  }

  /// One radio-style item in the Body collapse-mode group. The check-mark is
  /// shown (via a transparent placeholder icon when inactive, so labels stay
  /// aligned) when [value] corresponds to [current].
  PopupMenuItem<String> _collapseModeItem(
      String value, String label, APICollapseMode current) {
    final bool active = _valueMatchesMode(value, current);
    return PopupMenuItem<String>(
      value: value,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.check,
              size: 16, color: active ? null : Colors.transparent),
          const SizedBox(width: 8),
          Text(label),
        ],
      ),
    );
  }

  /// Whether a Body menu item's [value] denotes the currently active
  /// [APICollapseMode].
  bool _valueMatchesMode(String value, APICollapseMode current) {
    switch (current) {
      case APICollapseMode.auto:
        return value == 'collapse_auto';
      case APICollapseMode.collapsed:
        return value == 'collapse_collapsed';
      case APICollapseMode.expanded:
        return value == 'collapse_expanded';
    }
  }
}

/// Runs an Execute pass on the given node behind a modal "Executing…" placard.
///
/// Recipe (per `doc/design_node_execution.md`, Phase 3 — UX during execution):
/// 1. Show a non-dismissable `DraggableDialog` placard.
/// 2. Yield via `endOfFrame` so the dialog actually paints before the
///    synchronous FFI call begins (otherwise the UI thread is blocked
///    before the dialog reaches the screen).
/// 3. Run the FFI call inside `try { … } finally { Navigator.pop }` so the
///    placard is always dismissed — including on a thrown FFI error or a
///    Rust panic surfaced through FRB. Without the `finally`, those failure
///    paths would leave the user staring at an undismissable dialog.
/// 4. Surface error / failure messages via SnackBar **after** the dialog is
///    gone.
///
/// No `CircularProgressIndicator` — the UI thread is blocked during the FFI
/// call, so any animated widget would freeze mid-frame and look broken.
Future<void> runExecuteWithPlacard(
  BuildContext context,
  StructureDesignerModel model,
  BigInt nodeId, {
  List<BigInt> scopeChain = const [],
}) async {
  // Capture the messenger before we await — looking it up off `context`
  // afterward is racy (the context may be unmounted by the time we surface
  // the result).
  final messenger = ScaffoldMessenger.maybeOf(context);

  showDialog(
    context: context,
    barrierDismissible: false,
    builder: (_) => const DraggableDialog(
      width: 320,
      dismissible: false,
      child: Padding(
        padding: EdgeInsets.all(24),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.hourglass_empty),
            SizedBox(width: 16),
            Text('Executing…'),
          ],
        ),
      ),
    ),
  );

  // Let the dialog frame paint before we hand the UI thread off to the
  // synchronous FFI call.
  await SchedulerBinding.instance.endOfFrame;

  APIExecuteResult? result;
  Object? thrown;
  try {
    result = model.executeNode(nodeId, scopeChain: scopeChain);
  } catch (e) {
    thrown = e;
  } finally {
    if (context.mounted) Navigator.of(context).pop();
  }

  if (messenger == null) return;
  if (thrown != null) {
    messenger.showSnackBar(
      SnackBar(
        content: Text('Execute failed: $thrown'),
        backgroundColor: Colors.red,
      ),
    );
    return;
  }
  if (result == null) {
    messenger.showSnackBar(
      const SnackBar(content: Text('No active network for Execute')),
    );
    return;
  }
  if (!result.ok) {
    messenger.showSnackBar(
      SnackBar(
        content: Text(result.error ?? 'Execute failed'),
        backgroundColor: Colors.red,
      ),
    );
  } else {
    messenger.showSnackBar(
      const SnackBar(
        content: Text('Execute complete'),
        duration: Duration(seconds: 2),
      ),
    );
  }
}

/// Translucent body region rendered inside an HOF node. Carries the inner-edge
/// zone pins (zone-input on the left, zone-output on the right). Body nodes
/// themselves are rendered by the parent `NodeNetwork`'s Stack at their
/// scope-resolved screen positions — not nested inside this widget.
///
/// Zone pins are fully interactive [PinWidget]s in U4 so users can wire body
/// nodes to/from them. From the body's perspective, zone-input pins are
/// sources (drag *from* them) and zone-output pins are destinations (drag
/// *to* them). The right-click on body interior opens the Add Node popup
/// parameterized by the body's scope (handled in [NodeNetwork]).
class _ZoneBodyRegion extends StatelessWidget {
  final BigInt nodeId;
  final List<BigInt> scopeChain;
  final ZoneView zone;

  /// Rendered body size (== `max(stored, content_bbox + padding)`), computed
  /// by the scope resolver's layout pass. Drives both the body rectangle
  /// and the inner-right edge pin positions.
  final Size effectiveSize;

  /// Called when the user finishes a resize drag; receives the new logical
  /// size to persist via `set_zone_size`. The drag bottom-right corner
  /// updates this in real time and commits at drag end.
  final ValueChanged<Size> onResize;

  /// Called at the start / end of a resize drag so the model can coalesce the
  /// live `onResize` updates into a single undo command.
  final VoidCallback onResizeStart;
  final VoidCallback onResizeEnd;

  const _ZoneBodyRegion({
    required this.nodeId,
    required this.scopeChain,
    required this.zone,
    required this.effectiveSize,
    required this.onResize,
    required this.onResizeStart,
    required this.onResizeEnd,
  });

  /// Scope chain of the body's interior — the HOF's containing-network chain
  /// extended with the HOF's own node id.
  List<BigInt> get _bodyScopeChain => [...scopeChain, nodeId];

  @override
  Widget build(BuildContext context) {
    final width = effectiveSize.width;
    final height = effectiveSize.height;

    // Pin vertical positions relative to the body region's top-left match the
    // `pinScreenPosition` formula in ScopeResolver: the offset from the node's
    // top is `NODE_VERT_WIRE_OFFSET + (i + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM`,
    // and the body region's top within the node is BASE_HOF_BODY_TOP_OFFSET.
    double pinY(int i) =>
        NODE_VERT_WIRE_OFFSET +
        (i + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM -
        BASE_HOF_BODY_TOP_OFFSET -
        PIN_HIT_AREA_HEIGHT / 2;

    return Container(
      width: width,
      height: height,
      margin: const EdgeInsets.only(top: BASE_HOF_BODY_TOP_OFFSET - 30),
      decoration: BoxDecoration(
        // Light "canvas-like" surface matching the main network background, so
        // the body reads as the same kind of space as the top level rather
        // than inverting to dark from level 2 down. The fill is the theme's
        // surface color (what the Scaffold paints behind the main canvas), so
        // it matches exactly under any theme. The border delineates the region
        // against the (also light) parent canvas. See `doc/design_zones_ui.md`
        // §"Visual model".
        color: Theme.of(context).colorScheme.surface,
        border: Border.all(
          color: HOF_BODY_BORDER_COLOR,
          width: 1.0,
        ),
        borderRadius: BorderRadius.circular(6.0),
      ),
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          // Grid backdrop — the same minor/major grid the main canvas draws,
          // so the body reads as a sub-canvas (level-1/level-2 parity). Drawn
          // first so it sits behind the zone pins, resize handle, and (via the
          // top-level Stack) the body nodes and wires. Clipped to the rounded
          // body rect; `IgnorePointer` so it never competes for pointer events.
          // Only ever rendered at normal zoom (scale 1.0), so no scale math.
          Positioned.fill(
            child: IgnorePointer(
              child: ClipRRect(
                borderRadius: BorderRadius.circular(6.0),
                child: CustomPaint(
                  painter: const _BodyGridPainter(),
                ),
              ),
            ),
          ),
          // Click-to-activate-body layer. A `Listener` (not a GestureDetector)
          // so it doesn't enter the gesture arena and compete with inner pins'
          // Draggable. We mark the body scope active on pointer-down and clear
          // body-scope selection only on pointer-up that didn't move — this
          // avoids the translucent-GestureDetector hit-test that was killing
          // wire drags from zone pins.
          Positioned.fill(
            child: Listener(
              behavior: HitTestBehavior.translucent,
              onPointerDown: (_) {
                final model = Provider.of<StructureDesignerModel>(context,
                    listen: false);
                model.setActiveScopeChain(_bodyScopeChain);
              },
            ),
          ),
          // Zone-input pins on the body's inner-left edge, facing into the
          // body. From the body's perspective these are sources, so we use
          // `PinKind.zoneInput`. Body nodes connect to them by dragging a
          // wire from the pin to an input. Positioned *inside* the body
          // Container (left: 0) so the Stack's hit-testing reliably reaches
          // them — pins at negative offset are visible with `Clip.none` but
          // hit-testing through Container/SizedBox parent boundaries was
          // unreliable, sending pointer events to the canvas instead.
          for (int i = 0; i < zone.zoneInputPins.length; i++)
            Positioned(
              left: 0,
              top: pinY(i),
              child: PinWidget(
                pinReference: PinReference(
                  nodeId: nodeId,
                  scopeChain: scopeChain,
                  pinKind: PinKind.zoneInput,
                  pinIndex: i,
                  dataType: zone.zoneInputPins[i].effectiveDataType,
                ),
                multi: false,
                pinName: zone.zoneInputPins[i].name,
              ),
            ),
          // Zone-output pins on the body's inner-right edge. These accept
          // incoming body-return wires (drag from a body node's output here).
          for (int i = 0; i < zone.zoneOutputPins.length; i++)
            Positioned(
              right: 0,
              top: pinY(i),
              child: PinWidget(
                pinReference: PinReference(
                  nodeId: nodeId,
                  scopeChain: scopeChain,
                  pinKind: PinKind.zoneOutput,
                  pinIndex: i,
                  dataType: zone.zoneOutputPins[i].dataType,
                ),
                multi: zone.zoneOutputPins[i].multi,
                pinName: zone.zoneOutputPins[i].name,
              ),
            ),
          // Bottom-right resize handle. Drags update stored width/height.
          // The drag accumulator is held in a stateful sibling widget below so
          // we can mutate the drag delta without rebuilding the parent.
          Positioned(
            right: 0,
            bottom: 0,
            child: _BodyResizeHandle(
              initialSize: effectiveSize,
              onResize: onResize,
              onResizeStart: onResizeStart,
              onResizeEnd: onResizeEnd,
            ),
          ),
        ],
      ),
    );
  }
}

/// Stateful resize handle in the body's bottom-right corner. Maintains the
/// in-flight drag delta locally so the model only gets one update per drag
/// commit (begin/end coalescing — see `doc/design_zones_ui.md` §"Resize
/// handles"). Per-frame intermediate updates also push to the model so the
/// body grows live during the drag.
class _BodyResizeHandle extends StatefulWidget {
  final Size initialSize;
  final ValueChanged<Size> onResize;
  final VoidCallback onResizeStart;
  final VoidCallback onResizeEnd;

  const _BodyResizeHandle({
    required this.initialSize,
    required this.onResize,
    required this.onResizeStart,
    required this.onResizeEnd,
  });

  @override
  State<_BodyResizeHandle> createState() => _BodyResizeHandleState();
}

class _BodyResizeHandleState extends State<_BodyResizeHandle> {
  Size? _dragSize;
  bool _hovered = false;
  bool _dragging = false;

  @override
  Widget build(BuildContext context) {
    // Brighten the handle while the user hovers or actively drags it — gives
    // the same feedback as the pointer cursor change but stays visible after
    // the cursor leaves the handle bounds during a drag. See
    // `doc/design_zones_ui.md` §"Phase U7" → resize handle polish.
    final bool highlight = _hovered || _dragging;
    final Color fillColor = highlight
        ? Colors.black.withValues(alpha: 0.30)
        : Colors.black.withValues(alpha: 0.12);
    final Color borderColor = highlight
        ? Colors.black.withValues(alpha: 0.70)
        : Colors.black.withValues(alpha: 0.35);
    final double borderWidth = highlight ? 1.5 : 1.0;

    return MouseRegion(
      cursor: SystemMouseCursors.resizeDownRight,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onPanStart: (_) {
          widget.onResizeStart();
          setState(() {
            _dragging = true;
            _dragSize = widget.initialSize;
          });
        },
        onPanUpdate: (details) {
          final base = _dragSize ?? widget.initialSize;
          // The handle sits at the body's bottom-right corner; deltas grow
          // the body. Floor at 100x60 so the body never collapses below
          // its inner pins. The Rust API clamps to the same minimum.
          final next = Size(
            (base.width + details.delta.dx).clamp(100.0, 4000.0),
            (base.height + details.delta.dy).clamp(60.0, 4000.0),
          );
          _dragSize = next;
          widget.onResize(next);
        },
        onPanEnd: (_) {
          widget.onResizeEnd();
          setState(() {
            _dragging = false;
            _dragSize = null;
          });
        },
        child: Container(
          width: BASE_HOF_BODY_RESIZE_HANDLE_SIZE,
          height: BASE_HOF_BODY_RESIZE_HANDLE_SIZE,
          decoration: BoxDecoration(
            color: fillColor,
            border: Border.all(color: borderColor, width: borderWidth),
            borderRadius: const BorderRadius.only(
              bottomRight: Radius.circular(4),
            ),
          ),
        ),
      ),
    );
  }
}

/// Compact stand-in for an HOF's body region when the body is collapsed by
/// the zoom-level readability check (see `doc/design_zones_ui.md`
/// §"Zoom levels"). Renders the same translucent rectangle as the live body
/// region so the HOF's overall footprint doesn't shift, but replaces the
/// inner pins and content with a centered "[N nodes]" label. Body nodes and
/// intra-body wires are hidden by the recursive walks in
/// `node_network.dart` / `node_network_painter.dart` when the same body
/// chain is collapsed.
class _ZoneCollapsedPlaceholder extends StatelessWidget {
  final int nodeCount;
  final Size effectiveSize;

  const _ZoneCollapsedPlaceholder({
    required this.nodeCount,
    required this.effectiveSize,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      width: effectiveSize.width,
      height: effectiveSize.height,
      margin: const EdgeInsets.only(top: BASE_HOF_BODY_TOP_OFFSET - 30),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surface,
        border: Border.all(
          color: HOF_BODY_BORDER_COLOR,
          width: 1.0,
        ),
        borderRadius: BorderRadius.circular(6.0),
      ),
      child: Center(
        child: Text(
          '[$nodeCount nodes]',
          style: const TextStyle(
            color: HOF_BODY_PLACEHOLDER_TEXT_COLOR,
            fontSize: 11,
            fontStyle: FontStyle.italic,
          ),
          textAlign: TextAlign.center,
        ),
      ),
    );
  }
}

/// Placeholder shown in place of an HOF's body region when its `f` (function)
/// input pin is wired: the inline body is ignored at eval time, so it is
/// hidden and this note makes the wired closure the one obvious source of
/// truth. The body content is skipped by the recursive walks (the scope
/// resolver flags such bodies collapsed). See `doc/design_closures.md`.
class _ZoneFunctionOverridePlaceholder extends StatelessWidget {
  final Size effectiveSize;

  const _ZoneFunctionOverridePlaceholder({required this.effectiveSize});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: effectiveSize.width,
      height: effectiveSize.height,
      margin: const EdgeInsets.only(top: BASE_HOF_BODY_TOP_OFFSET - 30),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surface,
        border: Border.all(
          // Amber-tinted to echo the Function wire color.
          color: HOF_BODY_FUNCTION_OVERRIDE_BORDER_COLOR,
          width: 1.0,
        ),
        borderRadius: BorderRadius.circular(6.0),
      ),
      child: const Center(
        child: Padding(
          padding: EdgeInsets.symmetric(horizontal: 6),
          child: Text(
            'body ignored\n— driven by `f` —',
            style: TextStyle(
              color: HOF_BODY_PLACEHOLDER_TEXT_COLOR,
              fontSize: 11,
              fontStyle: FontStyle.italic,
            ),
            textAlign: TextAlign.center,
          ),
        ),
      ),
    );
  }
}

/// Paints the minor/major grid inside an HOF body region so the body reads as
/// a sub-canvas, matching the main network grid (`node_network_painter.dart`'s
/// `_drawGrid`). Drawn in the body's local coordinate frame starting at its
/// top-left; phase alignment with the global canvas grid is irrelevant (the
/// body is its own coordinate space) and invisible. The body region only
/// renders at normal zoom (scale 1.0), so spacings are used as-is with no
/// scaling. Reuses the canvas grid constants for an exact color/spacing match.
class _BodyGridPainter extends CustomPainter {
  const _BodyGridPainter();

  @override
  void paint(Canvas canvas, Size size) {
    final minorPaint = Paint()
      ..color = GRID_MINOR_COLOR
      ..strokeWidth = 1.0;
    final majorPaint = Paint()
      ..color = GRID_MAJOR_COLOR
      ..strokeWidth = 1.0;

    bool isMajor(double v) => (v % GRID_MAJOR_SPACING).abs() < 0.01;

    for (double x = 0; x <= size.width; x += GRID_MINOR_SPACING) {
      canvas.drawLine(Offset(x, 0), Offset(x, size.height),
          isMajor(x) ? majorPaint : minorPaint);
    }
    for (double y = 0; y <= size.height; y += GRID_MINOR_SPACING) {
      canvas.drawLine(Offset(0, y), Offset(size.width, y),
          isMajor(y) ? majorPaint : minorPaint);
    }
  }

  @override
  bool shouldRepaint(covariant _BodyGridPainter oldDelegate) => false;
}
