import 'package:flutter/material.dart';

import '../../core/state/calendar_controller.dart';
import '../theme.dart';

/// Zones a company is likely to use. Any other IANA name can be typed; the host checks it.
const List<String> calendarZones = [
  'Europe/Madrid',
  'Atlantic/Canary',
  'Europe/London',
  'Europe/Lisbon',
  'Europe/Paris',
  'Europe/Berlin',
  'Europe/Rome',
  'Europe/Athens',
  'Europe/Helsinki',
  'Europe/Moscow',
  'America/New_York',
  'America/Chicago',
  'America/Denver',
  'America/Los_Angeles',
  'America/Mexico_City',
  'America/Bogota',
  'America/Lima',
  'America/Santiago',
  'America/Argentina/Buenos_Aires',
  'America/Sao_Paulo',
  'Africa/Cairo',
  'Africa/Johannesburg',
  'Asia/Dubai',
  'Asia/Kolkata',
  'Asia/Shanghai',
  'Asia/Tokyo',
  'Australia/Sydney',
  'Pacific/Auckland',
  'UTC',
];

/// The company without a zone: the notice that it is fixed once, and the choice of it.
class CalendarZoneView extends StatefulWidget {
  const CalendarZoneView({
    super.key,
    required this.unset,
    required this.onSave,
  });

  final ZoneUnset unset;
  final void Function(String zone) onSave;

  @override
  State<CalendarZoneView> createState() => _CalendarZoneViewState();
}

class _CalendarZoneViewState extends State<CalendarZoneView> {
  String _choice = calendarZones.first;
  final TextEditingController _typed = TextEditingController();

  @override
  void dispose() {
    _typed.dispose();
    super.dispose();
  }

  /// A zone typed by hand wins over the list.
  void _save() {
    final typed = _typed.text.trim();
    widget.onSave(typed.isEmpty ? _choice : typed);
  }

  @override
  Widget build(BuildContext context) {
    final unset = widget.unset;
    final failure = unset.failure;
    return ListView(
      padding: const EdgeInsets.all(Spacing.lg),
      children: [
        Text('La empresa aún no tiene zona horaria', style: context.text.title),
        const SizedBox(height: Spacing.sm),
        Text(
          'Los eventos se muestran y se crean en la hora de la empresa. Sin zona no se '
          'puede crear ningún evento. La zona se fija una sola vez.',
          style: context.text.bodySecondary,
        ),
        const SizedBox(height: Spacing.lg),
        DropdownMenu<String>(
          initialSelection: _choice,
          label: const Text('Zona'),
          expandedInsets: EdgeInsets.zero,
          dropdownMenuEntries: [
            for (final zone in calendarZones)
              DropdownMenuEntry(value: zone, label: zone),
          ],
          onSelected: (value) => setState(() => _choice = value ?? _choice),
        ),
        const SizedBox(height: Spacing.md),
        TextField(
          controller: _typed,
          enabled: !unset.saving,
          decoration: const InputDecoration(
            labelText: 'O escribe otra (p. ej. Asia/Tokyo)',
          ),
        ),
        if (failure != null) ...[
          const SizedBox(height: Spacing.md),
          Text(failure, style: context.text.bodySecondary),
        ],
        const SizedBox(height: Spacing.lg),
        FilledButton(
          onPressed: unset.saving ? null : _save,
          child: const Text('Guardar zona'),
        ),
      ],
    );
  }
}
