import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme.dart';
import 'calendar_format.dart';
import 'calendar_pickers.dart';
import 'calendar_recurrence.dart';

/// «Repetir»: how often, which weekdays, and when it ends. A rule the form cannot
/// write is shown read-only, and it stays as it is until the person picks another.
class CalendarRepeatField extends StatelessWidget {
  const CalendarRepeatField({
    super.key,
    required this.repeat,
    required this.startDay,
    required this.enabled,
    required this.onChanged,
  });

  final Repeat repeat;

  /// The day the event starts: the weekday a new weekly rule takes, and the first day
  /// an end may fall on.
  final DateTime startDay;
  final bool enabled;
  final ValueChanged<Repeat> onChanged;

  @override
  Widget build(BuildContext context) => switch (repeat) {
    NoRepeat() => _freqMenu(null),
    CustomRepeat() => _custom(context),
    RepeatRule rule => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _freqMenu(rule.freq),
        const SizedBox(height: Spacing.sm),
        _interval(rule),
        if (rule.freq == RepeatFreq.weekly) ...[
          const SizedBox(height: Spacing.sm),
          _weekdays(rule),
        ],
        const SizedBox(height: Spacing.sm),
        _end(context, rule),
        const SizedBox(height: Spacing.sm),
        Text(repeatWords(rule), style: context.text.bodySecondary),
      ],
    ),
  };

  Widget _freqMenu(RepeatFreq? freq) => DropdownMenu<RepeatFreq?>(
    initialSelection: freq,
    label: const Text('Repetir'),
    expandedInsets: EdgeInsets.zero,
    enabled: enabled,
    dropdownMenuEntries: [
      const DropdownMenuEntry(value: null, label: 'No se repite'),
      for (final value in RepeatFreq.values)
        DropdownMenuEntry(value: value, label: value.words(1)),
    ],
    onSelected: _pickFreq,
  );

  void _pickFreq(RepeatFreq? freq) {
    if (freq == null) {
      onChanged(const NoRepeat());
      return;
    }
    final current = repeat;
    onChanged(
      current is RepeatRule
          ? current.copyWith(freq: freq)
          : RepeatRule.starting(freq, startDay),
    );
  }

  Widget _custom(BuildContext context) => Row(
    children: [
      Expanded(
        child: Text(
          'Repetición personalizada',
          style: context.text.bodySecondary,
        ),
      ),
      TextButton(
        onPressed: enabled ? () => onChanged(const NoRepeat()) : null,
        child: const Text('Cambiar'),
      ),
    ],
  );

  Widget _interval(RepeatRule rule) => Row(
    children: [
      SizedBox(
        width: 88,
        child: TextFormField(
          initialValue: '${rule.interval}',
          enabled: enabled,
          keyboardType: TextInputType.number,
          inputFormatters: [
            FilteringTextInputFormatter.digitsOnly,
            LengthLimitingTextInputFormatter(3),
          ],
          decoration: const InputDecoration(labelText: 'Cada'),
          onChanged: (text) =>
              onChanged(rule.copyWith(interval: int.tryParse(text) ?? 0)),
        ),
      ),
      const SizedBox(width: Spacing.sm),
      Text(rule.freq.unit(rule.interval)),
    ],
  );

  Widget _weekdays(RepeatRule rule) => Wrap(
    spacing: Spacing.sm,
    children: [
      for (var day = DateTime.monday; day <= DateTime.sunday; day++)
        Tooltip(
          message: weekdayName(day),
          child: FilterChip(
            label: Text(_dayLetters[day - 1]),
            selected: rule.days.contains(day),
            onSelected: enabled
                ? (on) => onChanged(
                    rule.copyWith(days: _toggled(rule.days, day, on)),
                  )
                : null,
          ),
        ),
    ],
  );

  // Parenthesised: a bare `..remove` would apply to the whole conditional and undo the add.
  Set<int> _toggled(Set<int> days, int day, bool on) =>
      on ? {...days, day} : ({...days}..remove(day));

  Widget _end(BuildContext context, RepeatRule rule) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      DropdownMenu<RepeatEndKind>(
        initialSelection: _kindOf(rule.end),
        label: const Text('Fin'),
        expandedInsets: EdgeInsets.zero,
        enabled: enabled,
        dropdownMenuEntries: const [
          DropdownMenuEntry(value: RepeatEndKind.never, label: 'Nunca'),
          DropdownMenuEntry(value: RepeatEndKind.count, label: 'Tras N veces'),
          DropdownMenuEntry(
            value: RepeatEndKind.until,
            label: 'Hasta una fecha',
          ),
        ],
        onSelected: (kind) => onChanged(rule.copyWith(end: _endOf(kind))),
      ),
      ..._endDetail(context, rule),
    ],
  );

  List<Widget> _endDetail(BuildContext context, RepeatRule rule) =>
      switch (rule.end) {
        EndsAfter(:final count) => [
          const SizedBox(height: Spacing.sm),
          TextFormField(
            initialValue: '$count',
            enabled: enabled,
            keyboardType: TextInputType.number,
            inputFormatters: [
              FilteringTextInputFormatter.digitsOnly,
              LengthLimitingTextInputFormatter(4),
            ],
            decoration: const InputDecoration(labelText: 'Veces'),
            onChanged: (text) => onChanged(
              rule.copyWith(end: EndsAfter(int.tryParse(text) ?? 0)),
            ),
          ),
        ],
        EndsOn(:final day) => [
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: TextButton(
              onPressed: enabled ? () => _pickUntil(context, rule, day) : null,
              child: Text('Hasta el ${dayLabel(day)} ${day.year}'),
            ),
          ),
        ],
        EndsNever() => const <Widget>[],
      };

  Future<void> _pickUntil(
    BuildContext context,
    RepeatRule rule,
    DateTime current,
  ) async {
    final day = await pickCalendarDay(context, current);
    if (day != null) onChanged(rule.copyWith(end: EndsOn(day)));
  }

  static RepeatEndKind _kindOf(RepeatEnd end) => switch (end) {
    EndsNever() => RepeatEndKind.never,
    EndsAfter() => RepeatEndKind.count,
    EndsOn() => RepeatEndKind.until,
  };

  RepeatEnd _endOf(RepeatEndKind? kind) => switch (kind) {
    RepeatEndKind.count => const EndsAfter(10),
    RepeatEndKind.until => EndsOn(startDay),
    RepeatEndKind.never || null => const EndsNever(),
  };
}

/// The ways «Repetir» can end, as the menu offers them.
enum RepeatEndKind { never, count, until }

const List<String> _dayLetters = ['L', 'M', 'X', 'J', 'V', 'S', 'D'];
