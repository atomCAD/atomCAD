import 'dart:typed_data';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/network_thumbnails.dart';
import 'package:flutter_test/flutter_test.dart';

APINetworkWithValidationErrors _network(String name,
        {bool hasThumbnail = true, int revision = 1}) =>
    APINetworkWithValidationErrors(
      name: name,
      validationErrors: const [],
      readOnly: false,
      hasThumbnail: hasThumbnail,
      thumbnailRevision: BigInt.from(revision),
      thumbnailUserSet: false,
    );

void main() {
  // prune() evicts from the painting binding's image cache.
  TestWidgetsFlutterBinding.ensureInitialized();

  late List<String> fetched;
  late NetworkThumbnailCache cache;

  setUp(() {
    fetched = [];
    cache = NetworkThumbnailCache(fetch: (name) {
      fetched.add(name);
      return Uint8List.fromList([fetched.length]);
    });
  });

  test('a network without a thumbnail fetches nothing', () {
    expect(cache.imageFor(_network('a', hasThumbnail: false)), isNull);
    expect(fetched, isEmpty);
  });

  test('a cached revision is not fetched again', () {
    final first = cache.imageFor(_network('a', revision: 5));
    final second = cache.imageFor(_network('a', revision: 5));
    expect(first, isNotNull);
    expect(identical(first, second), isTrue);
    expect(fetched, ['a']);
  });

  test('a rename keeps the cached image: the key is the revision alone', () {
    final before = cache.imageFor(_network('a', revision: 5));
    final after = cache.imageFor(_network('renamed', revision: 5));
    expect(identical(before, after), isTrue);
    expect(fetched, ['a']);
  });

  test('a new revision fetches again', () {
    cache.imageFor(_network('a', revision: 5));
    cache.imageFor(_network('a', revision: 6));
    expect(fetched, ['a', 'a']);
  });

  test('missing bytes are not cached', () {
    final empty = NetworkThumbnailCache(fetch: (_) => null);
    expect(empty.imageFor(_network('a')), isNull);
    expect(empty.length, 0);
  });

  test('prune drops revisions that left the list', () {
    cache.imageFor(_network('a', revision: 1));
    cache.imageFor(_network('b', revision: 2));
    cache.prune([_network('a', revision: 1), _network('b', revision: 3)]);
    expect(cache.length, 1);
    cache.imageFor(_network('a', revision: 1));
    expect(fetched, ['a', 'b']);
  });
}
