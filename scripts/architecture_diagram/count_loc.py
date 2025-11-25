"""
Count Lines of Code (LOC) for atomCAD modules.

This script counts non-empty, non-comment lines in Rust and Dart files,
excluding generated files.
"""

import os
import json
from pathlib import Path
from typing import Dict

# Project root (two levels up from this script)
PROJECT_ROOT = Path(__file__).parent.parent.parent

def count_loc_in_file(file_path: Path) -> int:
    """Count non-empty, non-comment lines in a file."""
    count = 0
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            in_block_comment = False
            for line in f:
                stripped = line.strip()
                
                # Skip empty lines
                if not stripped:
                    continue
                
                # Handle Rust block comments
                if '/*' in stripped:
                    in_block_comment = True
                if '*/' in stripped:
                    in_block_comment = False
                    continue
                if in_block_comment:
                    continue
                
                # Skip single-line comments
                if stripped.startswith('//') or stripped.startswith('#'):
                    continue
                
                # Skip Dart comments
                if stripped.startswith('///'):
                    continue
                
                count += 1
    except Exception as e:
        print(f"Warning: Could not read {file_path}: {e}")
    
    return count

def count_rust_module(module_path: Path) -> int:
    """Count LOC in all .rs files in a module directory."""
    total = 0
    if not module_path.exists():
        print(f"Warning: Module path does not exist: {module_path}")
        return 0
    
    for rs_file in module_path.rglob('*.rs'):
        total += count_loc_in_file(rs_file)
    
    return total

def count_flutter_ui() -> int:
    """Count LOC in Flutter UI code, excluding generated files."""
    lib_path = PROJECT_ROOT / 'lib'
    total = 0
    
    if not lib_path.exists():
        print(f"Warning: Flutter lib directory does not exist: {lib_path}")
        return 0
    
    for dart_file in lib_path.rglob('*.dart'):
        # Exclude generated files
        if dart_file.name.endswith('.g.dart'):
            continue
        if dart_file.name.endswith('.freezed.dart'):
            continue
        
        total += count_loc_in_file(dart_file)
    
    return total

def count_all_modules() -> Dict[str, int]:
    """Count LOC for all atomCAD modules."""
    rust_src = PROJECT_ROOT / 'rust' / 'src'
    
    modules = {}
    
    # Rust modules
    rust_modules = {
        'structure_designer': [
            rust_src / 'structure_designer',
            rust_src / 'api'  # Include API in structure_designer
        ],
        'crystolecule': [rust_src / 'crystolecule'],
        'renderer': [rust_src / 'renderer'],
        'display': [rust_src / 'display'],
        'expr': [rust_src / 'expr'],
        'geo_tree': [rust_src / 'geo_tree'],
        'util': [rust_src / 'util'],
    }
    
    for module_name, paths in rust_modules.items():
        total = 0
        for path in paths:
            total += count_rust_module(path)
        modules[module_name] = total
        print(f"{module_name}: {total:,} lines")
    
    # Flutter UI module
    ui_loc = count_flutter_ui()
    modules['ui'] = ui_loc
    print(f"ui: {ui_loc:,} lines")
    
    return modules

def main():
    """Main entry point."""
    print("Counting lines of code for atomCAD modules...")
    print("=" * 60)
    
    modules = count_all_modules()
    
    print("=" * 60)
    print(f"Total: {sum(modules.values()):,} lines")
    
    # Save to JSON
    output_file = Path(__file__).parent / 'loc_counts.json'
    with open(output_file, 'w') as f:
        json.dump(modules, f, indent=2)
    
    print(f"\nSaved to: {output_file}")

if __name__ == '__main__':
    main()
