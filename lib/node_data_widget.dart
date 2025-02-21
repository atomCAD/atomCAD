import 'package:flutter/material.dart';
import 'package:flutter_cad/graph_model.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:provider/provider.dart';

/// A widget that displays and allows editing of node-specific data
/// based on the currently selected node in the graph.
class NodeDataWidget extends StatelessWidget {
  final GraphModel graphModel;

  const NodeDataWidget({
    super.key,
    required this.graphModel,
  });

  @override
  Widget build(BuildContext context) {
    // Listen to changes in the graph model
    return ChangeNotifierProvider.value(
      value: graphModel,
      child: Consumer<GraphModel>(
        builder: (context, model, child) {
          final nodeNetworkView = model.nodeNetworkView;
          if (nodeNetworkView == null) return const SizedBox.shrink();

          // Find the selected node
          final selectedNode = nodeNetworkView.nodes.entries
              .where((entry) => entry.value.selected)
              .map((entry) => entry.value)
              .firstOrNull;

          if (selectedNode == null) {
            return const Center(
              child: Text('No node selected'),
            );
          }

          // Based on the node type, show the appropriate editor
          switch (selectedNode.nodeTypeName) {
            case 'cuboid':
              return _CuboidEditor(
                nodeNetworkName: nodeNetworkView.name,
                nodeId: selectedNode.id,
              );
            case 'sphere':
              return _SphereEditor(
                nodeNetworkName: nodeNetworkView.name,
                nodeId: selectedNode.id,
              );
            case 'half_space':
              return _HalfSpaceEditor(
                nodeNetworkName: nodeNetworkView.name,
                nodeId: selectedNode.id,
              );
            default:
              return Center(
                child: Text(
                    'No editor available for ${selectedNode.nodeTypeName}'),
              );
          }
        },
      ),
    );
  }
}

/// Editor widget for Cuboid nodes
class _CuboidEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;

  const _CuboidEditor({
    required this.nodeNetworkName,
    required this.nodeId,
  });

  @override
  State<_CuboidEditor> createState() => _CuboidEditorState();
}

class _CuboidEditorState extends State<_CuboidEditor> {
  APICuboidData? _data;
  APICuboidData? _stagedData;

  @override
  void initState() {
    super.initState();
    _loadData();
  }

  Future<void> _loadData() async {
    final data = getCuboidData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
    );
    if (mounted) {
      setState(() {
        _data = data;
        _stagedData = data;
      });
    }
  }

  void _updateStagedData(APICuboidData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setCuboidData(
        nodeNetworkName: widget.nodeNetworkName,
        nodeId: widget.nodeId,
        data: _stagedData!,
      );
      setState(() => _data = _stagedData);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_stagedData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Cuboid Properties', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            _Vec3Input(
              label: 'Min Corner',
              value: _stagedData!.minCorner,
              onChanged: (newValue) {
                _updateStagedData(APICuboidData(
                  minCorner: newValue,
                  extent: _stagedData!.extent,
                ));
              },
            ),
            const SizedBox(height: 8),
            _Vec3Input(
              label: 'Extent',
              value: _stagedData!.extent,
              onChanged: (newValue) {
                _updateStagedData(APICuboidData(
                  minCorner: _stagedData!.minCorner,
                  extent: newValue,
                ));
              },
            ),
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _data == _stagedData ? null : () {
                    setState(() => _stagedData = _data);
                  },
                  child: const Text('Reset'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _data == _stagedData ? null : _applyChanges,
                  child: const Text('Apply'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// Editor widget for Sphere nodes
class _SphereEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;

  const _SphereEditor({
    required this.nodeNetworkName,
    required this.nodeId,
  });

  @override
  State<_SphereEditor> createState() => _SphereEditorState();
}

class _SphereEditorState extends State<_SphereEditor> {
  APISphereData? _data;
  APISphereData? _stagedData;

  @override
  void initState() {
    super.initState();
    _loadData();
  }

  Future<void> _loadData() async {
    final data = getSphereData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
    );
    if (mounted) {
      setState(() {
        _data = data;
        _stagedData = data;
      });
    }
  }

  void _updateStagedData(APISphereData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setSphereData(
        nodeNetworkName: widget.nodeNetworkName,
        nodeId: widget.nodeId,
        data: _stagedData!,
      );
      setState(() => _data = _stagedData);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_stagedData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Sphere Properties', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            _Vec3Input(
              label: 'Center',
              value: _stagedData!.center,
              onChanged: (newValue) {
                _updateStagedData(APISphereData(
                  center: newValue,
                  radius: _stagedData!.radius,
                ));
              },
            ),
            const SizedBox(height: 8),
            _IntInput(
              label: 'Radius',
              value: _stagedData!.radius,
              onChanged: (newValue) {
                _updateStagedData(APISphereData(
                  center: _stagedData!.center,
                  radius: newValue,
                ));
              },
            ),
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _data == _stagedData ? null : () {
                    setState(() => _stagedData = _data);
                  },
                  child: const Text('Reset'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _data == _stagedData ? null : _applyChanges,
                  child: const Text('Apply'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// Editor widget for HalfSpace nodes
class _HalfSpaceEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;

  const _HalfSpaceEditor({
    required this.nodeNetworkName,
    required this.nodeId,
  });

  @override
  State<_HalfSpaceEditor> createState() => _HalfSpaceEditorState();
}

class _HalfSpaceEditorState extends State<_HalfSpaceEditor> {
  APIHalfSpaceData? _data;
  APIHalfSpaceData? _stagedData;

  @override
  void initState() {
    super.initState();
    _loadData();
  }

  Future<void> _loadData() async {
    final data = getHalfSpaceData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
    );
    if (mounted) {
      setState(() {
        _data = data;
        _stagedData = data;
      });
    }
  }

  void _updateStagedData(APIHalfSpaceData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setHalfSpaceData(
        nodeNetworkName: widget.nodeNetworkName,
        nodeId: widget.nodeId,
        data: _stagedData!,
      );
      setState(() => _data = _stagedData);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_stagedData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Half Space Properties', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            _Vec3Input(
              label: 'Miller Index',
              value: _stagedData!.millerIndex,
              onChanged: (newValue) {
                _updateStagedData(APIHalfSpaceData(
                  millerIndex: newValue,
                  shift: _stagedData!.shift,
                ));
              },
            ),
            const SizedBox(height: 8),
            _IntInput(
              label: 'Shift',
              value: _stagedData!.shift,
              onChanged: (newValue) {
                _updateStagedData(APIHalfSpaceData(
                  millerIndex: _stagedData!.millerIndex,
                  shift: newValue,
                ));
              },
            ),
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _data == _stagedData ? null : () {
                    setState(() => _stagedData = _data);
                  },
                  child: const Text('Reset'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _data == _stagedData ? null : _applyChanges,
                  child: const Text('Apply'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// A reusable widget for editing Vec3 values
class _Vec3Input extends StatelessWidget {
  final String label;
  final APIIVec3 value;
  final ValueChanged<APIIVec3> onChanged;

  const _Vec3Input({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label),
        Row(
          children: [
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'X'),
                controller: TextEditingController(text: value.x.toString()),
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? value.x;
                  onChanged(APIIVec3(x: newValue, y: value.y, z: value.z));
                },
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'Y'),
                controller: TextEditingController(text: value.y.toString()),
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? value.y;
                  onChanged(APIIVec3(x: value.x, y: newValue, z: value.z));
                },
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'Z'),
                controller: TextEditingController(text: value.z.toString()),
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? value.z;
                  onChanged(APIIVec3(x: value.x, y: value.y, z: newValue));
                },
              ),
            ),
          ],
        ),
      ],
    );
  }
}

/// A reusable widget for editing integer values
class _IntInput extends StatelessWidget {
  final String label;
  final int value;
  final ValueChanged<int> onChanged;

  const _IntInput({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label),
        TextField(
          decoration: const InputDecoration(
            border: OutlineInputBorder(),
          ),
          controller: TextEditingController(text: value.toString()),
          keyboardType: TextInputType.number,
          onChanged: (text) {
            final newValue = int.tryParse(text);
            if (newValue != null) {
              onChanged(newValue);
            }
          },
        ),
      ],
    );
  }
}
