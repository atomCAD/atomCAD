# Comment Node - Technical Design Document

*Technical implementation plan for comment nodes in atomCAD's node network editor.*

**Related:** [comment_node_ux.md](comment_node_ux.md) — UX specification

## Architecture Decision

**Comment nodes use the existing `Node` struct** with a specialized `CommentData` implementation of the `NodeData` trait. This provides:

- Transparent serialization, selection, move, delete, copy/paste
- No parallel data structures or duplicated logic
- Minimal evaluator overhead (returns `NetworkResult::None`)
- Natural integration with existing UI rendering pipeline

## Implementation Plan

### Phase 1: Rust Backend

#### 1.1 Add `Annotation` Category

**File:** `rust/src/api/structure_designer/structure_designer_api_types.rs`

```rust
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum NodeTypeCategory {
    Annotation,           // NEW - appears first
    MathAndProgramming,
    Geometry2D,
    Geometry3D,
    AtomicStructure,
    OtherBuiltin,
    Custom,
}

impl NodeTypeCategory {
    pub fn order(&self) -> u8 {
        match self {
            Self::Annotation => 0,         // First in list
            Self::MathAndProgramming => 1,
            Self::Geometry2D => 2,
            Self::Geometry3D => 3,
            Self::AtomicStructure => 4,
            Self::OtherBuiltin => 5,
            Self::Custom => 6,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Annotation => "Annotation",
            // ... existing cases
        }
    }
}
```

**File:** `rust/src/structure_designer/serialization/node_networks_serialization.rs`

Update `category_to_string()` and `category_from_string()` to handle `"Annotation"`.

#### 1.2 Create CommentData

**File:** `rust/src/structure_designer/nodes/comment.rs` (new)

```rust
use serde::{Serialize, Deserialize};
use std::collections::HashSet;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::{NetworkEvaluator, NetworkStackElement, NetworkEvaluationContext};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::data_type::DataType;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentData {
    pub label: String,      // Optional title shown in header
    pub text: String,       // Main comment content
    pub width: f64,         // Box width in logical units
    pub height: f64,        // Box height in logical units
}

impl Default for CommentData {
    fn default() -> Self {
        Self {
            label: String::new(),
            text: String::new(),
            width: 200.0,
            height: 100.0,
        }
    }
}

impl NodeData for CommentData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        Some(Box::new(CommentGadget {
            label: self.label.clone(),
            text: self.text.clone(),
            width: self.width,
            height: self.height,
        }))
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &Vec<NetworkStackElement<'a>>,
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
        // Comments don't produce values - they're purely visual
        NetworkResult::None
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}

// Gadget for property panel editing
pub struct CommentGadget {
    pub label: String,
    pub text: String,
    pub width: f64,
    pub height: f64,
}

impl NodeNetworkGadget for CommentGadget {
    fn sync_data(&self, node_data: &mut dyn std::any::Any) {
        if let Some(comment_data) = node_data.downcast_mut::<CommentData>() {
            comment_data.label = self.label.clone();
            comment_data.text = self.text.clone();
            comment_data.width = self.width;
            comment_data.height = self.height;
        }
    }
}

// Node type factory function
pub fn get_node_type() -> NodeType {
    NodeType {
        name: "Comment".to_string(),
        description: "Add text annotations to document your node network.".to_string(),
        category: NodeTypeCategory::Annotation,
        parameters: vec![],  // No input pins
        output_type: DataType::None,  // No output pin
        node_data_creator: || Box::new(CommentData::default()),
        node_data_saver: comment_data_saver,
        node_data_loader: comment_data_loader,
        public: true,
    }
}

fn comment_data_saver(data: &mut dyn std::any::Any, _design_dir: Option<&str>) -> std::io::Result<serde_json::Value> {
    let comment_data = data.downcast_ref::<CommentData>()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Expected CommentData"))?;
    serde_json::to_value(comment_data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn comment_data_loader(json: &serde_json::Value, _design_dir: Option<&str>) -> std::io::Result<Box<dyn NodeData>> {
    let comment_data: CommentData = serde_json::from_value(json.clone())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Box::new(comment_data))
}
```

#### 1.3 Add DataType::None

**File:** `rust/src/structure_designer/data_type.rs`

```rust
pub enum DataType {
    None,      // NEW - for nodes with no output
    Bool,
    Int,
    Float,
    // ... existing types
}

impl DataType {
    pub fn to_string(&self) -> String {
        match self {
            Self::None => "None".to_string(),
            // ... existing cases
        }
    }

    pub fn from_string(s: &str) -> Result<Self, String> {
        match s {
            "None" => Ok(Self::None),
            // ... existing cases
        }
    }
}
```

#### 1.4 Register Comment Node Type

**File:** `rust/src/structure_designer/nodes/mod.rs`

```rust
pub mod comment;  // Add this line
```

**File:** `rust/src/structure_designer/node_type_registry.rs`

```rust
use super::nodes::comment::get_node_type as comment_get_node_type;

impl NodeTypeRegistry {
    pub fn new() -> Self {
        let mut ret = Self { ... };
        
        // Add at the beginning (annotation nodes)
        ret.add_node_type(comment_get_node_type());
        
        // ... existing node types
    }
}
```

#### 1.5 Update Category Ordering

**File:** `rust/src/structure_designer/node_type_registry.rs`

In `get_node_type_views()` and `get_compatible_node_types()`, update `ordered_categories`:

```rust
let ordered_categories = vec![
    NodeTypeCategory::Annotation,  // NEW - first
    NodeTypeCategory::MathAndProgramming,
    NodeTypeCategory::Geometry2D,
    NodeTypeCategory::Geometry3D,
    NodeTypeCategory::AtomicStructure,
    NodeTypeCategory::OtherBuiltin,
    NodeTypeCategory::Custom,
];
```

### Phase 2: API Layer

#### 2.1 Add Comment-Specific View Data

**File:** `rust/src/api/structure_designer/structure_designer_api_types.rs`

```rust
/// Additional data for comment nodes sent to Flutter
pub struct APICommentNodeData {
    pub label: String,
    pub text: String,
    pub width: f64,
    pub height: f64,
}
```

#### 2.2 Extend NodeView

**File:** `rust/src/api/structure_designer/structure_designer_api_types.rs`

Add optional comment data to `NodeView` (or use a discriminated union approach):

```rust
pub struct NodeView {
    // ... existing fields
    pub comment_data: Option<APICommentNodeData>,  // Only set for Comment nodes
}
```

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

When building `NodeView`, check if the node is a Comment and populate `comment_data`:

```rust
let comment_data = if node.node_type_name == "Comment" {
    node.data.as_any_ref().downcast_ref::<CommentData>().map(|cd| APICommentNodeData {
        label: cd.label.clone(),
        text: cd.text.clone(),
        width: cd.width,
        height: cd.height,
    })
} else {
    None
};
```

### Phase 3: Flutter Frontend

#### 3.1 Update Add Node Dialog Categories

**File:** `lib/structure_designer/node_network/add_node_popup.dart`

```dart
String getCategoryDisplayName(NodeTypeCategory category) {
  switch (category) {
    case NodeTypeCategory.annotation:
      return 'Annotation';  // NEW
    case NodeTypeCategory.mathAndProgramming:
      return 'Math and Programming';
    // ... existing cases
  }
}
```

#### 3.2 Create Comment Node Widget

**File:** `lib/structure_designer/node_network/comment_node_widget.dart` (new)

```dart
import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

class CommentNodeWidget extends StatefulWidget {
  final NodeView node;
  final bool isSelected;
  final double scale;
  final VoidCallback? onResizeStart;
  final Function(double dx, double dy)? onResize;
  final VoidCallback? onResizeEnd;

  const CommentNodeWidget({
    super.key,
    required this.node,
    required this.isSelected,
    required this.scale,
    this.onResizeStart,
    this.onResize,
    this.onResizeEnd,
  });

  @override
  State<CommentNodeWidget> createState() => _CommentNodeWidgetState();
}

class _CommentNodeWidgetState extends State<CommentNodeWidget> {
  static const double _minWidth = 100.0;
  static const double _minHeight = 60.0;
  static const double _handleSize = 12.0;

  @override
  Widget build(BuildContext context) {
    final commentData = widget.node.commentData;
    if (commentData == null) return const SizedBox.shrink();

    final width = commentData.width * widget.scale;
    final height = commentData.height * widget.scale;
    
    // Non-linear font scaling: zoom^0.5 for readability
    final fontScale = math.sqrt(widget.scale);
    final baseFontSize = 13.0;
    final fontSize = baseFontSize * fontScale;

    return SizedBox(
      width: width,
      height: height,
      child: Stack(
        children: [
          // Comment box
          Container(
            decoration: BoxDecoration(
              color: const Color(0xFFFFF9C4).withOpacity(0.85), // Pale yellow
              border: Border.all(
                color: widget.isSelected 
                    ? const Color(0xFFD84315)  // Selection color
                    : const Color(0xFF9E9E9E),
                width: widget.isSelected ? 2.0 : 1.0,
                style: BorderStyle.solid,
              ),
              borderRadius: BorderRadius.circular(4),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Header with label
                if (commentData.label.isNotEmpty)
                  Container(
                    width: double.infinity,
                    padding: EdgeInsets.symmetric(
                      horizontal: 8.0 * widget.scale,
                      vertical: 4.0 * widget.scale,
                    ),
                    decoration: BoxDecoration(
                      color: const Color(0xFFFFE082).withOpacity(0.5),
                      borderRadius: const BorderRadius.only(
                        topLeft: Radius.circular(3),
                        topRight: Radius.circular(3),
                      ),
                    ),
                    child: Text(
                      commentData.label,
                      style: TextStyle(
                        fontSize: fontSize,
                        fontWeight: FontWeight.w600,
                        color: const Color(0xFF5D4037),
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                // Text content with scrolling
                Expanded(
                  child: SingleChildScrollView(
                    padding: EdgeInsets.all(8.0 * widget.scale),
                    child: Text(
                      commentData.text,
                      style: TextStyle(
                        fontSize: fontSize,
                        color: const Color(0xFF424242),
                        height: 1.4,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
          // Resize handle (bottom-right corner)
          if (widget.isSelected)
            Positioned(
              right: 0,
              bottom: 0,
              child: GestureDetector(
                onPanStart: (_) => widget.onResizeStart?.call(),
                onPanUpdate: (details) => widget.onResize?.call(
                  details.delta.dx / widget.scale,
                  details.delta.dy / widget.scale,
                ),
                onPanEnd: (_) => widget.onResizeEnd?.call(),
                child: MouseRegion(
                  cursor: SystemMouseCursors.resizeDownRight,
                  child: Container(
                    width: _handleSize,
                    height: _handleSize,
                    decoration: BoxDecoration(
                      color: const Color(0xFFD84315),
                      borderRadius: BorderRadius.circular(2),
                    ),
                    child: const Icon(
                      Icons.open_in_full,
                      size: 8,
                      color: Colors.white,
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
```

#### 3.3 Integrate into Node Network Widget

**File:** `lib/structure_designer/node_network/node_network.dart`

In the build method where nodes are rendered, add a check:

```dart
for (final node in nodes) {
  if (node.nodeTypeName == 'Comment') {
    // Render comment widget
    children.add(Positioned(
      left: (node.position.x + panOffset.dx) * scale,
      top: (node.position.y + panOffset.dy) * scale,
      child: CommentNodeWidget(
        node: node,
        isSelected: selectedNodeIds.contains(node.id),
        scale: scale,
        onResizeStart: () => _startCommentResize(node.id),
        onResize: (dx, dy) => _resizeComment(node.id, dx, dy),
        onResizeEnd: () => _endCommentResize(node.id),
      ),
    ));
  } else {
    // Render normal node widget
    children.add(Positioned(...));
  }
}
```

#### 3.4 Add Comment Property Editor

**File:** `lib/structure_designer/node_data/comment_editor.dart` (new)

```dart
import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart' as sd_api;

class CommentEditor extends StatefulWidget {
  final int nodeId;
  final String initialLabel;
  final String initialText;
  
  const CommentEditor({
    super.key,
    required this.nodeId,
    required this.initialLabel,
    required this.initialText,
  });
  
  @override
  State<CommentEditor> createState() => _CommentEditorState();
}

class _CommentEditorState extends State<CommentEditor> {
  late TextEditingController _labelController;
  late TextEditingController _textController;
  
  @override
  void initState() {
    super.initState();
    _labelController = TextEditingController(text: widget.initialLabel);
    _textController = TextEditingController(text: widget.initialText);
  }
  
  void _updateComment() {
    sd_api.updateCommentNode(
      nodeId: widget.nodeId,
      label: _labelController.text,
      text: _textController.text,
    );
  }
  
  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text('Label', style: TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 4),
        TextField(
          controller: _labelController,
          decoration: const InputDecoration(
            hintText: 'Optional title...',
            isDense: true,
          ),
          onChanged: (_) => _updateComment(),
        ),
        const SizedBox(height: 16),
        const Text('Text', style: TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 4),
        TextField(
          controller: _textController,
          decoration: const InputDecoration(
            hintText: 'Enter comment text...',
          ),
          maxLines: 8,
          onChanged: (_) => _updateComment(),
        ),
      ],
    );
  }
  
  @override
  void dispose() {
    _labelController.dispose();
    _textController.dispose();
    super.dispose();
  }
}
```

#### 3.5 Add Resize API

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

```rust
pub fn resize_comment_node(node_id: u64, width: f64, height: f64) {
    with_structure_designer_mut(|sd| {
        if let Some(network) = sd.registry.node_networks.get_mut(&sd.current_network_name) {
            if let Some(node) = network.nodes.get_mut(&node_id) {
                if let Some(comment_data) = node.data.as_any_mut().downcast_mut::<CommentData>() {
                    comment_data.width = width.max(100.0);  // Enforce minimum
                    comment_data.height = height.max(60.0);
                }
            }
        }
    });
}

pub fn update_comment_node(node_id: u64, label: String, text: String) {
    with_structure_designer_mut(|sd| {
        if let Some(network) = sd.registry.node_networks.get_mut(&sd.current_network_name) {
            if let Some(node) = network.nodes.get_mut(&node_id) {
                if let Some(comment_data) = node.data.as_any_mut().downcast_mut::<CommentData>() {
                    comment_data.label = label;
                    comment_data.text = text;
                }
            }
        }
    });
}
```

### Phase 4: Evaluator Integration

#### 4.1 Skip Comment Nodes in Display

Comment nodes should not appear in the 3D viewport. The evaluator already handles this naturally:

- Comments have no output pin → cannot be wired to other nodes
- `NetworkResult::None` means no geometry/atomic data to display
- The display system only shows nodes in `displayed_node_ids` with valid geometry

No changes needed — the existing architecture handles this correctly.

### Phase 5: Testing

#### 5.1 Rust Tests

**File:** `rust/tests/structure_designer/comment_node_test.rs` (new)

```rust
use flutter_cad::structure_designer::nodes::comment::{CommentData, get_node_type};
use flutter_cad::structure_designer::node_data::NodeData;

#[test]
fn test_comment_data_default() {
    let data = CommentData::default();
    assert_eq!(data.label, "");
    assert_eq!(data.text, "");
    assert_eq!(data.width, 200.0);
    assert_eq!(data.height, 100.0);
}

#[test]
fn test_comment_node_type() {
    let node_type = get_node_type();
    assert_eq!(node_type.name, "Comment");
    assert!(node_type.parameters.is_empty());
    assert!(node_type.public);
}

#[test]
fn test_comment_serialization_roundtrip() {
    let original = CommentData {
        label: "Test Label".to_string(),
        text: "Test content".to_string(),
        width: 250.0,
        height: 150.0,
    };
    
    let json = serde_json::to_value(&original).unwrap();
    let restored: CommentData = serde_json::from_value(json).unwrap();
    
    assert_eq!(restored.label, original.label);
    assert_eq!(restored.text, original.text);
    assert_eq!(restored.width, original.width);
    assert_eq!(restored.height, original.height);
}
```

### Implementation Order

1. **Rust backend** (Phase 1) — ~2 hours
   - Add `Annotation` category
   - Create `CommentData` and register node type
   - Add `DataType::None`

2. **API layer** (Phase 2) — ~1 hour
   - Extend `NodeView` with comment data
   - Add resize/update API functions

3. **Regenerate bindings** — ~5 minutes
   ```powershell
   flutter_rust_bridge_codegen generate
   ```

4. **Flutter frontend** (Phase 3) — ~3 hours
   - Update category display
   - Create `CommentNodeWidget`
   - Integrate into node network rendering
   - Create property editor

5. **Testing** (Phase 5) — ~1 hour
   - Rust unit tests
   - Manual integration testing

**Total estimated time:** ~7 hours

---

*Document created: Comment Node Technical Design*
*Status: Ready for implementation*
