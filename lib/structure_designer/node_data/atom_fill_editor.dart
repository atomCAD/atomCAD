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

  @override
  void initState() {
    super.initState();
    _definitionController = CodeController(
      text: widget.data?.parameterElementValueDefinition ?? '',
      // No language specified - will use plain text by default
    );
    _definitionFocusNode = FocusNode();
    _motifOffset = widget.data?.motifOffset ?? APIVec3(x: 0.0, y: 0.0, z: 0.0);
    _hydrogenPassivation = widget.data?.hydrogenPassivation ?? true;
    _removeSingleBondAtomsBeforePassivation =
        widget.data?.removeSingleBondAtomsBeforePassivation ?? false;
    _surfaceReconstruction = widget.data?.surfaceReconstruction ?? false;
  }

  @override
  void didUpdateWidget(AtomFillEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data?.parameterElementValueDefinition !=
        widget.data?.parameterElementValueDefinition) {
      _definitionController.text =
          widget.data?.parameterElementValueDefinition ?? '';
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
  }

  @override
  void dispose() {
    _definitionController.dispose();
    _definitionFocusNode.dispose();
    super.dispose();
  }

  void _applyChanges() {
    widget.model.setAtomFillData(
      widget.nodeId,
      APIAtomFillData(
        parameterElementValueDefinition: _definitionController.text,
        motifOffset: _motifOffset,
        hydrogenPassivation: _hydrogenPassivation,
        removeSingleBondAtomsBeforePassivation:
            _removeSingleBondAtomsBeforePassivation,
        surfaceReconstruction: _surfaceReconstruction,
        error: null, // This will be set by the backend after parsing
      ),
    );
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
                    color: Theme.of(context).colorScheme.surfaceVariant,
                    borderRadius: const BorderRadius.only(
                      topLeft: Radius.circular(4.0),
                      topRight: Radius.circular(4.0),
                    ),
                  ),
                  child: Text(
                    'Parameter Element Value Definition',
                    style: Theme.of(context).textTheme.labelMedium?.copyWith(
                          color: Theme.of(context).colorScheme.onSurfaceVariant,
                        ),
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
            },
          ),

          const SizedBox(height: 8),

          // Remove single-bond atoms checkbox
          CheckboxListTile(
            title: const Text('Remove single-bond atoms'),
            value: _removeSingleBondAtomsBeforePassivation,
            onChanged: (value) {
              setState(() {
                _removeSingleBondAtomsBeforePassivation = value ?? false;
              });
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
              setState(() {
                _surfaceReconstruction = value ?? false;
              });
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
              setState(() {
                _hydrogenPassivation = value ?? true;
              });
            },
            controlAffinity: ListTileControlAffinity.leading,
          ),

          const SizedBox(height: 8),

          // Apply button
          SizedBox(
            width: double.infinity,
            child: ElevatedButton(
              onPressed: _applyChanges,
              child: const Text('Apply'),
            ),
          ),

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
