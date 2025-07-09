import 'dart:math';
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:vector_math/vector_math_64.dart' as vm;

/// A widget that displays a map of possible Miller indices using a sinusoidal projection.
/// This provides a more intuitive way to select Miller indices compared to manually entering values.
class MillerIndexMap extends StatefulWidget {
  final String label;
  final APIIVec3 value;
  final ValueChanged<APIIVec3> onChanged;

  // Size constraints for the widget
  final double mapWidth;
  final double mapHeight;

  // Miller index bound
  final int maxValue;

  // Visual properties
  final double dotSize;
  final Color dotColor;
  final Color selectedDotColor;

  const MillerIndexMap({
    Key? key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.mapWidth = 300,
    this.mapHeight = 150,
    this.maxValue = 5,
    this.dotSize = 2.0,
    this.dotColor = Colors.grey,
    this.selectedDotColor = Colors.blue,
  }) : super(key: key);

  /// Converts a Miller index to a latitude/longitude pair.
  /// This is done by normalizing the vector and converting to spherical coordinates.
  /// In this system, Y is the up direction (for latitude).
  /// Returns a Vector2 where x = longitude and y = latitude.
  static vm.Vector2 millerIndexToLatLong(APIIVec3 miller) {
    // Handle the zero vector case
    if (miller.x == 0 && miller.y == 0 && miller.z == 0) {
      return vm.Vector2(0.0, 0.0); // longitude, latitude
    }

    // Convert to double for calculations
    double x = miller.x.toDouble();
    double y = miller.y.toDouble();
    double z = miller.z.toDouble();

    // Calculate magnitude
    double magnitude = sqrt(x * x + y * y + z * z);

    // Normalize
    x /= magnitude;
    y /= magnitude;
    z /= magnitude;

    // Convert to spherical coordinates
    // Y is the up direction in our coordinate system
    double latitude = asin(y);
    // For longitude, we use x and z in a right-handed system
    double longitude = -atan2(z, x);

    return vm.Vector2(longitude, latitude);
  }

  /// Apply sinusoidal projection to convert latitude/longitude to x,y coordinates
  /// Input vector: x = longitude, y = latitude
  /// Output vector: x, y in projected coordinates
  static vm.Vector2 sinusoidalProjection(vm.Vector2 latLong) {
    // Use 0 as central meridian (λ0)
    // latLong.x = longitude, latLong.y = latitude
    return vm.Vector2(latLong.x * cos(latLong.y), latLong.y);
  }

  /// Checks if two Miller indices represent the same direction
  /// Note: Opposing directions (-x,-y,-z) vs (x,y,z) are considered different
  /// because they define different half-spaces for cutting operations
  static bool isSameDirection(APIIVec3 a, APIIVec3 b) {
    if (a.x == 0 && a.y == 0 && a.z == 0) {
      return b.x == 0 && b.y == 0 && b.z == 0;
    }

    if (b.x == 0 && b.y == 0 && b.z == 0) {
      return false;
    }

    // Check if one is a multiple of the other
    int gcdA = _findGcd(a);
    int gcdB = _findGcd(b);

    // Reduce both to their simplest form
    APIIVec3 reducedA = APIIVec3(
      x: (a.x ~/ gcdA),
      y: (a.y ~/ gcdA),
      z: (a.z ~/ gcdA),
    );

    APIIVec3 reducedB = APIIVec3(
      x: (b.x ~/ gcdB),
      y: (b.y ~/ gcdB),
      z: (b.z ~/ gcdB),
    );

    // Compare reduced forms - note that we preserve sign direction
    // since opposite normals define different half-spaces
    return reducedA.x == reducedB.x &&
        reducedA.y == reducedB.y &&
        reducedA.z == reducedB.z;
  }

  /// Find the greatest common divisor of the absolute values of the components
  static int _findGcd(APIIVec3 vector) {
    int gcd = _gcd(_gcd(vector.x.abs(), vector.y.abs()), vector.z.abs());
    return gcd == 0 ? 1 : gcd; // Handle zero vector case
  }

  /// Euclidean algorithm for finding GCD of two numbers
  static int _gcd(int a, int b) {
    while (b != 0) {
      int temp = b;
      b = a % b;
      a = temp;
    }
    return a;
  }

  /// Generate a list of Miller indices (includes opposite directions as distinct entries)
  List<APIIVec3> _generateUniqueMilerIndices() {
    List<APIIVec3> allIndices = [];

    // Generate all valid Miller indices within bounds
    for (int x = -maxValue; x <= maxValue; x++) {
      for (int y = -maxValue; y <= maxValue; y++) {
        for (int z = -maxValue; z <= maxValue; z++) {
          // Skip the zero vector
          if (x == 0 && y == 0 && z == 0) continue;

          APIIVec3 miller = APIIVec3(x: x, y: y, z: z);

          // Get the reduced form of this Miller index
          APIIVec3 reducedMiller = _getReducedMillerIndex(miller);

          // Check if this direction is already in the list
          bool isDuplicate = false;
          for (var existing in allIndices) {
            if (isSameDirection(reducedMiller, existing)) {
              isDuplicate = true;
              break;
            }
          }

          if (!isDuplicate) {
            // Store the reduced form in the list
            allIndices.add(reducedMiller);
          }
        }
      }
    }

    return allIndices;
  }

  /// Find the reduced form of a Miller index (lowest possible integers)
  /// Preserves the original direction (sign) of the vector
  APIIVec3 _getReducedMillerIndex(APIIVec3 miller) {
    int gcd = _findGcd(miller);

    // Handle zero vector
    if (gcd == 0) return miller;

    // Reduce to smallest possible integers while preserving direction
    return APIIVec3(
      x: miller.x ~/ gcd,
      y: miller.y ~/ gcd,
      z: miller.z ~/ gcd,
    );
  }

  @override
  State<MillerIndexMap> createState() => _MillerIndexMapState();
}

class _DotInfo {
  final APIIVec3 miller;
  final Offset position;

  _DotInfo(this.miller, this.position);
}

class _MillerIndexMapState extends State<MillerIndexMap> {
  // List of unique Miller indices and their positions
  late List<APIIVec3> _uniqueIndices;
  final Map<APIIVec3, Offset> _dotPositions = {};
  final GlobalKey _mapKey = GlobalKey();
  Offset? _hoverPosition;
  APIIVec3? _hoveredIndex;
  OverlayEntry? _tooltipOverlay;

  @override
  void initState() {
    super.initState();
    _uniqueIndices = _generateUniqueMilerIndices();
  }

  @override
  void didUpdateWidget(MillerIndexMap oldWidget) {
    super.didUpdateWidget(oldWidget);
    // If maxValue changes, we need to regenerate the Miller indices
    if (widget.maxValue != oldWidget.maxValue) {
      setState(() {
        _uniqueIndices = _generateUniqueMilerIndices();
        _dotPositions.clear(); // Clear cached positions to force recalculation
        _hoveredIndex = null; // Clear hover state
        _removeTooltip(); // Remove any tooltips
      });
    }
  }

  @override
  void dispose() {
    // Clean up tooltip overlay if it exists
    _removeTooltip();
    super.dispose();
  }

  List<APIIVec3> _generateUniqueMilerIndices() {
    return widget._generateUniqueMilerIndices();
  }

  // Calculate positions of all dots based on the container size
  void _calculateDotPositions(Size size) {
    _dotPositions.clear();

    for (final miller in _uniqueIndices) {
      // Convert miller index to lat/long
      var latLong = MillerIndexMap.millerIndexToLatLong(miller);

      // Apply sinusoidal projection
      var projection = MillerIndexMap.sinusoidalProjection(latLong);

      // Map from projection coordinates to canvas coordinates
      double x = (projection.x + pi) / (2 * pi) * size.width;
      double y = size.height - ((projection.y + pi / 2) / pi * size.height);

      _dotPositions[miller] = Offset(x, y);
    }
  }

  // Handle mouse hover event
  void _handleHover(PointerHoverEvent event) {
    final localPosition = event.localPosition;

    // Find which dot (if any) is under the cursor
    APIIVec3? foundIndex;
    Offset? foundPosition;
    double closestDistance = double.infinity;
    final hoverRadius = widget.dotSize * 2.0; // Detection radius

    _dotPositions.forEach((miller, position) {
      final distance = (position - localPosition).distance;
      if (distance <= hoverRadius && distance < closestDistance) {
        closestDistance = distance;
        foundIndex = miller;
        foundPosition = position; // Store the actual dot position
      }
    });

    // Only rebuild if the hover state changed
    if (_hoveredIndex != foundIndex) {
      setState(() {
        // Use the dot's actual position (if found) instead of mouse position
        _hoverPosition = foundPosition; // Will be null if no dot was found
        _hoveredIndex = foundIndex;
      });

      // Update tooltip
      _removeTooltip();
      if (foundIndex != null) {
        // We know foundIndex is not null here, so we can safely pass it
        _showTooltip(foundIndex);
      }
    }
  }

  // Show tooltip for the given Miller index
  void _showTooltip(APIIVec3? miller) {
    if (miller == null) return;
    final overlay = Overlay.of(context);
    if (overlay == null) return;

    // Get the position for the tooltip
    final position = _dotPositions[miller];
    if (position == null) return;

    // Convert the local position to global position
    final RenderBox? renderBox =
        _mapKey.currentContext?.findRenderObject() as RenderBox?;
    if (renderBox == null) return;

    final globalPosition = renderBox.localToGlobal(position);

    // Create the overlay entry
    _tooltipOverlay = OverlayEntry(
      builder: (context) => Positioned(
        left: globalPosition.dx,
        top: globalPosition.dy - 35, // Position above the dot
        child: Material(
          elevation: 4.0,
          borderRadius: BorderRadius.circular(4.0),
          color: Colors.black87,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            child: Text(
              '(${miller.x}, ${miller.y}, ${miller.z})',
              style: const TextStyle(color: Colors.white, fontSize: 12),
            ),
          ),
        ),
      ),
    );

    // Add the tooltip to the overlay
    overlay.insert(_tooltipOverlay!);
  }

  // Remove the tooltip if it exists
  void _removeTooltip() {
    _tooltipOverlay?.remove();
    _tooltipOverlay = null;
  }

  // Handle tap on the map to select a Miller index
  void _handleTap(TapUpDetails details) {
    final localPosition = details.localPosition;

    // Find which dot (if any) was tapped
    APIIVec3? tappedIndex;
    double closestDistance = double.infinity;
    final tapRadius =
        widget.dotSize * 2.5; // Slightly larger than hover for easier selection

    _dotPositions.forEach((miller, position) {
      final distance = (position - localPosition).distance;
      if (distance <= tapRadius && distance < closestDistance) {
        closestDistance = distance;
        tappedIndex = miller;
      }
    });

    // If a dot was tapped, call onChanged with the selected index
    if (tappedIndex != null) {
      // Call onChanged with the selected Miller index
      widget.onChanged(tappedIndex!);

      // Update hover state and tooltip
      setState(() {
        _hoveredIndex = tappedIndex;
        _hoverPosition = _dotPositions[tappedIndex];
      });

      // Briefly show tooltip for visual feedback of selection
      _removeTooltip();
      _showTooltip(tappedIndex);
    }
  }

  @override
  Widget build(BuildContext context) {
    // Calculate dot positions when the widget is built
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final RenderBox? box =
          _mapKey.currentContext?.findRenderObject() as RenderBox?;
      if (box != null) {
        _calculateDotPositions(box.size);
        setState(() {}); // Trigger a rebuild with the calculated positions
      }
    });

    // Reduce the current value to its simplest form
    APIIVec3 reducedValue = widget._getReducedMillerIndex(widget.value);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.label, style: AppTextStyles.label),
        const SizedBox(height: 8),
        Container(
          key: _mapKey,
          width: widget.mapWidth,
          height: widget.mapHeight,
          decoration: BoxDecoration(
            color: Colors.white,
            border: Border.all(color: Colors.grey.shade300),
            borderRadius: BorderRadius.circular(4),
          ),
          child: GestureDetector(
            onTapUp: _handleTap,
            child: MouseRegion(
              onHover: _handleHover,
              onExit: (_) {
                setState(() {
                  _hoverPosition = null;
                  _hoveredIndex = null;
                });
                _removeTooltip();
              },
              child: CustomPaint(
                painter: _MillerIndexMapPainter(
                  uniqueIndices: _uniqueIndices,
                  currentValue: reducedValue,
                  dotSize: widget.dotSize,
                  dotColor: widget.dotColor,
                  selectedDotColor: widget.selectedDotColor,
                  dotPositions: _dotPositions,
                  hoverPosition: _hoverPosition,
                  hoveredIndex: _hoveredIndex,
                ),
                size: Size(widget.mapWidth, widget.mapHeight),
              ),
            ),
          ),
        ),
        const SizedBox(height: 4),
      ],
    );
  }
}

/// Custom painter for drawing the Miller index map with sinusoidal projection
class _MillerIndexMapPainter extends CustomPainter {
  final List<APIIVec3> uniqueIndices;
  final APIIVec3 currentValue;
  final double dotSize;
  final Color dotColor;
  final Color selectedDotColor;
  final Map<APIIVec3, Offset> dotPositions;
  final Offset? hoverPosition;
  final APIIVec3? hoveredIndex;

  _MillerIndexMapPainter({
    required this.uniqueIndices,
    required this.currentValue,
    required this.dotSize,
    required this.dotColor,
    required this.selectedDotColor,
    required this.dotPositions,
    this.hoverPosition,
    this.hoveredIndex,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final Paint dotPaint = Paint()
      ..color = dotColor
      ..style = PaintingStyle.fill;

    final Paint selectedDotPaint = Paint()
      ..color = selectedDotColor
      ..style = PaintingStyle.fill;
    final Paint gridPaint = Paint()
      ..color = Colors.grey.withOpacity(0.3)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 0.5;

    // Define standard paints for dots (removing duplicates)

    // Draw the background
    canvas.drawRect(Rect.fromLTWH(0, 0, size.width, size.height),
        Paint()..color = Colors.white);

    // Draw grid lines (latitude/longitude)
    _drawGrid(canvas, size, gridPaint);

    // The range of sinusoidal projection is:
    // x: [-π, π]
    // y: [-π/2, π/2]

    // We no longer need a separate hover position indicator since
    // we highlight the dot itself when hovered

    for (var miller in uniqueIndices) {
      // Skip if position hasn't been calculated yet
      if (!dotPositions.containsKey(miller)) continue;

      // Get pre-calculated position
      final position = dotPositions[miller]!;
      final x = position.dx;
      final y = position.dy;

      // Check if this is the current value or hovered index
      bool isCurrentValue =
          MillerIndexMap.isSameDirection(miller, currentValue);
      bool isHovered = hoveredIndex != null &&
          MillerIndexMap.isSameDirection(miller, hoveredIndex!);

      // Draw the dot
      final Paint dotPaint = Paint()
        ..color = isCurrentValue
            ? selectedDotColor
            : (isHovered ? Colors.amber : dotColor)
        ..style = PaintingStyle.fill;

      // Make hovered or selected dots slightly larger
      final double radius = dotSize * (isHovered || isCurrentValue ? 2.0 : 1.0);

      canvas.drawCircle(
        Offset(x, y),
        radius,
        dotPaint,
      );

      // Draw outline for the selected or hovered dot for better visibility
      if (isCurrentValue || isHovered) {
        final Paint outlinePaint = Paint()
          ..color = Colors.amber.withOpacity(0.6)
          ..style = PaintingStyle.stroke
          ..strokeWidth = isCurrentValue ? 1.5 : 1.0;

        canvas.drawCircle(
          Offset(x, y),
          radius + 3,
          outlinePaint,
        );
      }
    }
  }

  /// Draw grid lines for latitude and longitude
  void _drawGrid(Canvas canvas, Size size, Paint paint) {
    // Draw latitude lines
    int latSteps = 6; // Number of latitude lines
    double latStepSize = size.height / latSteps;

    for (int i = 0; i <= latSteps; i++) {
      double y = i * latStepSize;
      canvas.drawLine(
        Offset(0, y),
        Offset(size.width, y),
        paint,
      );
    }

    // Draw longitude lines - these are curved in sinusoidal projection
    int longSteps = 12; // Number of longitude lines

    for (int i = 0; i <= longSteps; i++) {
      // Convert to longitude in radians
      double lon = -pi + (i * (2 * pi / longSteps));

      Path path = Path();
      bool first = true;

      // Draw the curved line for this longitude
      for (int j = 0; j <= 100; j++) {
        // Convert to latitude in radians
        double lat = -pi / 2 + (j * (pi / 100));

        // Apply sinusoidal projection
        var projection =
            MillerIndexMap.sinusoidalProjection(vm.Vector2(lon, lat));

        // Map to canvas coordinates
        double x = (projection.x + pi) / (2 * pi) * size.width;
        double y = size.height - ((projection.y + pi / 2) / pi * size.height);

        if (first) {
          path.moveTo(x, y);
          first = false;
        } else {
          path.lineTo(x, y);
        }
      }

      canvas.drawPath(path, paint);
    }
  }

  @override
  bool shouldRepaint(_MillerIndexMapPainter oldDelegate) {
    return oldDelegate.currentValue != currentValue ||
        oldDelegate.uniqueIndices != uniqueIndices ||
        oldDelegate.dotSize != dotSize ||
        oldDelegate.dotColor != dotColor ||
        oldDelegate.selectedDotColor != selectedDotColor ||
        oldDelegate.hoverPosition != hoverPosition ||
        oldDelegate.hoveredIndex != hoveredIndex ||
        oldDelegate.dotPositions != dotPositions;
  }
}
