import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/storage/pairing_store.dart';
import 'package:herdr_notify/ui/palette/palette.dart';
import 'package:herdr_notify/ui/palette/palette_controller.dart';
import 'package:herdr_notify/ui/palette/palette_prefs.dart';

import 'support/memory_store.dart';

void main() {
  late MemoryKeyValueStore values;
  late PalettePrefs prefs;

  setUp(() {
    values = MemoryKeyValueStore();
    prefs = PalettePrefs(values);
  });

  test('las seis paletas tienen id estable, nombre y nota', () {
    expect(HerdrPalettes.all.map((palette) => palette.id), [
      'noche',
      'pergamino',
      'carbon',
      'papel',
      'medianoche',
      'contraste',
    ]);
    expect(
      HerdrPalettes.all.map((palette) => palette.id).toSet(),
      hasLength(6),
    );
    for (final palette in HerdrPalettes.all) {
      expect(palette.name, isNotEmpty);
      expect(palette.note, isNotEmpty);
    }
    expect(HerdrPalettes.byId('desconocida').id, 'noche');
  });

  test(
    'instalación nueva: Noche, sin seguir el sistema y con pantalla',
    () async {
      final controller = await PaletteController.load(
        prefs: prefs,
        hasPairing: false,
      );

      expect(controller.needsChoice, isTrue);
      expect(controller.followSystem, isFalse);
      expect(controller.paletteFor(Brightness.dark).id, 'noche');
      expect(controller.paletteFor(Brightness.light).id, 'noche');
      expect(await prefs.load(), isNull);
    },
  );

  test(
    'instalación existente sin preferencia: sigue el sistema y no hay pantalla',
    () async {
      final controller = await PaletteController.load(
        prefs: prefs,
        hasPairing: true,
      );

      expect(controller.needsChoice, isFalse);
      expect(controller.followSystem, isTrue);
      expect(controller.paletteFor(Brightness.dark).id, 'noche');
      expect(controller.paletteFor(Brightness.light).id, 'pergamino');
      expect((await prefs.load())?.followSystem, isTrue);
    },
  );

  test('preferencia explícita: se respeta en claro y oscuro', () async {
    await prefs.save(
      const PalettePreference(id: 'carbon', followSystem: false),
    );
    final controller = await PaletteController.load(
      prefs: prefs,
      hasPairing: true,
    );

    expect(controller.needsChoice, isFalse);
    expect(controller.paletteFor(Brightness.dark).id, 'carbon');
    expect(controller.paletteFor(Brightness.light).id, 'carbon');
  });

  test(
    'seguir el sistema resuelve Noche de noche y Pergamino de día',
    () async {
      await prefs.save(
        const PalettePreference(id: 'carbon', followSystem: true),
      );
      final controller = await PaletteController.load(
        prefs: prefs,
        hasPairing: true,
      );

      expect(controller.paletteFor(Brightness.dark).id, 'noche');
      expect(controller.paletteFor(Brightness.light).id, 'pergamino');
    },
  );

  test('prefs corruptas: Noche y sin pantalla', () async {
    values.values[PalettePrefs.storageKey] = '{roto';

    final controller = await PaletteController.load(
      prefs: prefs,
      hasPairing: false,
    );

    expect(controller.needsChoice, isFalse);
    expect(controller.followSystem, isFalse);
    expect(controller.paletteFor(Brightness.light).id, 'noche');
  });

  test('un almacén que falla al leer también cae en Noche', () async {
    final controller = await PaletteController.load(
      prefs: PalettePrefs(_BrokenStore()),
      hasPairing: true,
    );

    expect(controller.needsChoice, isFalse);
    expect(controller.followSystem, isFalse);
    expect(controller.paletteFor(Brightness.light).id, 'noche');
  });

  test('cambiar de paleta la guarda y se aplica', () async {
    final controller = await PaletteController.load(
      prefs: prefs,
      hasPairing: false,
    );

    await controller.choose('medianoche');

    expect(controller.paletteFor(Brightness.light).id, 'medianoche');
    final reloaded = await PaletteController.load(
      prefs: prefs,
      hasPairing: true,
    );
    expect(reloaded.paletteFor(Brightness.dark).id, 'medianoche');
  });

  test('preview no guarda; completeFirstRun sí', () async {
    final controller = await PaletteController.load(
      prefs: prefs,
      hasPairing: false,
    );

    controller.preview('papel');

    expect(controller.paletteFor(Brightness.dark).id, 'papel');
    expect(await prefs.load(), isNull);

    await controller.completeFirstRun();

    expect(controller.needsChoice, isFalse);
    expect((await prefs.load())?.id, 'papel');
  });

  test('seguir el sistema se puede activar y guardar', () async {
    final controller = await PaletteController.load(
      prefs: prefs,
      hasPairing: true,
    );

    await controller.setFollowSystem(false);
    expect(controller.followSystem, isFalse);
    expect((await prefs.load())?.followSystem, isFalse);

    await controller.setFollowSystem(true);
    expect((await prefs.load())?.followSystem, isTrue);
  });

  test('elegir una tarjeta desactiva seguir el sistema', () async {
    final controller = await PaletteController.load(
      prefs: prefs,
      hasPairing: true,
    );

    controller.preview('contraste');

    expect(controller.followSystem, isFalse);
    expect(controller.paletteFor(Brightness.light).id, 'contraste');
  });
}

class _BrokenStore implements KeyValueStore {
  @override
  Future<String?> read(String key) => throw StateError('sin almacén');

  @override
  Future<void> write(String key, String value) async {}

  @override
  Future<void> delete(String key) async {}
}
