/// The move-wires drag (`doc/design_move_wires.md`, Phase 2): its state and
/// the pure functions the model and the painters build on.
///
/// Rust owns every rule — which wires a pick-up holds (`sd_api.pinWires`) and
/// which pins may take them (`sd_api.moveWiresRefusal`). What is here only
/// names pins and wires and works out what to draw, and is widget-free, so
/// `test/wire_move_test.dart` runs it over hand-built API data classes with no
/// Rust library.
library;

import 'dart:ui' show Offset;

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_painter.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// A move drag in progress: the wires of [from], held while the pointer
/// looks for a pin to drop them on.
///
/// Kept apart from `StructureDesignerModel.draggedWire` on purpose: with that
/// left null the open-port tabs stay hidden and the source pin's `onDragEnd`
/// does not open the Add Node popup — neither fits a drag that can hold
/// several wires.
class WireMove {
  WireMove({required this.from, required this.ends, required this.cursor})
      : heldKeys = {for (final end in ends) heldWireKey(end)};

  /// The pin the wires were picked up from.
  final PinReference from;

  /// What `sd_api.pinWires` answered at the drag start: one entry per held
  /// wire, with the end that stays put.
  final List<APIWireEnd> ends;

  /// [heldWireKey] of every entry of [ends]: the wires the painters skip.
  final Set<String> heldKeys;

  /// The pointer, in canvas-local pixels.
  Offset cursor;

  /// An output under the pointer that accepts a move held by its input end
  /// (D16). While set, the painters draw the wire as it will be — from this
  /// output to [from] — instead of the rubber band from the old source.
  PinReference? hoveredTarget;

  /// Escape was pressed. `Draggable` cannot be cancelled from outside, so the
  /// drag runs on; the held wires are drawn again, no pin accepts, and the
  /// release does nothing.
  bool cancelled = false;
}

/// One curve a move drag draws, from a wire's source end to its destination
/// end, coloured by [dataType].
class WireMoveSegment {
  const WireMoveSegment(this.source, this.dest, this.dataType);

  final Offset source;
  final Offset dest;
  final String dataType;

  @override
  bool operator ==(Object other) =>
      other is WireMoveSegment &&
      other.source == source &&
      other.dest == dest &&
      other.dataType == dataType;

  @override
  int get hashCode => Object.hash(source, dest, dataType);

  @override
  String toString() => 'WireMoveSegment($source → $dest, $dataType)';
}

/// The memo key of a drop target: the pin's full identity. Body node ids
/// repeat across scopes, so the scope chain is part of it; the data type is
/// not, since it does not tell two pins apart.
String pinKey(PinReference pin) =>
    '${pin.scopeChain.join('/')}|${pin.nodeId}|${pin.pinKind.name}|'
    '${pin.pinIndex}';

/// The key of a held wire, as the painters key the wires they draw
/// ([WireHitResult.keyOf]). The two must agree, or the held wire is drawn
/// twice, or another wire is hidden in its place.
String heldWireKey(APIWireEnd end) => WireHitResult.storageKey(
      end.storageScope.toList(),
      end.anchor.sourceNodeId,
      end.anchor.destinationNodeId,
      end.anchor.destinationArgumentKind,
      end.anchor.destinationArgumentIndex,
    );

/// [pin] as the move API addresses it.
APIPinAddress pinAddressOf(PinReference pin) {
  final APIPinSide side;
  switch (pin.pinKind) {
    case PinKind.externalInput:
      side = APIPinSide.input(
          kind: APIArgumentKind.external_, index: pin.pinIndex);
    case PinKind.zoneOutput:
      side = APIPinSide.input(
          kind: APIArgumentKind.zoneOutput, index: pin.pinIndex);
    case PinKind.externalOutput:
      side = APIPinSide.output(
          pin: APISourcePin.nodeOutput(pinIndex: pin.pinIndex));
    case PinKind.functionPin:
      side =
          const APIPinSide.output(pin: APISourcePin.nodeOutput(pinIndex: -1));
    case PinKind.zoneInput:
      side = APIPinSide.output(
          pin: APISourcePin.zoneInput(pinIndex: pin.pinIndex));
  }
  return APIPinAddress(
      scope: scopeChainToBytes(pin.scopeChain), nodeId: pin.nodeId, side: side);
}

/// The pin [address] names, for the resolver. Its data type is unknown here
/// and left empty; the resolver reports the pin's own.
PinReference pinReferenceOf(APIPinAddress address) {
  final scopeChain = address.scope.toList();
  final side = address.side;
  final PinKind kind;
  final int index;
  switch (side) {
    case APIPinSide_Input(kind: final argumentKind, index: final i):
      kind = argumentKind == APIArgumentKind.zoneOutput
          ? PinKind.zoneOutput
          : PinKind.externalInput;
      index = i;
    case APIPinSide_Output(pin: APISourcePin_ZoneInput(pinIndex: final i)):
      kind = PinKind.zoneInput;
      index = i;
    case APIPinSide_Output(pin: APISourcePin_NodeOutput(pinIndex: final i)):
      kind = i == -1 ? PinKind.functionPin : PinKind.externalOutput;
      index = i;
  }
  return PinReference(
    nodeId: address.nodeId,
    scopeChain: scopeChain,
    pinKind: kind,
    pinIndex: index,
    dataType: '',
  );
}

/// What the painters draw for [move]. [pinPosition] is the resolver's
/// `tryPinScreenPosition`: a pin's screen position and data type, or null
/// when it is not on the canvas.
///
/// - A cancelled move draws nothing (the held wires are drawn as usual).
/// - Over an output that accepts a move held by its input end (D16): one
///   curve, from that output to the starting input — the wire as it will be.
/// - Otherwise one rubber band per held wire, from its fixed end to the
///   cursor, in the wire's own colour: the source's type, which is the fixed
///   end's for an input-side pick-up and the starting pin's for an output-side
///   one.
List<WireMoveSegment> wireMoveSegments(
    WireMove move, (Offset, String)? Function(PinReference pin) pinPosition) {
  if (move.cancelled) return const [];
  final hovered = move.hoveredTarget;
  if (hovered != null) {
    final source = pinPosition(hovered);
    final dest = pinPosition(move.from);
    if (source == null || dest == null) return const [];
    return [WireMoveSegment(source.$1, dest.$1, source.$2)];
  }
  final segments = <WireMoveSegment>[];
  for (final end in move.ends) {
    final fixed = pinPosition(pinReferenceOf(end.fixedEnd));
    if (fixed == null) continue;
    segments.add(move.from.isOutput
        ? WireMoveSegment(move.cursor, fixed.$1, move.from.dataType)
        : WireMoveSegment(fixed.$1, move.cursor, fixed.$2));
  }
  return segments;
}
