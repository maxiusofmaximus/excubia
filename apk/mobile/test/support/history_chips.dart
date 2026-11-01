import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// The horizontal row of history chips in the drawer.
Finder historyChipRow() => find.byWidgetPredicate(
  (widget) => widget is ListView && widget.scrollDirection == Axis.horizontal,
);

/// The row builds only the chips near its viewport, so a chip further along is
/// revealed by scrolling the row until it exists, then brought into view.
Future<void> revealHistoryChip(WidgetTester tester, String label) async {
  await tester.scrollUntilVisible(
    find.text(label),
    60,
    scrollable: find.descendant(
      of: historyChipRow(),
      matching: find.byType(Scrollable),
    ),
  );
  await tester.ensureVisible(find.text(label));
  await tester.pumpAndSettle();
}
