import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/common/parameter_element_override_editor.dart';
import 'package:flutter_cad/common/passivant_dropdown.dart';

/// Index of the `regions` input pin on `materialize`.
/// 0 = shape, 1 = passivate, 2 = rm_single, 3 = surf_recon,
/// 4 = invert_phase, 5 = rm_unbonded, 6 = regions, 7 = passiv_elem.
const int _REGIONS_PIN_INDEX = 6;

/// Index of the `passiv_elem` input pin on `materialize` (appended last, D4).
const int _PASSIV_ELEM_PIN_INDEX = 7;

/// Editor widget for materialize nodes
class MaterializeEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIMaterializeData? data;
  final StructureDesignerModel model;

  const MaterializeEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<MaterializeEditor> createState() => _MaterializeEditorState();
}

class _MaterializeEditorState extends State<MaterializeEditor> {
  late String _parameterElementValueDefinition;
  late bool _hydrogenPassivation;
  late bool _removeUnbondedAtoms;
  late bool _removeSingleBondAtomsBeforePassivation;
  late bool _surfaceReconstruction;
  late bool _invertPhase;
  late int _passivationElement;

  /// True when the optional `passiv_elem` input pin is wired. When connected,
  /// the wired value replaces the stored element at eval, so the dropdown
  /// renders disabled (standard "disable on wired input" pattern) — unlike the
  /// annotate-only `regions` pin which augments rather than replaces.
  bool _isPassivElemPinConnected() {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId &&
          wire.destParamIndex == BigInt.from(_PASSIV_ELEM_PIN_INDEX)) {
        return true;
      }
    }
    return false;
  }

  /// True when the optional `regions` input pin is wired. When connected, the
  /// per-region records layer *on top of* these node settings (the "root"),
  /// so the checkboxes stay enabled — see doc/design_blueprint_region_atom_edits.md §B7.
  bool _isRegionsPinConnected() {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId &&
          wire.destParamIndex == BigInt.from(_REGIONS_PIN_INDEX)) {
        return true;
      }
    }
    return false;
  }

  void _commitChanges() {
    if (widget.data == null) {
      return;
    }

    widget.model.setMaterializeData(
      widget.nodeId,
      APIMaterializeData(
        parameterElementValueDefinition: _parameterElementValueDefinition,
        hydrogenPassivation: _hydrogenPassivation,
        removeUnbondedAtoms: _removeUnbondedAtoms,
        removeSingleBondAtomsBeforePassivation:
            _removeSingleBondAtomsBeforePassivation,
        surfaceReconstruction: _surfaceReconstruction,
        invertPhase: _invertPhase,
        passivationElement: _passivationElement,
        error: null,
        availableParameters: const [],
      ),
    );
  }

  @override
  void initState() {
    super.initState();
    _parameterElementValueDefinition =
        widget.data?.parameterElementValueDefinition ?? '';
    _hydrogenPassivation = widget.data?.hydrogenPassivation ?? true;
    _removeUnbondedAtoms = widget.data?.removeUnbondedAtoms ?? true;
    _removeSingleBondAtomsBeforePassivation =
        widget.data?.removeSingleBondAtomsBeforePassivation ?? false;
    _surfaceReconstruction = widget.data?.surfaceReconstruction ?? false;
    _invertPhase = widget.data?.invertPhase ?? false;
    _passivationElement = widget.data?.passivationElement ?? 1;
  }

  @override
  void didUpdateWidget(MaterializeEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data?.parameterElementValueDefinition !=
        widget.data?.parameterElementValueDefinition) {
      _parameterElementValueDefinition =
          widget.data?.parameterElementValueDefinition ?? '';
    }
    if (oldWidget.data?.hydrogenPassivation !=
        widget.data?.hydrogenPassivation) {
      _hydrogenPassivation = widget.data?.hydrogenPassivation ?? true;
    }
    if (oldWidget.data?.removeUnbondedAtoms !=
        widget.data?.removeUnbondedAtoms) {
      _removeUnbondedAtoms = widget.data?.removeUnbondedAtoms ?? true;
    }
    if (oldWidget.data?.removeSingleBondAtomsBeforePassivation !=
        widget.data?.removeSingleBondAtomsBeforePassivation) {
      _removeSingleBondAtomsBeforePassivation =
          widget.data?.removeSingleBondAtomsBeforePassivation ?? false;
    }
    if (oldWidget.data?.surfaceReconstruction !=
        widget.data?.surfaceReconstruction) {
      _surfaceReconstruction = widget.data?.surfaceReconstruction ?? false;
    }
    if (oldWidget.data?.invertPhase != widget.data?.invertPhase) {
      _invertPhase = widget.data?.invertPhase ?? false;
    }
    if (oldWidget.data?.passivationElement !=
        widget.data?.passivationElement) {
      _passivationElement = widget.data?.passivationElement ?? 1;
    }
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Materialize Properties',
            nodeTypeName: 'materialize',
          ),
          const SizedBox(height: 8),

          // Parameter Element Override Editor (WYSIWYG)
          ParameterElementOverrideEditor(
            availableParameters: widget.data!.availableParameters,
            currentDefinitionText: _parameterElementValueDefinition,
            onChanged: (newText) {
              setState(() {
                _parameterElementValueDefinition = newText;
              });
              _commitChanges();
            },
          ),

          const SizedBox(height: 8),

          // Annotation: when `regions` is wired, the per-region records override
          // these settings inside their volumes; the checkboxes below remain the
          // root (and stay enabled). See design doc §B7.
          if (_isRegionsPinConnected())
            Padding(
              padding: const EdgeInsets.only(bottom: 8.0),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(
                    Icons.layers_outlined,
                    size: 16,
                    color: Theme.of(context).colorScheme.primary,
                  ),
                  const SizedBox(width: 6),
                  const Expanded(
                    child: Text(
                      'Regions override these settings inside their volumes.',
                      style: TextStyle(fontStyle: FontStyle.italic, fontSize: 12),
                    ),
                  ),
                ],
              ),
            ),

          // Remove unbonded atoms checkbox
          CheckboxListTile(
            title: const Text('Remove unbonded atoms'),
            subtitle:
                const Text('Remove atoms left with no bonds after the cut'),
            value: _removeUnbondedAtoms,
            onChanged: (value) {
              final newValue = value ?? true;
              setState(() {
                _removeUnbondedAtoms = newValue;
              });

              _commitChanges();
            },
            controlAffinity: ListTileControlAffinity.leading,
          ),

          const SizedBox(height: 8),

          // Remove single-bond atoms checkbox
          CheckboxListTile(
            title: const Text('Remove single-bond atoms'),
            subtitle: const Text(
              'Recursive; also removes unbonded atoms regardless of the option above',
            ),
            value: _removeSingleBondAtomsBeforePassivation,
            onChanged: (value) {
              final newValue = value ?? false;
              setState(() {
                _removeSingleBondAtomsBeforePassivation = newValue;
              });

              _commitChanges();
            },
            controlAffinity: ListTileControlAffinity.leading,
          ),

          const SizedBox(height: 8),

          // Surface Reconstruction checkbox
          CheckboxListTile(
            title: const Text('Surface Reconstruction'),
            subtitle: const Text('Apply (100) 2x1 dimer reconstruction'),
            value: _surfaceReconstruction,
            onChanged: (value) {
              final newValue = value ?? false;
              setState(() {
                _surfaceReconstruction = newValue;
              });

              _commitChanges();
            },
            controlAffinity: ListTileControlAffinity.leading,
          ),

          const SizedBox(height: 8),

          CheckboxListTile(
            title: const Text('Invert Phase'),
            subtitle: const Text('Swap the surface reconstruction phase (A/B)'),
            value: _invertPhase,
            onChanged: (value) {
              final newValue = value ?? false;
              setState(() {
                _invertPhase = newValue;
              });

              _commitChanges();
            },
            controlAffinity: ListTileControlAffinity.leading,
          ),

          const SizedBox(height: 8),

          // Passivation checkbox
          CheckboxListTile(
            title: const Text('Passivation'),
            subtitle: const Text(
                'Add terminating atoms to passivate dangling bonds'),
            value: _hydrogenPassivation,
            onChanged: (value) {
              final newValue = value ?? true;
              setState(() {
                _hydrogenPassivation = newValue;
              });

              _commitChanges();
            },
            controlAffinity: ListTileControlAffinity.leading,
          ),

          const SizedBox(height: 8),

          // Passivant element dropdown. A wired `passiv_elem` pin replaces the
          // stored element at eval, so disable inline editing while connected
          // (but keep the stored value for re-activation on disconnect).
          Builder(
            builder: (context) {
              final connected = _isPassivElemPinConnected();
              return Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (connected)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 4.0),
                      child: Text(
                        'Passivant element supplied by `passiv_elem` input. '
                        'Disconnect to edit inline.',
                        style: TextStyle(
                          fontStyle: FontStyle.italic,
                          fontSize: 12,
                          color: Theme.of(context).colorScheme.primary,
                        ),
                      ),
                    ),
                  Opacity(
                    opacity: connected ? 0.5 : 1.0,
                    child: IgnorePointer(
                      ignoring: connected,
                      child: PassivantDropdown(
                        value: _passivationElement,
                        onChanged: (newValue) {
                          setState(() {
                            _passivationElement = newValue;
                          });
                          _commitChanges();
                        },
                      ),
                    ),
                  ),
                ],
              );
            },
          ),

          const SizedBox(height: 8),

          // Error message display
          if (widget.data?.error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8.0),
              child: Container(
                width: double.infinity,
                padding: const EdgeInsets.all(8.0),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.errorContainer,
                  borderRadius: BorderRadius.circular(4.0),
                  border: Border.all(
                    color: Theme.of(context).colorScheme.error,
                    width: 1.0,
                  ),
                ),
                child: Text(
                  widget.data!.error!,
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.onErrorContainer,
                    fontSize: 12.0,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
