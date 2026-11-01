import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/state/voice_send_settings.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';

/// Ajustes → Voz: keep the note in the field or send it, and hold or tap the mic.
class VoiceSettingsScreen extends StatelessWidget {
  const VoiceSettingsScreen({super.key, required this.voice});

  final VoiceSendSettings voice;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Voz')),
      body: ListenableBuilder(
        listenable: voice,
        builder: (context, _) => ListView(
          padding: const EdgeInsets.all(Spacing.lg),
          children: [
            Text('AL TERMINAR DE HABLAR', style: context.text.label),
            const SizedBox(height: Spacing.sm),
            _ChoiceCard<VoiceSendMode>(
              value: voice.mode,
              onChoose: (mode) => unawaited(voice.choose(mode)),
              tiles: const [
                RadioListTile<VoiceSendMode>(
                  value: VoiceSendMode.draft,
                  title: Text('Dejar en el cuadro para editar'),
                ),
                RadioListTile<VoiceSendMode>(
                  value: VoiceSendMode.auto,
                  title: Text('Enviar automáticamente'),
                ),
              ],
            ),
            const SizedBox(height: Spacing.sm),
            Text(
              'Con el envío automático, lo que digas se manda a Alfred al terminar, '
              'aunque la transcripción tenga fallos. Las acciones peligrosas siguen '
              'pidiendo tu aprobación.',
              style: context.text.small,
            ),
            const SizedBox(height: Spacing.xl),
            Text('BOTÓN DEL MICRÓFONO', style: context.text.label),
            const SizedBox(height: Spacing.sm),
            _ChoiceCard<VoiceMicMode>(
              value: voice.micMode,
              onChoose: (mode) => unawaited(voice.chooseMic(mode)),
              tiles: const [
                RadioListTile<VoiceMicMode>(
                  value: VoiceMicMode.hold,
                  title: Text('Mantener pulsado'),
                ),
                RadioListTile<VoiceMicMode>(
                  value: VoiceMicMode.toggle,
                  title: Text('Tocar para empezar y tocar para parar'),
                ),
              ],
            ),
            const SizedBox(height: Spacing.sm),
            Text(
              'Con “Tocar”, el micrófono sigue grabando hasta que lo toques otra vez '
              '(máximo 60 s).',
              style: context.text.small,
            ),
          ],
        ),
      ),
    );
  }
}

/// One card of exclusive choices; a tap on a tile reports its value.
class _ChoiceCard<T> extends StatelessWidget {
  const _ChoiceCard({
    required this.value,
    required this.onChoose,
    required this.tiles,
  });

  final T value;
  final void Function(T choice) onChoose;
  final List<RadioListTile<T>> tiles;

  @override
  Widget build(BuildContext context) => SurfaceCard(
    padding: EdgeInsets.zero,
    child: Material(
      color: Colors.transparent,
      borderRadius: BorderRadius.circular(16),
      clipBehavior: Clip.antiAlias,
      child: RadioGroup<T>(
        groupValue: value,
        onChanged: (choice) {
          if (choice != null) onChoose(choice);
        },
        child: Column(children: tiles),
      ),
    ),
  );
}
