import 'package:flutter/material.dart';

import '../../core/api/alerts_api.dart';
import '../../core/api/alerts_failure_text.dart';
import '../../core/api/alerts_models.dart';
import '../theme.dart';

/// «Nueva regla»: one rule, written as the sentence it will read as. Pops the saved rule.
class AlertRuleFormScreen extends StatefulWidget {
  const AlertRuleFormScreen({super.key, required this.api});

  final AlertsApi api;

  @override
  State<AlertRuleFormScreen> createState() => _AlertRuleFormScreenState();
}

class _AlertRuleFormScreenState extends State<AlertRuleFormScreen> {
  late final Future<List<String>> _agents = widget.api.agentNames();

  AlertTrigger _trigger = AlertTrigger.paneStatus;
  String? _agent;
  AlertRuleStatus _status = AlertRuleStatus.blocked;
  AlertAction _action = AlertAction.notify;
  bool _saving = false;
  String? _failure;

  bool get _onAgent => _trigger == AlertTrigger.paneStatus;

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _failure = null;
    });
    try {
      final rule = await widget.api.createAlertRule(
        trigger: _trigger,
        agent: _onAgent ? _agent : null,
        status: _onAgent ? _status : null,
        action: _action,
      );
      if (mounted) Navigator.of(context).pop(rule);
    } on Object catch (error) {
      if (mounted) {
        setState(() {
          _saving = false;
          _failure = alertsFailureText(error);
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Nueva regla')),
      body: ListView(
        padding: const EdgeInsets.all(Spacing.lg),
        children: [
          Text('TIPO', style: text.label),
          const SizedBox(height: Spacing.sm),
          SegmentedButton<AlertTrigger>(
            segments: const [
              ButtonSegment(
                value: AlertTrigger.paneStatus,
                label: Text('Agente'),
              ),
              ButtonSegment(
                value: AlertTrigger.calendarReminder,
                label: Text('Recordatorio'),
              ),
            ],
            selected: {_trigger},
            showSelectedIcon: false,
            onSelectionChanged: (picked) =>
                setState(() => _trigger = picked.single),
          ),
          if (_onAgent) ...[
            const SizedBox(height: Spacing.xl),
            Text('CUÁNDO', style: text.label),
            const SizedBox(height: Spacing.sm),
            _AgentPicker(
              agents: _agents,
              selected: _agent,
              onSelected: (agent) => setState(() => _agent = agent),
            ),
            const SizedBox(height: Spacing.md),
            SegmentedButton<AlertRuleStatus>(
              segments: const [
                ButtonSegment(
                  value: AlertRuleStatus.blocked,
                  label: Text('Se bloquea'),
                ),
                ButtonSegment(
                  value: AlertRuleStatus.done,
                  label: Text('Termina'),
                ),
              ],
              selected: {_status},
              showSelectedIcon: false,
              onSelectionChanged: (picked) =>
                  setState(() => _status = picked.single),
            ),
          ],
          const SizedBox(height: Spacing.xl),
          Text('ACCIÓN', style: text.label),
          const SizedBox(height: Spacing.sm),
          SegmentedButton<AlertAction>(
            segments: const [
              ButtonSegment(value: AlertAction.notify, label: Text('Avisar')),
              ButtonSegment(value: AlertAction.mute, label: Text('Silenciar')),
            ],
            selected: {_action},
            showSelectedIcon: false,
            onSelectionChanged: (picked) =>
                setState(() => _action = picked.single),
          ),
          const SizedBox(height: Spacing.lg),
          Text(
            'Quedará así: ${alertSentence(trigger: _trigger, agent: _agent, status: _status, action: _action)}.',
            style: text.body,
          ),
          if (_failure != null) ...[
            const SizedBox(height: Spacing.md),
            Text(
              _failure!,
              style: text.bodySecondary.copyWith(
                color: context.colors.statusBlocked,
              ),
            ),
          ],
          const SizedBox(height: Spacing.xl),
          FilledButton(
            onPressed: _saving ? null : _save,
            child: const Text('Crear regla'),
          ),
        ],
      ),
    );
  }
}

/// The agents on the PC, plus «Cualquiera». A failed load says so and still leaves «Cualquiera».
class _AgentPicker extends StatelessWidget {
  const _AgentPicker({
    required this.agents,
    required this.selected,
    required this.onSelected,
  });

  final Future<List<String>> agents;
  final String? selected;
  final ValueChanged<String?> onSelected;

  @override
  Widget build(BuildContext context) => FutureBuilder<List<String>>(
    future: agents,
    builder: (context, snapshot) {
      if (snapshot.connectionState != ConnectionState.done) {
        return const LinearProgressIndicator();
      }
      final names = snapshot.data ?? const <String>[];
      final error = snapshot.error;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (error != null) ...[
            Text(alertsFailureText(error), style: context.text.bodySecondary),
            const SizedBox(height: Spacing.sm),
          ],
          DropdownMenu<String?>(
            initialSelection: selected,
            label: const Text('Nombre del agente'),
            expandedInsets: EdgeInsets.zero,
            dropdownMenuEntries: [
              const DropdownMenuEntry(value: null, label: 'Cualquiera'),
              for (final name in names)
                DropdownMenuEntry(value: name, label: name),
            ],
            onSelected: onSelected,
          ),
        ],
      );
    },
  );
}
