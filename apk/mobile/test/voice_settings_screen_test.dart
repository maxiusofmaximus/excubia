import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/voice_send_settings.dart';
import 'package:excubia/ui/chat/voice_settings_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/memory_store.dart';

void main() {
  testWidgets(
    'el ajuste ofrece dos opciones y por defecto deja la nota en el cuadro',
    (tester) async {
      final store = MemoryKeyValueStore();
      final voice = VoiceSendSettings(store);
      await voice.load();

      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: VoiceSettingsScreen(voice: voice),
        ),
      );

      expect(find.text('Dejar en el cuadro para editar'), findsOneWidget);
      expect(find.text('Enviar automáticamente'), findsOneWidget);
      expect(
        find.text(
          'Con el envío automático, lo que digas se manda a Alfred al terminar, '
          'aunque la transcripción tenga fallos. Las acciones peligrosas siguen '
          'pidiendo tu aprobación.',
        ),
        findsOneWidget,
      );
      expect(
        tester
            .widget<RadioGroup<VoiceSendMode>>(
              find.byType(RadioGroup<VoiceSendMode>),
            )
            .groupValue,
        VoiceSendMode.draft,
      );
    },
  );

  testWidgets('elegir enviar automáticamente lo cambia y lo guarda', (
    tester,
  ) async {
    final store = MemoryKeyValueStore();
    final voice = VoiceSendSettings(store);
    await voice.load();

    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: VoiceSettingsScreen(voice: voice),
      ),
    );
    await tester.tap(find.text('Enviar automáticamente'));
    await tester.pumpAndSettle();

    expect(voice.mode, VoiceSendMode.auto);
    expect(store.values[VoiceSendSettings.key], 'auto');
    expect(
      tester
          .widget<RadioGroup<VoiceSendMode>>(
            find.byType(RadioGroup<VoiceSendMode>),
          )
          .groupValue,
      VoiceSendMode.auto,
    );
  });

  testWidgets(
    'el botón del micrófono por defecto se mantiene pulsado y ofrece tocar',
    (tester) async {
      final voice = VoiceSendSettings(MemoryKeyValueStore());
      await voice.load();

      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: VoiceSettingsScreen(voice: voice),
        ),
      );

      expect(find.text('BOTÓN DEL MICRÓFONO'), findsOneWidget);
      expect(find.text('Mantener pulsado'), findsOneWidget);
      expect(
        find.text('Tocar para empezar y tocar para parar'),
        findsOneWidget,
      );
      expect(
        find.text(
          'Con “Tocar”, el micrófono sigue grabando hasta que lo toques otra vez (máximo 60 s).',
        ),
        findsOneWidget,
      );
      expect(
        tester
            .widget<RadioGroup<VoiceMicMode>>(
              find.byType(RadioGroup<VoiceMicMode>),
            )
            .groupValue,
        VoiceMicMode.hold,
      );
    },
  );

  testWidgets(
    'elegir tocar para empezar y parar lo guarda sin tocar el envío',
    (tester) async {
      final store = MemoryKeyValueStore();
      final voice = VoiceSendSettings(store);
      await voice.load();

      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: VoiceSettingsScreen(voice: voice),
        ),
      );
      await tester.tap(find.text('Tocar para empezar y tocar para parar'));
      await tester.pumpAndSettle();

      expect(voice.micMode, VoiceMicMode.toggle);
      expect(store.values[VoiceSendSettings.micKey], 'toggle');
      expect(store.values.containsKey(VoiceSendSettings.key), isFalse);
    },
  );
}
