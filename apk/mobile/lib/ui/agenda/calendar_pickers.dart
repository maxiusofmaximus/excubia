import 'package:flutter/material.dart';

/// The day picked on Flutter's own calendar, as the company's day (midnight, UTC).
/// The labels are Spanish; the month names stay as Flutter has them.
Future<DateTime?> pickCalendarDay(BuildContext context, DateTime day) async {
  final picked = await showDatePicker(
    context: context,
    initialDate: day,
    firstDate: DateTime(2000),
    lastDate: DateTime(2100),
    helpText: 'Elige el día',
    cancelText: 'Cancelar',
    confirmText: 'Aceptar',
  );
  return picked == null
      ? null
      : DateTime.utc(picked.year, picked.month, picked.day);
}

/// The time picked on Flutter's clock, on the day of [clock], as the company's clock
/// reads it. The clock is 24-hour, as Spanish users expect.
Future<DateTime?> pickCalendarClock(BuildContext context, DateTime clock) async {
  final picked = await showTimePicker(
    context: context,
    initialTime: TimeOfDay(hour: clock.hour, minute: clock.minute),
    helpText: 'Elige la hora',
    cancelText: 'Cancelar',
    confirmText: 'Aceptar',
    builder: (context, child) => MediaQuery(
      data: MediaQuery.of(context).copyWith(alwaysUse24HourFormat: true),
      child: child ?? const SizedBox.shrink(),
    ),
  );
  return picked == null
      ? null
      : DateTime.utc(clock.year, clock.month, clock.day, picked.hour, picked.minute);
}
