import 'package:flutter/material.dart';
import 'package:timezone/timezone.dart' as tz;

import '../../core/api/agenda_api.dart';
import '../../core/api/ics_files.dart';
import '../../core/state/calendar_controller.dart';
import '../../core/state/calendar_time.dart';
import '../theme.dart';
import 'agenda_widgets.dart';
import 'calendar_event_screen.dart';
import 'calendar_ics_actions.dart';
import 'calendar_ics_menu.dart';
import 'calendar_list.dart';
import 'calendar_range_bar.dart';
import 'calendar_zone_view.dart';

/// «Calendario»: the company's events, by day, in the company's own zone. The zone is
/// asked the first time; without it no event can be created.
class CalendarScreen extends StatefulWidget {
  const CalendarScreen({
    super.key,
    required this.api,
    this.clock = DateTime.now,
    this.files = const SystemIcsFiles(),
  });

  final AgendaApi api;

  /// The phone's clock. A test fixes «hoy» with it.
  final DateTime Function() clock;

  /// The phone's `.ics` dialogs. A test replaces them.
  final IcsFiles files;

  @override
  State<CalendarScreen> createState() => _CalendarScreenState();
}

class _CalendarScreenState extends State<CalendarScreen> {
  late final CalendarController _calendar = CalendarController(
    widget.api,
    clock: widget.clock,
  );
  late final CalendarIcsActions _ics = CalendarIcsActions(
    api: widget.api,
    files: widget.files,
    calendar: _calendar,
  );

  @override
  void initState() {
    super.initState();
    _calendar.loadZone();
  }

  @override
  void dispose() {
    _calendar.dispose();
    super.dispose();
  }

  /// A new event, or the editor of [event], in [zone]. The list is read again on return.
  /// An occurrence opens its whole series: the editor writes the series, so it starts
  /// from the series' own start, not from the occurrence's.
  Future<void> _open(tz.Location zone, [CalendarEvent? event]) async {
    var shown = event;
    if (event != null && event.recurring) {
      shown = await _readSeries(event.id);
      if (shown == null || !mounted) return;
    }
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => CalendarEventScreen(
          api: widget.api,
          zone: zone,
          now: wallClock(widget.clock(), zone),
          event: shown,
        ),
      ),
    );
    if (mounted) await _calendar.events.reload();
  }

  /// The series of an occurrence. `null` when it cannot be read: the reason is shown
  /// and no editor opens on one occurrence alone.
  Future<CalendarEvent?> _readSeries(String id) async {
    try {
      return await widget.api.calendarEvent(id);
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(agendaFailureText(error))),
        );
      }
      return null;
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: _calendar,
    builder: (context, _) {
      final zone = _calendar.zone;
      return Scaffold(
        backgroundColor: context.colors.bg,
        appBar: AppBar(
          title: const Text('Calendario'),
          actions: [
            CalendarIcsMenu(
              window: _calendar.window,
              onImport: () => _ics.importFile(context),
              onExport: (window) => _ics.exportFile(context, window),
            ),
          ],
        ),
        floatingActionButton: FloatingActionButton.extended(
          onPressed: zone is ZoneReady ? () => _open(zone.location) : null,
          icon: const Icon(Icons.event_outlined),
          label: const Text('Nuevo evento'),
        ),
        body: _body(zone),
      );
    },
  );

  Widget _body(CalendarZone zone) => switch (zone) {
    ZoneLoading() => const Center(child: CircularProgressIndicator()),
    ZoneUnset unset => CalendarZoneView(
      unset: unset,
      onSave: _calendar.saveZone,
    ),
    ZoneUnknown(:final name) => AgendaMessage(
      text:
          'La zona «$name» no está en los datos de este teléfono, '
          'así que no se pueden mostrar sus eventos.',
    ),
    ZoneFailed(:final failure) => AgendaMessage(
      text: failure,
      actionLabel: 'Reintentar',
      onAction: _calendar.loadZone,
    ),
    ZoneReady ready => _ready(ready),
  };

  Widget _ready(ZoneReady ready) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      CalendarRangeBar(
        ready: ready,
        onRange: _calendar.setRange,
        onShift: _calendar.shift,
      ),
      Expanded(
        child: CalendarList(
          list: _calendar.events,
          zone: ready.location,
          today: wallDay(widget.clock(), ready.location),
          onOpen: (event) => _open(ready.location, event),
        ),
      ),
    ],
  );
}
