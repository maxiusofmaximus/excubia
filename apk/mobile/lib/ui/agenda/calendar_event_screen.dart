import 'package:flutter/material.dart';
import 'package:timezone/timezone.dart' as tz;

import '../../core/api/agenda_api.dart';
import '../../core/state/calendar_time.dart';
import '../theme.dart';
import 'agenda_widgets.dart';
import 'calendar_draft.dart';
import 'calendar_format.dart';
import 'calendar_pickers.dart';
import 'calendar_recurrence.dart';
import 'calendar_repeat_field.dart';

/// One event: a new one, or the editor of [event]. The days and clock times are the
/// company's, never the phone's. Deleting needs `APPROVE`.
class CalendarEventScreen extends StatefulWidget {
  const CalendarEventScreen({
    super.key,
    required this.api,
    required this.zone,
    required this.now,
    this.event,
  });

  final AgendaApi api;
  final tz.Location zone;

  /// The company's clock now; a new event starts at the next full hour.
  final DateTime now;
  final CalendarEvent? event;

  @override
  State<CalendarEventScreen> createState() => _CalendarEventScreenState();
}

class _CalendarEventScreenState extends State<CalendarEventScreen> {
  late final CalendarDraft _first = _initialDraft();
  late final TextEditingController _title = TextEditingController(
    text: _first.title,
  );
  late final TextEditingController _location = TextEditingController(
    text: _first.location,
  );
  late final TextEditingController _notes = TextEditingController(
    text: _first.notes,
  );
  late bool _allDay = _first.allDay;
  late DateTime _start = _first.start;
  late DateTime? _end = _first.end;
  late int? _reminder = _first.reminderMinutes;
  late Repeat _repeat = _first.repeat;
  bool _busy = false;
  String? _failure;

  CalendarDraft _initialDraft() {
    final event = widget.event;
    return event == null
        ? CalendarDraft.fresh(start: nextFullHour(widget.now))
        : CalendarDraft.of(event, widget.zone);
  }

  @override
  void dispose() {
    _title.dispose();
    _location.dispose();
    _notes.dispose();
    super.dispose();
  }

  CalendarDraft _draft() => CalendarDraft(
    title: _title.text.trim(),
    allDay: _allDay,
    start: _start,
    end: _end,
    location: _location.text.trim(),
    notes: _notes.text.trim(),
    reminderMinutes: _reminder,
    repeat: _repeat,
  );

  /// All-day keeps the days only; a timed event that would end before it starts gets
  /// an hour, so the editor never opens on an invalid pair.
  void _setAllDay(bool value) => setState(() {
    _allDay = value;
    final end = _end;
    if (value) {
      _end = end ?? _start;
    } else if (end == null || !end.isAfter(_start)) {
      _end = addHours(_start, 1);
    }
  });

  /// A timed event keeps its end after its start; the end moves with it when it must.
  void _setStart(DateTime value) => setState(() {
    _start = value;
    final end = _end;
    if (!_allDay && end != null && !end.isAfter(value)) {
      _end = addHours(value, 1);
    }
  });

  void _setEnd(DateTime? value) => setState(() => _end = value);

  Future<void> _pickStartDay() async {
    final day = await pickCalendarDay(context, _start);
    if (day == null) return;
    _setStart(DateTime.utc(day.year, day.month, day.day, _start.hour, _start.minute));
  }

  Future<void> _pickStartClock() async {
    final clock = await pickCalendarClock(context, _start);
    if (clock != null) _setStart(clock);
  }

  Future<void> _pickEndDay() async {
    final base = _end ?? _start;
    final day = await pickCalendarDay(context, base);
    if (day == null) return;
    _setEnd(DateTime.utc(day.year, day.month, day.day, base.hour, base.minute));
  }

  Future<void> _pickEndClock() async {
    final clock = await pickCalendarClock(context, _end ?? _start);
    if (clock != null) _setEnd(clock);
  }

  Future<void> _save() async {
    final draft = _draft();
    final problem = draft.problem();
    if (problem != null) {
      setState(() => _failure = problem);
      return;
    }
    final input = draft.toInput();
    final event = widget.event;
    await _run(() async {
      if (event == null) {
        await widget.api.createEvent(input);
      } else {
        await widget.api.updateEvent(event.id, input);
      }
    });
  }

  Future<void> _delete() async {
    final event = widget.event;
    if (event == null) return;
    final sure = await confirmDelete(
      context,
      title: '¿Borrar este evento?',
      body: event.recurrence == null
          ? 'Se borra ya el evento y su aviso. No se puede deshacer.'
          : 'Se borran todas las repeticiones. No se puede deshacer.',
    );
    if (!sure || !mounted) return;
    await _run(() => widget.api.deleteEvent(event.id));
  }

  /// Runs one write. Success closes the editor; a failure stays on screen.
  Future<void> _run(Future<void> Function() action) async {
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      await action();
      if (mounted) Navigator.of(context).pop();
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _failure = agendaFailureText(error);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final event = widget.event;
    final failure = _failure;
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: Text(event == null ? 'Nuevo evento' : 'Evento')),
      body: ListView(
        padding: const EdgeInsets.all(Spacing.lg),
        children: [
          TextField(
            controller: _title,
            enabled: !_busy,
            maxLength: calendarTitleMax,
            style: context.text.body,
            decoration: const InputDecoration(labelText: 'Título'),
          ),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Todo el día'),
            value: _allDay,
            onChanged: _busy ? null : _setAllDay,
          ),
          _startRow(context),
          _endRow(context),
          const SizedBox(height: Spacing.md),
          CalendarRepeatField(
            repeat: _repeat,
            startDay: dayOnly(_start),
            enabled: !_busy,
            onChanged: (repeat) => setState(() => _repeat = repeat),
          ),
          if (_repeat is! NoRepeat) ...[
            const SizedBox(height: Spacing.xs),
            Text(
              'Al editar o borrar se cambia toda la serie.',
              style: context.text.small,
            ),
          ],
          const SizedBox(height: Spacing.md),
          _reminderField(),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: _location,
            enabled: !_busy,
            maxLength: calendarLocationMax,
            style: context.text.body,
            decoration: const InputDecoration(labelText: 'Lugar'),
          ),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: _notes,
            enabled: !_busy,
            minLines: 3,
            maxLines: 3,
            style: context.text.body,
            decoration: const InputDecoration(labelText: 'Notas'),
          ),
          if (failure != null) ...[
            const SizedBox(height: Spacing.md),
            Text(failure, style: context.text.bodySecondary),
          ],
          const SizedBox(height: Spacing.lg),
          FilledButton(
            onPressed: _busy ? null : _save,
            child: const Text('Guardar'),
          ),
          if (event != null) ...[
            const SizedBox(height: Spacing.md),
            ApproveGrant(
              api: widget.api,
              builder: (context, canApprove) => Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  OutlinedButton(
                    onPressed: _busy || canApprove != true ? null : _delete,
                    child: const Text('Borrar evento'),
                  ),
                  if (canApprove == false) ...[
                    const SizedBox(height: Spacing.sm),
                    Text(approveMissingText, style: context.text.small),
                  ],
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }

  Widget _startRow(BuildContext context) => _whenRow(context, 'Inicio', [
    TextButton(onPressed: _pickStartDay, child: Text(dayLabel(_start))),
    if (!_allDay)
      TextButton(onPressed: _pickStartClock, child: Text(clockText(_start))),
  ]);

  Widget _endRow(BuildContext context) {
    final end = _end;
    if (!_allDay && end == null) {
      return _whenRow(context, 'Fin', [
        TextButton(
          onPressed: () => _setEnd(addHours(_start, 1)),
          child: const Text('Añadir fin'),
        ),
      ]);
    }
    final shown = end ?? _start;
    return _whenRow(context, _allDay ? 'Último día' : 'Fin', [
      TextButton(onPressed: _pickEndDay, child: Text(dayLabel(shown))),
      if (!_allDay)
        TextButton(onPressed: _pickEndClock, child: Text(clockText(shown))),
      if (!_allDay)
        IconButton(
          tooltip: 'Quitar el fin',
          icon: const Icon(Icons.close),
          onPressed: () => _setEnd(null),
        ),
    ]);
  }

  Widget _whenRow(BuildContext context, String label, List<Widget> buttons) =>
      Row(
        children: [
          SizedBox(
            width: 80,
            child: Text(label, style: context.text.bodySecondary),
          ),
          Expanded(
            child: Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              children: buttons,
            ),
          ),
        ],
      );

  Widget _reminderField() => DropdownMenu<int?>(
    initialSelection: _reminder,
    label: const Text('Aviso'),
    expandedInsets: EdgeInsets.zero,
    dropdownMenuEntries: [
      for (final minutes in reminderChoices(_reminder))
        DropdownMenuEntry(
          value: minutes,
          label: minutes == null ? 'Ninguno' : reminderLabel(minutes),
        ),
    ],
    onSelected: (value) => setState(() => _reminder = value),
  );
}
