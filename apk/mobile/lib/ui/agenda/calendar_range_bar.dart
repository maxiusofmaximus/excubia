import 'package:flutter/material.dart';

import '../../core/state/calendar_controller.dart';
import '../../core/state/calendar_time.dart';
import '../theme.dart';
import 'calendar_format.dart';

/// The range of days on screen, the days before and after it, and the zone they are
/// read in.
class CalendarRangeBar extends StatelessWidget {
  const CalendarRangeBar({
    super.key,
    required this.ready,
    required this.onRange,
    required this.onShift,
  });

  final ZoneReady ready;
  final void Function(CalendarRange range) onRange;
  final void Function(int steps) onShift;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(Spacing.md),
    child: Column(
      children: [
        SegmentedButton<CalendarRange>(
          segments: [
            for (final range in CalendarRange.values)
              ButtonSegment(value: range, label: Text(range.label)),
          ],
          selected: {ready.range},
          showSelectedIcon: false,
          onSelectionChanged: (picked) => onRange(picked.single),
        ),
        Row(
          children: [
            IconButton(
              tooltip: 'Anterior',
              icon: const Icon(Icons.chevron_left),
              onPressed: () => onShift(-1),
            ),
            Expanded(
              child: Text(
                rangeLabel(ready.anchor, ready.range),
                textAlign: TextAlign.center,
                style: context.text.body,
              ),
            ),
            IconButton(
              tooltip: 'Siguiente',
              icon: const Icon(Icons.chevron_right),
              onPressed: () => onShift(1),
            ),
          ],
        ),
        Text('Horas de ${ready.name}', style: context.text.small),
      ],
    ),
  );
}
