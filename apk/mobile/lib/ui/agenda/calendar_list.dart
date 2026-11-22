import 'package:flutter/material.dart';
import 'package:timezone/timezone.dart' as tz;

import '../../core/api/agenda_api.dart';
import '../../core/state/agenda_list_controller.dart';
import '../theme.dart';
import 'agenda_widgets.dart';
import 'calendar_format.dart';

/// The events on screen, by day, read in the company's zone. «Cargar más» pages in
/// the next ones, as the other agenda lists do.
class CalendarList extends StatelessWidget {
  const CalendarList({
    super.key,
    required this.list,
    required this.zone,
    required this.today,
    required this.onOpen,
  });

  final AgendaListController<CalendarEvent> list;
  final tz.Location zone;
  final DateTime today;
  final void Function(CalendarEvent event) onOpen;

  @override
  Widget build(BuildContext context) {
    final failure = list.failure;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (failure != null)
          AgendaMessage(
            text: failure,
            actionLabel: 'Reintentar',
            onAction: list.reload,
          ),
        Expanded(child: _body(context, calendarLines(list.items, zone))),
      ],
    );
  }

  Widget _body(BuildContext context, List<CalendarLine> lines) {
    if (lines.isEmpty) {
      if (list.loading) return const Center(child: CircularProgressIndicator());
      if (list.failure != null) return const SizedBox.shrink();
      if (list.truncated) {
        return const AgendaMessage(
          text: 'Hay demasiados eventos en este periodo, acota las fechas.',
        );
      }
      return const AgendaMessage(
        text: 'Nada en este periodo. Crea un evento con «Nuevo evento».',
      );
    }
    return ListView.builder(
      itemCount: lines.length + (list.hasMore ? 1 : 0),
      itemBuilder: (context, index) {
        if (index == lines.length) return _more();
        return switch (lines[index]) {
          DayLine(:final day) => _heading(context, day),
          EventLine(:final event) => _row(context, event),
        };
      },
    );
  }

  Widget _heading(BuildContext context, DateTime day) => Padding(
    padding: const EdgeInsets.fromLTRB(Spacing.lg, Spacing.md, Spacing.lg, Spacing.xs),
    child: Text(dayHeading(day, today), style: context.text.label),
  );

  Widget _row(BuildContext context, CalendarEvent event) => ListTile(
    minVerticalPadding: Spacing.md,
    leading: SizedBox(
      width: 64,
      child: Text(timeLabel(event, zone), style: context.text.small),
    ),
    title: Text(event.title, maxLines: 1, overflow: TextOverflow.ellipsis),
    subtitle: event.location.isEmpty
        ? null
        : Text(event.location, maxLines: 1, overflow: TextOverflow.ellipsis),
    trailing: event.recurring || event.reminderMinutes != null
        ? Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (event.recurring)
                _mark(context, Icons.repeat, 'Se repite'),
              if (event.reminderMinutes != null)
                _mark(context, Icons.notifications_active_outlined, 'Con aviso'),
            ],
          )
        : null,
    onTap: () => onOpen(event),
  );

  Widget _mark(BuildContext context, IconData icon, String label) => Icon(
    icon,
    semanticLabel: label,
    color: context.colors.textMuted,
  );

  Widget _more() => Center(
    child: list.loading
        ? const Padding(
            padding: EdgeInsets.all(Spacing.md),
            child: CircularProgressIndicator(),
          )
        : TextButton(onPressed: list.loadMore, child: const Text('Cargar más')),
  );
}
