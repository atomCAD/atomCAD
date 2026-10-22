/// Take type from wire, in the property panel
/// (`doc/design_wire_declared_inputs.md`): the marker and the **Use wire's
/// type** button a declared-input row gets when its wire now carries a type
/// the input was not declared with. Shared by the `expr` parameter list and
/// the anonymous `record_construct` field list.
///
/// Which rows get an offer, and with which type, is Rust's
/// (`model.getWireTypeOffers`, one entry per input pin = editor row). The
/// editors fetch the offers on every build: a cached copy would go stale
/// exactly when upstream changes, which is the case they exist for.
library;

import 'package:flutter/material.dart';

import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/inputs/type_editor_dialog.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// The offer for editor row [index], or null. Tolerates an offer list shorter
/// than the rows (an empty list means the node has no declared inputs).
APIDataType? wireTypeOfferAt(List<APIDataType?> offers, int index) =>
    index < offers.length ? offers[index] : null;

/// The warning marker beside a stale row's type selector.
class WireTypeOfferMarker extends StatelessWidget {
  const WireTypeOfferMarker({super.key});

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: 'The wire into this input carries a different type',
      child: Icon(Icons.warning_amber_rounded,
          size: 18, color: Colors.amber.shade700),
    );
  }
}

/// **Use wire's type: `<type>`** under a stale row: retypes input [index] of
/// [nodeId] to [offeredType] in one undo step. The type is printed by the
/// same formatter as the type selector.
class WireTypeOfferButton extends StatelessWidget {
  final StructureDesignerModel model;
  final BigInt nodeId;
  final int index;
  final APIDataType offeredType;

  const WireTypeOfferButton({
    super.key,
    required this.model,
    required this.nodeId,
    required this.index,
    required this.offeredType,
  });

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.centerLeft,
      child: TextButton.icon(
        onPressed: () {
          final error = model.takeTypesFromWires(nodeId, [index]);
          if (error != null) showErrorSnackBar(context, error);
        },
        icon: Icon(Icons.call_merge, size: 16, color: Colors.amber.shade800),
        label: Text(
          "Use wire's type: ${apiDataTypeToString(offeredType)}",
          overflow: TextOverflow.ellipsis,
        ),
        style: TextButton.styleFrom(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
          minimumSize: Size.zero,
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
        ),
      ),
    );
  }
}

/// The rename chip under a just-dropped input's row: the name derived from the
/// wire's source, when the node named the input otherwise (`expr` hands out
/// letters). One click renames; typing a name or selecting another node
/// dismisses it (`StructureDesignerModel.nameSuggestion`).
class NameSuggestionChip extends StatelessWidget {
  final String name;
  final VoidCallback onUse;

  const NameSuggestionChip(
      {super.key, required this.name, required this.onUse});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: 2.0),
      child: Align(
        alignment: Alignment.centerLeft,
        child: ActionChip(
          avatar: const Icon(Icons.drive_file_rename_outline, size: 16),
          label: Text('Name after source: $name'),
          tooltip: 'Rename this input to the name of the node it is wired to',
          onPressed: onUse,
          visualDensity: VisualDensity.compact,
          materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
        ),
      ),
    );
  }
}
