import 'package:flutter_cad/structure_designer/node_network/splice_drop_memo.dart';
import 'package:flutter_test/flutter_test.dart';

/// A stand-in for a hit-test result: a new object per tick, equal wires by
/// value only through the key.
class _Hit {
  _Hit(this.source, this.dest);
  final int source;
  final int dest;
}

void main() {
  late List<String> asked;
  late PerDragPlanMemo<_Hit> memo;

  setUp(() {
    asked = [];
    memo = PerDragPlanMemo<_Hit>(
      (hit) {
        asked.add('${hit.source}->${hit.dest}');
        return hit.dest.isEven;
      },
      (hit) => '${hit.source}->${hit.dest}',
    );
  });

  test('asks once per wire across many hover ticks', () {
    for (var tick = 0; tick < 50; tick++) {
      expect(memo.accepts(_Hit(1, 2)), isTrue);
    }
    for (var tick = 0; tick < 50; tick++) {
      expect(memo.accepts(_Hit(1, 3)), isFalse);
      expect(memo.accepts(_Hit(1, 2)), isTrue);
    }
    expect(asked, ['1->2', '1->3']);
    expect(memo.plannedWireCount, 2);
  });

  test('remembers a refusal as firmly as an acceptance', () {
    memo.accepts(_Hit(4, 5));
    memo.accepts(_Hit(4, 5));
    expect(asked, ['4->5']);
  });

  test('forgets everything when the drag ends', () {
    memo.accepts(_Hit(1, 2));
    memo.accepts(_Hit(1, 3));
    memo.endDrag();
    expect(memo.plannedWireCount, 0);
    memo.accepts(_Hit(1, 2));
    expect(asked, ['1->2', '1->3', '1->2']);
  });
}
