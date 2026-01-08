import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A widget that allows selection of a chemical element from a dropdown
/// 
/// The dropdown displays element names but represents them by their atomic numbers.
/// The widget takes a nullable atomic number as its value and fires an event when
/// a new element is selected.
class SelectElementWidget extends StatefulWidget {
  /// The currently selected atomic number, or null if no element is selected
  final int? value;
  
  /// Callback fired when an element is selected
  /// The parameter is the atomic number of the selected element, or null if selection is cleared
  final ValueChanged<int?> onChanged;
  
  /// Optional hint text to display when no element is selected
  final String? hint;

  /// Optional label text to display above the dropdown
  final String? label;
  
  /// If true, the widget will not allow null selection (no "None" option)
  final bool required;

  const SelectElementWidget({
    super.key,
    this.value,
    required this.onChanged,
    this.hint,
    this.label,
    this.required = false,
  });

  @override
  State<SelectElementWidget> createState() => _SelectElementWidgetState();
}

class _SelectElementWidgetState extends State<SelectElementWidget> {
  /// List of all available chemical elements
  List<ElementSummary>? _elements;
  
  /// Flag to track if elements are still loading
  bool _loading = true;
  
  @override
  void initState() {
    super.initState();
    _loadElements();
  }
  
  /// Load all chemical elements from the Rust API
  Future<void> _loadElements() async {
    setState(() {
      _loading = true;
    });
    
    try {
      final elements = getAllElements();
      setState(() {
        _elements = elements;
        _loading = false;
      });
    } catch (e) {
      setState(() {
        _elements = [];
        _loading = false;
      });
      
      // Only show error in debug console, don't disrupt the UI
      debugPrint('Error loading chemical elements: $e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final items = _buildDropdownItems();
    
    Widget dropdown = DropdownButtonFormField<int?>(
      decoration: InputDecoration(
        isDense: true,
        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        border: const OutlineInputBorder(),
        hintText: widget.hint ?? 'Select an element',
        labelText: widget.label,
        fillColor: Colors.white,
        filled: true,
      ),
      isExpanded: true,
      value: widget.value,
      items: items,
      onChanged: widget.onChanged,
      style: AppTextStyles.inputField.copyWith(color: Colors.black87),
      dropdownColor: Colors.white,
      icon: const Icon(Icons.arrow_drop_down),
      iconEnabledColor: AppColors.primaryAccent,
      menuMaxHeight: 300,
      hint: Text(widget.hint ?? 'Select an element', style: TextStyle(color: Colors.grey[600])),
    );
    
    if (_loading) {
      return Stack(
        alignment: Alignment.center,
        children: [
          Opacity(
            opacity: 0.5,
            child: IgnorePointer(
              child: dropdown,
            ),
          ),
          const SizedBox(
            width: 20,
            height: 20,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ],
      );
    }
    
    return dropdown;
  }
  
  /// Build the dropdown items from the loaded elements
  List<DropdownMenuItem<int?>> _buildDropdownItems() {
    if (_elements == null || _elements!.isEmpty) {
      return [];
    }
    
    final items = <DropdownMenuItem<int?>>[];
    
    // Add a null option for no selection only if not required
    if (!widget.required) {
      items.add(DropdownMenuItem<int?>(
        value: null,
        child: Text('None', style: TextStyle(color: Colors.black87)),
      ));
    }
    
    // Add items for each element
    items.addAll(_elements!.map((element) {
      return DropdownMenuItem<int?>(
        value: element.atomicNumber,
        child: Text(
          element.elementName,
          style: TextStyle(color: Colors.black87),
        ),
      );
    }));
    
    return items;
  }
}
