import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/state.dart';

import 'package:herdr_notify/core/state/reconciler.dart';

StateSnapshot stateAt(int revision) => StateSnapshot(
  revision: revision,
  counts: const <String, int>{},
  agents: const <AgentEntry>[],
);

/// "The last revision wins": the case this pins is a re-read racing a newer
/// frame, where applying the older answer would visibly undo the screen.
void main() {
  late StateReconciler reconciler;

  setUp(() => reconciler = StateReconciler());

  test('an older state is discarded and the newer one kept', () {
    expect(reconciler.acceptState(stateAt(5)), isTrue);
    expect(reconciler.acceptState(stateAt(4)), isFalse);
    expect(reconciler.revision, 5);
    expect(reconciler.lastGood!.revision, 5);
  });

  test(
    'equal revisions are allowed: re-reading the same state is harmless',
    () {
      expect(reconciler.acceptState(stateAt(5)), isTrue);
      expect(reconciler.acceptState(stateAt(5)), isTrue);
      expect(reconciler.revision, 5);
    },
  );

  test('a newer state wins', () {
    expect(reconciler.acceptState(stateAt(5)), isTrue);
    expect(reconciler.acceptState(stateAt(6)), isTrue);
    expect(reconciler.revision, 6);
  });

  test('a frame at or below the current revision is not worth a refresh', () {
    expect(reconciler.shouldRefreshForFrame(3), isTrue);
    reconciler.acceptState(stateAt(5));
    expect(reconciler.shouldRefreshForFrame(5), isFalse);
    expect(reconciler.shouldRefreshForFrame(4), isFalse);
    expect(reconciler.shouldRefreshForFrame(6), isTrue);
  });

  test('revision zero is a baseline, not a reason to skip the first frame', () {
    reconciler.acceptState(stateAt(0));
    expect(reconciler.shouldRefreshForFrame(0), isTrue);
    expect(reconciler.shouldRefreshForFrame(1), isTrue);
  });

  test('a frame without a revision always refreshes', () {
    reconciler.acceptState(stateAt(7));
    expect(reconciler.shouldRefreshForFrame(null), isTrue);
  });
}
