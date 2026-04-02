import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/select_element_widget.dart';

/// A reusable WYSIWYG editor for motif parameter element overrides.
///
/// Displays a table of available motif parameters, each with their default
/// element and a dropdown to override it. When no parameters are available
/// (e.g., no motif connected), shows a placeholder message.
///
/// Used by atom_fill node editor and (future) motif parameter override node.
class ParameterElementOverrideEditor extends StatefulWidget {
  /// Parameters from the connected motif (populated after evaluation)
  final List<APIMotifParameterInfo> availableParameters;

  /// The current override definition text (e.g., "PRIMARY Si\nSECONDARY Ge")
  final String currentDefinitionText;

  /// Called with the new definition text when overrides change
  final ValueChanged<String> onChanged;

  const ParameterElementOverrideEditor({
    super.key,
    required this.availableParameters,
    required this.currentDefinitionText,
    required this.onChanged,
  });

  @override
  State<ParameterElementOverrideEditor> createState() =>
      _ParameterElementOverrideEditorState();
}

class _ParameterElementOverrideEditorState
    extends State<ParameterElementOverrideEditor> {
  List<ElementSummary>? _elements;

  @override
  void initState() {
    super.initState();
    _loadElements();
  }

  Future<void> _loadElements() async {
    try {
      final elements = getAllElements();
      setState(() {
        _elements = elements;
      });
    } catch (e) {
      setState(() {
        _elements = [];
      });
      debugPrint('Error loading elements: $e');
    }
  }

  /// Parse the definition text into a map of parameter name -> atomic number.
  Map<String, int> _parseOverrides() {
    final overrides = <String, int>{};
    if (_elements == null) return overrides;

    // Build symbol -> atomic number lookup
    final symbolToNumber = <String, int>{};
    for (final e in _elements!) {
      symbolToNumber[e.symbol.toUpperCase()] = e.atomicNumber;
    }

    for (final line in widget.currentDefinitionText.split('\n')) {
      final trimmed = line.trim();
      if (trimmed.isEmpty || trimmed.startsWith('#')) continue;
      final parts = trimmed.split(RegExp(r'\s+'));
      if (parts.length >= 2) {
        final name = parts[0];
        final symbol = parts[1].toUpperCase();
        final atomicNumber = symbolToNumber[symbol];
        if (atomicNumber != null) {
          overrides[name] = atomicNumber;
        }
      }
    }
    return overrides;
  }

  /// Serialize the overrides map back to definition text.
  String _serializeOverrides(Map<String, int> overrides) {
    if (_elements == null || overrides.isEmpty) return '';

    // Build atomic number -> symbol lookup
    final numberToSymbol = <int, String>{};
    for (final e in _elements!) {
      numberToSymbol[e.atomicNumber] = e.symbol;
    }

    final lines = <String>[];
    for (final entry in overrides.entries) {
      final symbol = numberToSymbol[entry.value];
      if (symbol != null) {
        lines.add('${entry.key} $symbol');
      }
    }
    return lines.join('\n');
  }

  void _onOverrideChanged(
      String paramName, int defaultAtomicNumber, int? newAtomicNumber) {
    final overrides = _parseOverrides();

    if (newAtomicNumber == null || newAtomicNumber == defaultAtomicNumber) {
      // Remove override (use default)
      overrides.remove(paramName);
    } else {
      overrides[paramName] = newAtomicNumber;
    }

    widget.onChanged(_serializeOverrides(overrides));
  }

  @override
  Widget build(BuildContext context) {
    if (widget.availableParameters.isEmpty) {
      return const SizedBox.shrink();
    }

    final overrides = _parseOverrides();

    return Container(
      decoration: BoxDecoration(
        border: Border.all(color: Theme.of(context).colorScheme.outline),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Header
          Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: BoxDecoration(
              color: Theme.of(context).colorScheme.surfaceContainerHighest,
              borderRadius: const BorderRadius.only(
                topLeft: Radius.circular(4),
                topRight: Radius.circular(4),
              ),
            ),
            child: Text(
              'Parameter Element Overrides',
              style: Theme.of(context).textTheme.labelMedium?.copyWith(
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
            ),
          ),
          // Parameter rows
          ...widget.availableParameters.map((param) {
            final currentOverride = overrides[param.name];
            // If override equals default, treat as no override
            final effectiveOverride = (currentOverride != null &&
                    currentOverride != param.defaultAtomicNumber)
                ? currentOverride
                : null;

            return Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
              child: Row(
                children: [
                  // Parameter name
                  SizedBox(
                    width: 120,
                    child: Text(
                      param.name,
                      style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                            fontWeight: FontWeight.w600,
                          ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  const SizedBox(width: 8),
                  // Element dropdown (reuses SelectElementWidget for UX consistency)
                  Expanded(
                    child: SelectElementWidget(
                      value: effectiveOverride,
                      nullLabel: 'Default (${param.defaultElementSymbol})',
                      onChanged: (newValue) {
                        _onOverrideChanged(
                          param.name,
                          param.defaultAtomicNumber,
                          newValue,
                        );
                      },
                    ),
                  ),
                ],
              ),
            );
          }),
          const SizedBox(height: 6),
        ],
      ),
    );
  }
}
