import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:code_text_field/code_text_field.dart';
import 'package:flutter_highlight/themes/github.dart';

/// Editor widget for atom_fill nodes
class AtomFillEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIAtomFillData? data;
  final StructureDesignerModel model;

  const AtomFillEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<AtomFillEditor> createState() => _AtomFillEditorState();
}

class _AtomFillEditorState extends State<AtomFillEditor> {
  late CodeController _definitionController;
  late FocusNode _definitionFocusNode;
  late APIVec3 _motifOffset;
  late bool _hydrogenPassivation;
  late bool _removeSingleBondAtomsBeforePassivation;
  late bool _surfaceReconstruction;
  late bool _invertPhase;
  bool _definitionDirty = false;

  bool _hasPendingChanges() {
    if (widget.data == null) {
      return false;
    }

    if (_definitionDirty) {
      return true;
    }

    final data = widget.data!;
    if (_motifOffset != data.motifOffset) {
      return true;
    }
    if (_hydrogenPassivation != data.hydrogenPassivation) {
      return true;
    }
    if (_removeSingleBondAtomsBeforePassivation !=
        data.removeSingleBondAtomsBeforePassivation) {
      return true;
    }
    if (_surfaceReconstruction != data.surfaceReconstruction) {
      return true;
    }
    if (_invertPhase != data.invertPhase) {
      return true;
    }

    return false;
  }

  void _commitChanges() {
    if (widget.data == null) {
      return;
    }

    widget.model.setAtomFillData(
      widget.nodeId,
      APIAtomFillData(
        parameterElementValueDefinition: _definitionController.text,
        motifOffset: _motifOffset,
        hydrogenPassivation: _hydrogenPassivation,
        removeSingleBondAtomsBeforePassivation:
            _removeSingleBondAtomsBeforePassivation,
        surfaceReconstruction: _surfaceReconstruction,
        invertPhase: _invertPhase,
        error: null, // This will be set by the backend after parsing
      ),
    );
  }

  @override
  void initState() {
    super.initState();
    _definitionController = CodeController(
      text: widget.data?.parameterElementValueDefinition ?? '',
      // No language specified - will use plain text by default
    );
    _definitionController.addListener(() {
      final committed = widget.data?.parameterElementValueDefinition ?? '';
      final dirty = _definitionController.text != committed;
      if (dirty == _definitionDirty) {
        return;
      }
      setState(() {
        _definitionDirty = dirty;
      });
    });
    _definitionFocusNode = FocusNode();
    _definitionFocusNode.addListener(() {
      if (_definitionFocusNode.hasFocus) {
        return;
      }

      if (!_definitionDirty) {
        return;
      }

      _commitChanges();

      if (_definitionDirty) {
        setState(() {
          _definitionDirty = false;
        });
      }
    });
    _motifOffset = widget.data?.motifOffset ?? APIVec3(x: 0.0, y: 0.0, z: 0.0);
    _hydrogenPassivation = widget.data?.hydrogenPassivation ?? true;
    _removeSingleBondAtomsBeforePassivation =
        widget.data?.removeSingleBondAtomsBeforePassivation ?? false;
    _surfaceReconstruction = widget.data?.surfaceReconstruction ?? false;
    _invertPhase = widget.data?.invertPhase ?? false;
  }

  @override
  void didUpdateWidget(AtomFillEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data?.parameterElementValueDefinition !=
        widget.data?.parameterElementValueDefinition) {
      if (!_definitionFocusNode.hasFocus) {
        _definitionController.text =
            widget.data?.parameterElementValueDefinition ?? '';
      }

      final dirty = _definitionController.text !=
          (widget.data?.parameterElementValueDefinition ?? '');
      if (dirty != _definitionDirty) {
        setState(() {
          _definitionDirty = dirty;
        });
      }
    }
    if (oldWidget.data?.motifOffset != widget.data?.motifOffset) {
      _motifOffset =
          widget.data?.motifOffset ?? APIVec3(x: 0.0, y: 0.0, z: 0.0);
    }
    if (oldWidget.data?.hydrogenPassivation !=
        widget.data?.hydrogenPassivation) {
      _hydrogenPassivation = widget.data?.hydrogenPassivation ?? true;
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
  }

  @override
  void dispose() {
    if (_hasPendingChanges()) {
      _commitChanges();
    }
    _definitionController.dispose();
    _definitionFocusNode.dispose();
    super.dispose();
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
            title: 'Atom Fill Properties',
            nodeTypeName: 'atom_fill',
          ),
          const SizedBox(height: 8),

          // Parameter Element Value Definition text area with line numbers
          Container(
            decoration: BoxDecoration(
              border: Border.all(color: Theme.of(context).colorScheme.outline),
              borderRadius: BorderRadius.circular(4.0),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Label
                Container(
                  width: double.infinity,
                  padding:
                      const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.surfaceContainerHighest,
                    borderRadius: const BorderRadius.only(
                      topLeft: Radius.circular(4.0),
                      topRight: Radius.circular(4.0),
                    ),
                  ),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          'Parameter Element Value Definition',
                          style:
                              Theme.of(context).textTheme.labelMedium?.copyWith(
                                    color: Theme.of(context)
                                        .colorScheme
                                        .onSurfaceVariant,
                                  ),
                        ),
                      ),
                      const SizedBox(width: 8),
                      Tooltip(
                        message: 'Apply definition',
                        child: TextButton.icon(
                          onPressed: _definitionDirty
                              ? () {
                                  _commitChanges();
                                  setState(() {
                                    _definitionDirty = false;
                                  });
                                }
                              : null,
                          icon: const Icon(Icons.check, size: 16),
                          label: const Text('Apply'),
                        ),
                      ),
                    ],
                  ),
                ),
                // Code field
                SizedBox(
                  height: 200,
                  child: CodeTheme(
                    data: CodeThemeData(styles: githubTheme),
                    child: SingleChildScrollView(
                      child: CodeField(
                        controller: _definitionController,
                        focusNode: _definitionFocusNode,
                        textStyle: const TextStyle(
                          fontFamily: 'Courier New',
                          fontFamilyFallback: [
                            'Consolas',
                            'Monaco',
                            'Menlo',
                            'monospace'
                          ],
                          fontSize: 14.0,
                        ),
                        expands: false,
                        wrap: false,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),

          const SizedBox(height: 8),

          // Motif Offset input
          Vec3Input(
            label: 'Motif Offset (fractional coordinates)',
            value: _motifOffset,
            onChanged: (value) {
              setState(() {
                _motifOffset = value;
              });

              _commitChanges();
            },
          ),

          const SizedBox(height: 8),

          // Remove single-bond atoms checkbox
          CheckboxListTile(
            title: const Text('Remove single-bond atoms'),
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
            subtitle: const Text('Apply (100) 2×1 dimer reconstruction'),
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

          // Hydrogen Passivation checkbox
          CheckboxListTile(
            title: const Text('Hydrogen Passivation'),
            subtitle:
                const Text('Add hydrogen atoms to passivate dangling bonds'),
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
