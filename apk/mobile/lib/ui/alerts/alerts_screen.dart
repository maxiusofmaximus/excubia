import 'package:flutter/material.dart';

import '../../core/alerts/alert_platform.dart';
import '../../core/api/alerts_api.dart';
import '../theme.dart';
import 'alerts_setup_section.dart';

/// Opens Ajustes → Avisos con la pantalla apagada from the chat header.
void openAlertsSettings(BuildContext context, AlertsApi rules) {
  Navigator.of(context).push(
    MaterialPageRoute<void>(builder: (_) => AlertsScreen(rules: rules)),
  );
}

/// The alerts screen: what the phone must allow for the alerts to arrive. It only reads
/// state and opens the system screens.
class AlertsScreen extends StatelessWidget {
  const AlertsScreen({
    super.key,
    required this.rules,
    this.alerts = const MethodChannelAlertPlatform(),
  });

  final AlertsApi rules;
  final AlertPlatform alerts;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Avisos')),
      body: ListView(
        padding: const EdgeInsets.all(Spacing.lg),
        children: [AlertsSetupSection(platform: alerts, rules: rules)],
      ),
    );
  }
}
