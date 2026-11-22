import 'package:flutter/material.dart';

import '../../core/api/alerts_api.dart';
import '../../core/api/alerts_failure_text.dart';
import '../../core/api/alerts_models.dart';
import '../agenda/agenda_widgets.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';
import 'alert_rule_form_screen.dart';

/// Ajustes → Avisos → «Reglas de aviso»: which changes alert. The host applies the rules and its
/// own brakes; this screen only shows them and changes them.
class AlertRulesScreen extends StatefulWidget {
  const AlertRulesScreen({super.key, required this.api});

  final AlertsApi api;

  @override
  State<AlertRulesScreen> createState() => _AlertRulesScreenState();
}

class _AlertRulesScreenState extends State<AlertRulesScreen> {
  AlertsMode? _mode;
  List<AlertRule>? _rules;
  Object? _loadError;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    setState(() => _loadError = null);
    try {
      final (mode, rules) = await (
        widget.api.alertsMode(),
        widget.api.alertsRules(),
      ).wait;
      if (!mounted) return;
      setState(() {
        _mode = mode;
        _rules = rules;
      });
    } on Object catch (error) {
      if (mounted) setState(() => _loadError = error);
    }
  }

  Future<void> _setMode(AlertsMode mode) async {
    try {
      final chosen = await widget.api.setAlertsMode(mode);
      if (mounted) setState(() => _mode = chosen);
    } on Object catch (error) {
      _say(error);
    }
  }

  Future<void> _setEnabled(AlertRule rule, bool enabled) async {
    try {
      final changed = await widget.api.setAlertRuleEnabled(rule.id, enabled);
      if (mounted) {
        setState(
          () => _rules = [
            for (final item in _rules ?? const <AlertRule>[])
              item.id == changed.id ? changed : item,
          ],
        );
      }
    } on Object catch (error) {
      _say(error);
    }
  }

  Future<void> _delete(AlertRule rule) async {
    final sure = await confirmDelete(
      context,
      title: 'Borrar esta regla',
      body: '«${rule.sentence}» dejará de aplicarse.',
    );
    if (!sure || !mounted) return;
    try {
      await widget.api.deleteAlertRule(rule.id);
      if (mounted) {
        setState(
          () => _rules = [
            for (final item in _rules ?? const <AlertRule>[])
              if (item.id != rule.id) item,
          ],
        );
      }
    } on Object catch (error) {
      _say(error);
    }
  }

  Future<void> _newRule() async {
    final created = await Navigator.of(context).push<AlertRule>(
      MaterialPageRoute<AlertRule>(
        builder: (_) => AlertRuleFormScreen(api: widget.api),
      ),
    );
    if (created != null && mounted) {
      setState(() => _rules = [...?_rules, created]);
    }
  }

  void _say(Object error) {
    if (!mounted) return;
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(alertsFailureText(error))));
  }

  @override
  Widget build(BuildContext context) {
    final mode = _mode;
    final rules = _rules;
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Reglas de aviso')),
      floatingActionButton: rules == null
          ? null
          : FloatingActionButton.extended(
              onPressed: _newRule,
              icon: const Icon(Icons.add),
              label: const Text('Nueva regla'),
            ),
      body: switch ((mode, rules, _loadError)) {
        (final mode?, final rules?, _) => _Loaded(
          mode: mode,
          rules: rules,
          onMode: _setMode,
          onEnabled: _setEnabled,
          onDelete: _delete,
        ),
        (_, _, final error?) => AgendaMessage(
          text: alertsFailureText(error),
          actionLabel: 'Reintentar',
          onAction: _reload,
        ),
        _ => const Center(child: CircularProgressIndicator()),
      },
    );
  }
}

class _Loaded extends StatelessWidget {
  const _Loaded({
    required this.mode,
    required this.rules,
    required this.onMode,
    required this.onEnabled,
    required this.onDelete,
  });

  final AlertsMode mode;
  final List<AlertRule> rules;
  final ValueChanged<AlertsMode> onMode;
  final void Function(AlertRule rule, bool enabled) onEnabled;
  final ValueChanged<AlertRule> onDelete;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    return ListView(
      padding: const EdgeInsets.all(Spacing.lg),
      children: [
        Text('MODO', style: text.label),
        const SizedBox(height: Spacing.sm),
        SegmentedButton<AlertsMode>(
          segments: const [
            ButtonSegment(value: AlertsMode.all, label: Text('Avisar de todo')),
            ButtonSegment(
              value: AlertsMode.rulesOnly,
              label: Text('Solo lo que yo elija'),
            ),
          ],
          selected: {mode},
          showSelectedIcon: false,
          onSelectionChanged: (picked) => onMode(picked.single),
        ),
        const SizedBox(height: Spacing.sm),
        Text(
          'Avisar de todo: avisa de cada agente que se bloquea o termina, salvo lo que silencies.',
          style: text.bodySecondary,
        ),
        const SizedBox(height: Spacing.xs),
        Text(
          'Solo lo que yo elija: avisa solo de lo que casa con una regla de aviso.',
          style: text.bodySecondary,
        ),
        const SizedBox(height: Spacing.xs),
        Text(
          'El silencio gana siempre: una regla de silenciar impide el aviso en cualquier modo.',
          style: text.bodySecondary,
        ),
        const SizedBox(height: Spacing.xl),
        Text('REGLAS', style: text.label),
        const SizedBox(height: Spacing.sm),
        if (rules.isEmpty)
          Text(
            'Aún no hay reglas. Con «Solo lo que yo elija», ningún aviso llega hasta que crees una con «Nueva regla».',
            style: text.bodySecondary,
          )
        else
          SurfaceCard(
            padding: EdgeInsets.zero,
            child: Column(
              children: [
                for (final (index, rule) in rules.indexed) ...[
                  if (index > 0) const Divider(height: 1),
                  _RuleRow(
                    rule: rule,
                    onEnabled: (enabled) => onEnabled(rule, enabled),
                    onDelete: () => onDelete(rule),
                  ),
                ],
              ],
            ),
          ),
        const SizedBox(height: Spacing.lg),
        Text(
          'Los avisos siguen respetando los frenos del servidor (esperas entre avisos, horas de silencio).',
          style: text.bodySecondary,
        ),
      ],
    );
  }
}

class _RuleRow extends StatelessWidget {
  const _RuleRow({
    required this.rule,
    required this.onEnabled,
    required this.onDelete,
  });

  final AlertRule rule;
  final ValueChanged<bool> onEnabled;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) => ListTile(
    minVerticalPadding: Spacing.md,
    title: Text(rule.sentence),
    trailing: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Switch(value: rule.enabled, onChanged: onEnabled),
        IconButton(
          tooltip: 'Borrar regla',
          icon: const Icon(Icons.delete_outline),
          onPressed: onDelete,
        ),
      ],
    ),
  );
}
