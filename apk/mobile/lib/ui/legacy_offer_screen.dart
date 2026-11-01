import 'package:flutter/material.dart';

import '../core/app_info.dart';
import '../core/migration/legacy_migration.dart';
import '../core/migration/legacy_pairing.dart';
import '../core/storage/pairing_store.dart';
import 'theme.dart';
import 'widgets/surface_card.dart';

/// Shown before the pairing screen when the old app left a pairing behind.
/// Accepting moves it over; declining opens the normal pairing screen and the
/// offer is not shown again.
class LegacyOfferScreen extends StatefulWidget {
  const LegacyOfferScreen({
    super.key,
    required this.legacy,
    required this.migration,
    required this.onPaired,
    required this.onNewPairing,
  });

  final LegacyPairing legacy;
  final LegacyMigration migration;
  final ValueChanged<Pairing> onPaired;
  final VoidCallback onNewPairing;

  @override
  State<LegacyOfferScreen> createState() => _LegacyOfferScreenState();
}

class _LegacyOfferScreenState extends State<LegacyOfferScreen> {
  bool _busy = false;
  LegacyOutcome? _failure;

  Future<void> _use() async {
    setState(() {
      _busy = true;
      _failure = null;
    });
    final adoption = await widget.migration.adopt(widget.legacy);
    if (!mounted) return;
    final pairing = adoption.pairing;
    if (adoption.outcome == LegacyOutcome.migrated && pairing != null) {
      widget.onPaired(pairing);
      return;
    }
    setState(() {
      _busy = false;
      _failure = adoption.outcome;
    });
  }

  Future<void> _pairAgain() async {
    setState(() => _busy = true);
    await widget.migration.decline();
    if (!mounted) return;
    widget.onNewPairing();
  }

  @override
  Widget build(BuildContext context) {
    final host = widget.legacy.hostLabel;
    final failure = _failure;
    return Scaffold(
      backgroundColor: context.colors.bg,
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(Spacing.lg),
          children: [
            Text(appName, style: context.text.headline),
            const SizedBox(height: Spacing.xl),
            SurfaceCard(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    failure == null
                        ? 'Encontré el emparejamiento de la versión anterior con $host. ¿Usarlo?'
                        : _failureText(failure, host),
                    style: context.text.title,
                  ),
                  if (failure == null) ...[
                    const SizedBox(height: Spacing.sm),
                    Text(
                      'Así no tienes que emparejar otra vez.',
                      style: context.text.bodySecondary,
                    ),
                  ],
                  const SizedBox(height: Spacing.lg),
                  ..._actions(failure, host),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  List<Widget> _actions(LegacyOutcome? failure, String host) {
    if (_busy) {
      return [
        FilledButton(onPressed: null, child: Text('Comprobando con $host…')),
      ];
    }
    final retry = failure == null || failure == LegacyOutcome.unreachable;
    return [
      if (retry)
        FilledButton(
          onPressed: _use,
          child: Text(failure == null ? 'Usar' : 'Reintentar'),
        ),
      if (retry) const SizedBox(height: Spacing.sm),
      OutlinedButton(
        onPressed: _pairAgain,
        child: const Text('Emparejar de nuevo'),
      ),
    ];
  }

  static String _failureText(LegacyOutcome outcome, String host) =>
      switch (outcome) {
        LegacyOutcome.unreachable =>
          'No pude conectar con $host. Revisa que esté encendido y conectado a '
              'Tailscale. Tus datos anteriores siguen guardados.',
        LegacyOutcome.rejected =>
          '$host ya no reconoce este teléfono. Empareja de nuevo; tus datos '
              'anteriores siguen guardados.',
        LegacyOutcome.unverified =>
          'Conecté con $host, pero no pude comprobar el emparejamiento. Empareja '
              'de nuevo; tus datos anteriores siguen guardados.',
        LegacyOutcome.noRecoveryCode =>
          'La versión anterior no guardó una clave de recuperación, así que no '
              'puedo pasar tu emparejamiento. Empareja de nuevo.',
        LegacyOutcome.migrated => '',
      };
}
