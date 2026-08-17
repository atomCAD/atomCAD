"""
Generate architecture diagram SVG for atomCAD.

Creates an SVG visualization showing crates as circles (sized by LOC)
and dependencies as arrows.

Every Rust circle except `api` is a crate under `rust/crates/`, named
`atomcad-<label>`; `api` is the root cdylib package `rust_lib_flutter_cad`.
The arrows are therefore not a convention any more — they are the
`[dependencies]` sections of the workspace manifests, and the compiler rejects
an edge that is not drawn here (`doc/design_rust_crate_split.md`).
"""

import json
import math
from pathlib import Path
from typing import Dict, List, Tuple

# Circle sizing parameters
#
# `area = LOC * LOC_SCALE`, so LOC_SCALE trades legibility against overlap: the
# largest circle's diameter must stay under LAYER_SPACING or neighbouring
# layers collide. Re-tune it if the counts grow a lot (at 0.7 the largest,
# `ui`, comes out at r ~= 130 against a 270 px spacing).
LOC_SCALE = 0.7   # Scale factor for area calculation (area = LOC * scale)
MIN_RADIUS = 40   # Minimum circle radius in pixels

# Layout parameters (horizontal layout)
CANVAS_WIDTH = 1700
CANVAS_HEIGHT = 720
LAYER_SPACING = 270  # Horizontal spacing between layers
FIRST_LAYER_X = 210  # Centre of the leftmost layer

# Colors (modern, professional palette)
COLORS = {
    'util': '#94A3B8',        # Gray - foundation
    'renderer': '#10B981',     # Green - graphics
    'geo_tree': '#3B82F6',     # Blue - geometry
    'crystolecule': '#8B5CF6', # Purple - atoms
    'display': '#F59E0B',      # Orange - visualization
    'structure_designer': '#EF4444',  # Red - application
    'api': '#7C3AED',          # Violet - FFI boundary (root crate)
    'ui': '#EC4899',           # Pink - user interface
}

# Crate dependencies (from -> to). These mirror the `[dependencies]` sections
# of the workspace manifests one for one — keep them in sync.
DEPENDENCIES = [
    # Bottom layer dependencies
    ('renderer', 'util'),
    ('geo_tree', 'util'),
    ('crystolecule', 'util'),
    ('crystolecule', 'geo_tree'),

    # Middle layer dependencies
    ('display', 'renderer'),
    ('display', 'crystolecule'),
    ('display', 'geo_tree'),
    ('display', 'util'),

    # Top layer dependencies
    ('structure_designer', 'display'),
    ('structure_designer', 'crystolecule'),
    ('structure_designer', 'geo_tree'),
    ('structure_designer', 'renderer'),
    ('structure_designer', 'util'),

    # The root crate (rust_lib_flutter_cad): the FFI boundary
    ('api', 'structure_designer'),
    ('api', 'display'),
    ('api', 'crystolecule'),
    ('api', 'geo_tree'),
    ('api', 'renderer'),
    ('api', 'util'),

    # UI dependencies
    ('ui', 'api'),
]

# Layer definitions (horizontal positioning, left to right)
# Dependencies point rightward: if A depends on B, B is to the right of A
LAYERS = {
    0: ['ui'],
    1: ['api'],
    2: ['structure_designer'],
    3: ['display'],
    4: ['renderer', 'geo_tree', 'crystolecule'],
    5: ['util'],
}

# Arrows that would only add clutter: the ones every layer draws to the two
# widely-used bottom crates. Skipping them is cosmetic, and the subtitle says
# so — do not read the rendered SVG as the whole dependency list.
ELIDED_ARROWS = {
    ('display', 'util'),
    ('structure_designer', 'util'),
    ('structure_designer', 'renderer'),
    ('api', 'display'),
    ('api', 'crystolecule'),
    ('api', 'geo_tree'),
    ('api', 'renderer'),
    ('api', 'util'),
}

def calculate_radius(loc: int) -> float:
    """
    Calculate circle radius from LOC count.
    Area is proportional to LOC: area = LOC * LOC_SCALE
    Then radius = sqrt(area / π)
    """
    area = loc * LOC_SCALE
    radius = math.sqrt(area / math.pi)
    return max(MIN_RADIUS, radius)

def calculate_positions(modules: Dict[str, int]) -> Dict[str, Tuple[float, float, float]]:
    """
    Calculate (x, y, radius) positions for all modules.
    Horizontal layout: layers go from left to right.
    
    Returns: Dict[module_name, (x, y, radius)]
    """
    positions = {}
    
    for layer_idx, module_names in LAYERS.items():
        # Calculate radii for this layer
        layer_modules = [(name, calculate_radius(modules.get(name, 0))) 
                        for name in module_names]
        
        # Calculate total height needed for this layer (stacked vertically)
        total_height = sum(r * 2 for _, r in layer_modules)
        spacing = 60 if len(layer_modules) > 1 else 0
        total_height += spacing * (len(layer_modules) - 1)
        
        # Calculate X position for this layer (left to right)
        x = FIRST_LAYER_X + layer_idx * LAYER_SPACING
        
        # Calculate Y positions (centered vertically)
        start_y = (CANVAS_HEIGHT - total_height) / 2
        current_y = start_y
        
        for name, radius in layer_modules:
            y = current_y + radius
            positions[name] = (x, y, radius)
            current_y += radius * 2 + spacing
    
    return positions

def create_svg_header() -> str:
    """Create SVG header with styles."""
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{CANVAS_WIDTH}" height="{CANVAS_HEIGHT}" viewBox="0 0 {CANVAS_WIDTH} {CANVAS_HEIGHT}">
  <defs>
    <style>
      .module-circle {{
        stroke: white;
        stroke-width: 3;
        filter: drop-shadow(0 4px 6px rgba(0, 0, 0, 0.1));
      }}
      .module-label {{
        font-family: Arial, sans-serif;
        font-size: 16px;
        font-weight: bold;
        fill: white;
        text-anchor: middle;
        dominant-baseline: middle;
      }}
      .module-loc {{
        font-family: Arial, sans-serif;
        font-size: 18px;
        font-weight: bold;
        fill: white;
        text-anchor: middle;
        dominant-baseline: middle;
        opacity: 0.95;
      }}
      .dependency-arrow {{
        fill: none;
        stroke: #64748B;
        stroke-width: 2;
        opacity: 0.4;
        marker-end: url(#arrowhead);
      }}
      .title {{
        font-family: Arial, sans-serif;
        font-size: 24px;
        font-weight: bold;
        fill: #1E293B;
        text-anchor: middle;
      }}
      .subtitle {{
        font-family: Arial, sans-serif;
        font-size: 14px;
        fill: #64748B;
        text-anchor: middle;
      }}
    </style>
    <marker id="arrowhead" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto">
      <polygon points="0 0, 10 3, 0 6" fill="#64748B" opacity="0.6"/>
    </marker>
  </defs>
  
  <rect width="{CANVAS_WIDTH}" height="{CANVAS_HEIGHT}" fill="#F8FAFC"/>
  
  <text x="{CANVAS_WIDTH/2}" y="40" class="title">atomCAD Architecture</text>
  <text x="{CANVAS_WIDTH/2}" y="65" class="subtitle">Every Rust circle is a cargo crate: atomcad-&lt;name&gt;, except api = the root package rust_lib_flutter_cad. The DAG is compiler-enforced.</text>
  <text x="{CANVAS_WIDTH/2}" y="86" class="subtitle">Circle area proportional to lines of code in src/ • Arrows point to dependencies (some are cut for clarity)</text>

  <g id="arrows">
'''

def create_arrow(from_pos: Tuple[float, float, float], 
                to_pos: Tuple[float, float, float]) -> str:
    """Create an SVG path for an arrow between two circles."""
    x1, y1, r1 = from_pos
    x2, y2, r2 = to_pos
    
    # Calculate angle between centers
    dx = x2 - x1
    dy = y2 - y1
    angle = math.atan2(dy, dx)
    
    # Start point (edge of from circle)
    start_x = x1 + r1 * math.cos(angle)
    start_y = y1 + r1 * math.sin(angle)
    
    # End point (edge of to circle)
    end_x = x2 - r2 * math.cos(angle)
    end_y = y2 - r2 * math.sin(angle)
    
    # Calculate control point for curved arrow
    mid_x = (start_x + end_x) / 2
    mid_y = (start_y + end_y) / 2
    
    # Offset control point perpendicular to the line
    offset = 30
    perp_angle = angle + math.pi / 2
    ctrl_x = mid_x + offset * math.cos(perp_angle)
    ctrl_y = mid_y + offset * math.sin(perp_angle)
    
    return f'    <path class="dependency-arrow" d="M {start_x},{start_y} Q {ctrl_x},{ctrl_y} {end_x},{end_y}"/>\n'

def create_module_circle(name: str, pos: Tuple[float, float, float], loc: int) -> str:
    """Create SVG elements for a module circle with label."""
    x, y, r = pos
    color = COLORS.get(name, '#64748B')
    
    # Format LOC with thousands separator
    loc_number = f"{loc:,}"
    
    # Calculate font sizes based on radius (larger for presentation visibility)
    label_size = min(20, max(16, r / 3))
    loc_size = min(18, max(14, r / 4))
    lines_size = max(12, loc_size * 0.7)  # "lines" text is smaller
    
    return f'''  <g id="module-{name}">
    <circle class="module-circle" cx="{x}" cy="{y}" r="{r}" fill="{color}"/>
    <text class="module-label" x="{x}" y="{y - 10}" style="font-size: {label_size}px">{name}</text>
    <text class="module-loc" x="{x}" y="{y + 15}">
      <tspan style="font-size: {loc_size}px; font-weight: bold">{loc_number}</tspan>
      <tspan style="font-size: {lines_size}px; font-weight: normal; opacity: 0.85"> lines</tspan>
    </text>
  </g>
'''

def create_svg_footer() -> str:
    """Create SVG footer."""
    return '''  </g>
</svg>'''

def generate_diagram(modules: Dict[str, int]) -> str:
    """Generate complete SVG diagram."""
    positions = calculate_positions(modules)
    
    svg = create_svg_header()
    
    # Add arrows first (so they appear behind circles)
    for from_module, to_module in DEPENDENCIES:
        if (from_module, to_module) in ELIDED_ARROWS:
            continue
        if from_module in positions and to_module in positions:
            svg += create_arrow(positions[from_module], positions[to_module])
    
    svg += '  </g>\n  <g id="modules">\n'
    
    # Add module circles
    for module_name, (x, y, r) in positions.items():
        loc = modules.get(module_name, 0)
        svg += create_module_circle(module_name, (x, y, r), loc)
    
    svg += create_svg_footer()
    
    return svg

def main():
    """Main entry point."""
    # Load LOC counts
    loc_file = Path(__file__).parent / 'loc_counts.json'
    
    if not loc_file.exists():
        print(f"Error: {loc_file} not found. Run count_loc.py first.")
        return
    
    with open(loc_file, 'r') as f:
        modules = json.load(f)
    
    print("Generating architecture diagram...")
    print(f"Modules: {', '.join(modules.keys())}")
    
    # Generate SVG
    svg_content = generate_diagram(modules)
    
    # Save to doc folder
    output_file = Path(__file__).parent.parent.parent / 'doc' / 'architecture_diagram.svg'
    output_file.parent.mkdir(exist_ok=True)
    
    with open(output_file, 'w', encoding='utf-8') as f:
        f.write(svg_content)
    
    print(f"Generated: {output_file}")

if __name__ == '__main__':
    main()
