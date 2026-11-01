import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/state/voice_send_settings.dart';

import 'support/memory_store.dart';

void main() {
  test('sin ajuste guardado, la nota se queda en el cuadro', () async {
    final settings = VoiceSendSettings(MemoryKeyValueStore());

    await settings.load();

    expect(settings.mode, VoiceSendMode.draft);
  });

  test('el envío automático sobrevive a reiniciar la app', () async {
    final store = MemoryKeyValueStore();
    await VoiceSendSettings(store).choose(VoiceSendMode.auto);

    final restarted = VoiceSendSettings(store);
    expect(
      restarted.mode,
      VoiceSendMode.draft,
      reason: 'nothing is read before load',
    );
    await restarted.load();

    expect(restarted.mode, VoiceSendMode.auto);
  });

  test('volver a dejar en el cuadro también se guarda', () async {
    final store = MemoryKeyValueStore();
    final settings = VoiceSendSettings(store);
    await settings.choose(VoiceSendMode.auto);
    await settings.choose(VoiceSendMode.draft);

    final restarted = VoiceSendSettings(store);
    await restarted.load();

    expect(restarted.mode, VoiceSendMode.draft);
  });

  test(
    'un valor desconocido en el almacén cae en dejar en el cuadro',
    () async {
      final store = MemoryKeyValueStore()
        ..values[VoiceSendSettings.key] = 'siempre';
      final settings = VoiceSendSettings(store);

      await settings.load();

      expect(settings.mode, VoiceSendMode.draft);
    },
  );

  test('elegir avisa a quien escucha, antes de guardar', () async {
    final settings = VoiceSendSettings(MemoryKeyValueStore());
    var notified = 0;
    settings.addListener(() => notified++);

    await settings.choose(VoiceSendMode.auto);

    expect(notified, 1);
    expect(settings.mode, VoiceSendMode.auto);
  });

  test(
    'sin ajuste de micrófono guardado, el botón se mantiene pulsado',
    () async {
      final settings = VoiceSendSettings(MemoryKeyValueStore());

      await settings.load();

      expect(settings.micMode, VoiceMicMode.hold);
    },
  );

  test(
    'tocar para empezar y parar sobrevive a reiniciar, sin tocar el envío',
    () async {
      final store = MemoryKeyValueStore();
      await VoiceSendSettings(store).chooseMic(VoiceMicMode.toggle);

      final restarted = VoiceSendSettings(store);
      await restarted.load();

      expect(restarted.micMode, VoiceMicMode.toggle);
      expect(restarted.mode, VoiceSendMode.draft);
    },
  );

  test('un valor desconocido de micrófono cae en mantener pulsado', () async {
    final store = MemoryKeyValueStore()
      ..values[VoiceSendSettings.micKey] = 'siempre';
    final settings = VoiceSendSettings(store);

    await settings.load();

    expect(settings.micMode, VoiceMicMode.hold);
  });
}
