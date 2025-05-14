import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/geo_to_atom_api.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// A widget that allows selection of a crystal type from a dropdown
///
/// The dropdown displays crystal type names but represents them by their primary and secondary atomic numbers.
/// The widget takes a pair of atomic numbers as its value and fires an event when
/// a new crystal type is selected.
class SelectCrystalTypeWidget extends StatefulWidget {
  /// The currently selected primary atomic number
  final int? primaryAtomicNumber;

  /// The currently selected secondary atomic number
  final int? secondaryAtomicNumber;

  /// Callback fired when a crystal type is selected
  /// The parameters are the primary and secondary atomic numbers of the selected crystal type
  final void Function(int, int) onChanged;

  /// Optional hint text to display when no crystal type is selected
  final String? hint;

  /// Optional label text to display above the dropdown
  final String? label;

  const SelectCrystalTypeWidget({
    Key? key,
    this.primaryAtomicNumber,
    this.secondaryAtomicNumber,
    required this.onChanged,
    this.hint,
    this.label,
  }) : super(key: key);

  @override
  State<SelectCrystalTypeWidget> createState() =>
      _SelectCrystalTypeWidgetState();
}

class _SelectCrystalTypeWidgetState extends State<SelectCrystalTypeWidget> {
  /// List of all available crystal types
  List<APICrystalTypeInfo>? _crystalTypes;

  /// Flag to track if crystal types are still loading
  bool _loading = true;

  /// The currently selected crystal type key for the dropdown
  String? _selectedKey;

  @override
  void initState() {
    super.initState();
    _loadCrystalTypes();
  }

  @override
  void didUpdateWidget(SelectCrystalTypeWidget oldWidget) {
    super.didUpdateWidget(oldWidget);

    // If the selected atomic numbers changed, update the selected key
    if (widget.primaryAtomicNumber != oldWidget.primaryAtomicNumber ||
        widget.secondaryAtomicNumber != oldWidget.secondaryAtomicNumber) {
      _updateSelectedKey();
    }
  }

  /// Load the list of available crystal types
  Future<void> _loadCrystalTypes() async {
    try {
      // Add a custom entry at the beginning
      final customEntry = APICrystalTypeInfo(
        primaryAtomicNumber: 0,
        secondaryAtomicNumber: 0,
        unitCellSize: 0.0,
        name: 'Custom',
      );

      // Get crystal types from the API
      final crystalTypes = getCrystalTypes();

      // Insert the custom entry at the beginning
      crystalTypes.insert(0, customEntry);

      setState(() {
        _crystalTypes = crystalTypes;
        _loading = false;
        _updateSelectedKey();
      });
    } catch (e) {
      setState(() {
        _loading = false;
      });
      debugPrint('Error loading crystal types: $e');
    }
  }

  /// Update the selected key based on the current atomic numbers
  void _updateSelectedKey() {
    if (_crystalTypes == null ||
        widget.primaryAtomicNumber == null ||
        widget.secondaryAtomicNumber == null) {
      setState(() {
        _selectedKey = null;
      });
      return;
    }

    // Find a matching crystal type
    final matchingCrystal = _crystalTypes!.firstWhere(
      (crystal) =>
          crystal.primaryAtomicNumber == widget.primaryAtomicNumber &&
          crystal.secondaryAtomicNumber == widget.secondaryAtomicNumber,
      orElse: () => _crystalTypes![0], // Default to Custom if no match
    );

    setState(() {
      // Use a composite key of primary and secondary atomic numbers
      _selectedKey =
          '${matchingCrystal.primaryAtomicNumber}_${matchingCrystal.secondaryAtomicNumber}';
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    // Show a progress indicator while loading
    if (_loading) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(widget.label ?? ''),
          const SizedBox(height: 8),
          const LinearProgressIndicator(),
        ],
      );
    }

    // If crystal types failed to load, show an error
    if (_crystalTypes == null) {
      return Text('Failed to load crystal types',
          style: theme.textTheme.bodyMedium
              ?.copyWith(color: theme.colorScheme.error));
    }

    // Build the dropdown items
    final items = _crystalTypes!.map((crystal) {
      final value =
          '${crystal.primaryAtomicNumber}_${crystal.secondaryAtomicNumber}';
      return DropdownMenuItem<String>(
        value: value,
        child: Container(
          constraints: BoxConstraints(
              maxWidth: 300), // Constrain width to prevent overflow
          child: Text(
            crystal.name,
            softWrap: true, // Allow text to wrap
            overflow: TextOverflow.visible, // Show overflowing text
            style: TextStyle(
              fontSize: 13, // Smaller font size for better fit
              color: Colors.black, // Ensure text is visible on light background
            ),
          ),
        ),
      );
    }).toList();

    return DropdownButtonFormField<String>(
      value: _selectedKey,
      isExpanded: true, // Allow the dropdown to expand to full width
      style: TextStyle(
        fontSize: 13, // Smaller font size for button text
        color: Colors.black, // Ensure text is visible
      ),
      decoration: InputDecoration(
        isDense: true,
        hintText: widget.hint,
        labelText: widget.label,
        border: const OutlineInputBorder(),
        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        hintStyle: TextStyle(fontSize: 13, color: Colors.grey[600]),
        labelStyle: TextStyle(fontSize: 14),
      ),
      dropdownColor: Colors.white, // Set dropdown menu background color
      items: items,
      onChanged: (newValue) {
        if (newValue != null) {
          final parts = newValue.split('_');
          if (parts.length == 2) {
            final primary = int.parse(parts[0]);
            final secondary = int.parse(parts[1]);
            widget.onChanged(primary, secondary);
          }
        }
      },
    );
  }
}
