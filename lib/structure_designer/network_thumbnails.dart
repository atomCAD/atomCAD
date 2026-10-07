import 'package:flutter/foundation.dart' show Uint8List, visibleForTesting;
import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Side of the stored thumbnail in pixels, and of the hover preview
/// (`doc/design_network_thumbnails.md` D5, D9).
const double NETWORK_THUMBNAIL_PREVIEW_SIZE = 128.0;

/// Decoded network thumbnails, keyed by **revision alone**
/// (`doc/design_network_thumbnails.md` D9).
///
/// Revisions come from one process-wide counter in Rust that never repeats, so
/// two different images never share a key: a rename keeps its revision (and its
/// cached image), while a capture, an undo restore or File > Open brings a new
/// one and therefore a fetch. Bytes are fetched by network name in the active
/// document only on a miss.
class NetworkThumbnailCache {
  /// Fetches a network's PNG bytes; the Rust API unless a test injects one.
  final Uint8List? Function(String networkName) _fetch;

  NetworkThumbnailCache({Uint8List? Function(String networkName)? fetch})
      : _fetch = fetch ??
            ((name) => structure_designer_api.getNetworkThumbnailPng(
                networkName: name));

  final Map<BigInt, MemoryImage> _images = {};

  /// Number of cached images (for tests).
  @visibleForTesting
  int get length => _images.length;

  /// The image for [network], or null when it has no thumbnail.
  MemoryImage? imageFor(APINetworkWithValidationErrors network) {
    if (!network.hasThumbnail) return null;
    final cached = _images[network.thumbnailRevision];
    if (cached != null) return cached;
    final bytes = _fetch(network.name);
    if (bytes == null) return null;
    final image = MemoryImage(bytes);
    _images[network.thumbnailRevision] = image;
    return image;
  }

  /// Drops every entry whose revision is no longer in [networks] (the active
  /// document's list), so replaced images and another tab's images do not
  /// accumulate. Called whenever the network list is refreshed.
  void prune(List<APINetworkWithValidationErrors> networks) {
    if (_images.isEmpty) return;
    final live = <BigInt>{
      for (final n in networks)
        if (n.hasThumbnail) n.thumbnailRevision,
    };
    _images.removeWhere((revision, image) {
      if (live.contains(revision)) return false;
      image.evict();
      return true;
    });
  }
}

/// A network's thumbnail at [size]×[size] with rounded corners, or [fallback]
/// when it has none. Hovering the image for a moment shows it at full
/// 128×128 (D9), with a caption when it is [pinned].
///
/// A pinned thumbnail is marked **only** in the hover preview, never on the
/// small image itself: the row picture is the user's design and stays free of
/// badges.
class NetworkThumbnail extends StatelessWidget {
  final MemoryImage? image;
  final double size;
  final Widget fallback;
  final bool pinned;

  const NetworkThumbnail({
    super.key,
    required this.image,
    required this.size,
    required this.fallback,
    this.pinned = false,
  });

  @override
  Widget build(BuildContext context) {
    final image = this.image;
    if (image == null) return fallback;
    return Tooltip(
      waitDuration: const Duration(milliseconds: 400),
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(
        color: Colors.grey[800],
        borderRadius: BorderRadius.circular(4),
        boxShadow: const [
          BoxShadow(color: Colors.black26, blurRadius: 4, offset: Offset(0, 2)),
        ],
      ),
      richMessage: WidgetSpan(
        child: pinned
            ? Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  NetworkThumbnailPreview(image: image),
                  const Padding(
                    padding: EdgeInsets.fromLTRB(2, 4, 2, 2),
                    child: SizedBox(
                      width: NETWORK_THUMBNAIL_PREVIEW_SIZE - 4,
                      child: Text(
                        "Pinned — won't update automatically",
                        style: TextStyle(color: Colors.white, fontSize: 11),
                      ),
                    ),
                  ),
                ],
              )
            : NetworkThumbnailPreview(image: image),
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(size >= 32 ? 4 : 3),
        child: Image(
          image: image,
          width: size,
          height: size,
          fit: BoxFit.cover,
          filterQuality: FilterQuality.medium,
          gaplessPlayback: true,
          // A thumbnail that fails to decode is no worse than none.
          errorBuilder: (context, error, stackTrace) => fallback,
        ),
      ),
    );
  }
}

/// A thumbnail at full 128×128 with rounded corners: the body of every hover
/// preview — list and tree rows, and custom nodes on the canvas (D9).
class NetworkThumbnailPreview extends StatelessWidget {
  final MemoryImage image;

  const NetworkThumbnailPreview({super.key, required this.image});

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(3),
      child: Image(
        image: image,
        width: NETWORK_THUMBNAIL_PREVIEW_SIZE,
        height: NETWORK_THUMBNAIL_PREVIEW_SIZE,
        gaplessPlayback: true,
        errorBuilder: (context, error, stackTrace) => const SizedBox.shrink(),
      ),
    );
  }
}

/// Shared size of the list view's thumbnail and its fallback icon slot.
const double NETWORK_LIST_THUMBNAIL_SIZE = 40.0;

/// Shared size of the tree view's thumbnail.
const double NETWORK_TREE_THUMBNAIL_SIZE = 24.0;
