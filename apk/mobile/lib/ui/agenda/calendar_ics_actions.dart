import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../../core/api/ics_files.dart';
import '../../core/state/calendar_controller.dart';
import 'calendar_ics_result.dart';

/// The host names every export `excubia.ics` (contract `api.md`), and the transport does
/// not hand the `Content-Disposition` over, so the name is fixed here.
const String _exportName = 'excubia.ics';

/// The calendar's `.ics` actions. The phone's dialogs come from [files]; a failure is
/// said in a snackbar, and a cancelled dialog is no failure.
class CalendarIcsActions {
  const CalendarIcsActions({
    required this.api,
    required this.files,
    required this.calendar,
  });

  final AgendaApi api;
  final IcsFiles files;
  final CalendarController calendar;

  Future<void> importFile(BuildContext context) async {
    try {
      final bytes = await files.pick();
      if (bytes == null || !context.mounted) return;
      final report = await api.importIcs(bytes);
      if (!context.mounted) return;
      await calendar.events.reload();
      if (context.mounted) await _showReport(context, report);
    } catch (error) {
      if (context.mounted) _fail(context, error, importing: true);
    }
  }

  Future<void> exportFile(
    BuildContext context,
    ({String from, String to})? window,
  ) async {
    try {
      final bytes = await api.exportIcs(window);
      if (!context.mounted) return;
      final saved = await files.save(_exportName, bytes);
      if (saved && context.mounted) _say(context, 'Archivo .ics guardado.');
    } catch (error) {
      if (context.mounted) _fail(context, error, importing: false);
    }
  }

  void _fail(BuildContext context, Object error, {required bool importing}) {
    if (icsZoneUnset(error)) calendar.loadZone();
    _say(context, icsFailureText(error, importing: importing));
  }

  static Future<void> _showReport(
    BuildContext context,
    IcsImportReport report,
  ) => showDialog<void>(
    context: context,
    builder: (_) => CalendarIcsResult(report: report),
  );

  static void _say(BuildContext context, String text) =>
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
}
