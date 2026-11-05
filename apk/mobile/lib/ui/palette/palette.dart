import 'package:flutter/material.dart';

import 'palette_colors.dart';

/// A named palette: the tokens plus the brightness the app should use while it
/// is active. `Brightness` is not a property of the colours, so it lives here.
@immutable
class HerdrPalette {
  const HerdrPalette({
    required this.id,
    required this.name,
    required this.note,
    required this.brightness,
    required this.colors,
  });

  final String id;
  final String name;
  final String note;
  final Brightness brightness;
  final HerdrColors colors;
}

class HerdrPalettes {
  HerdrPalettes._();

  static const String nocheId = 'noche';
  static const String pergaminoId = 'pergamino';
  static const String carbonId = 'carbon';
  static const String papelId = 'papel';
  static const String medianocheId = 'medianoche';
  static const String contrasteId = 'contraste';

  static const HerdrPalette noche = HerdrPalette(
    id: nocheId,
    name: 'Noche',
    note: 'Azul noche y latón',
    brightness: Brightness.dark,
    colors: HerdrColors.noche,
  );

  static const HerdrPalette pergamino = HerdrPalette(
    id: pergaminoId,
    name: 'Pergamino',
    note: 'Papel cálido',
    brightness: Brightness.light,
    colors: HerdrColors.pergamino,
  );

  static const HerdrPalette carbon = HerdrPalette(
    id: carbonId,
    name: 'Carbón',
    note: 'Gris oscuro neutro',
    brightness: Brightness.dark,
    colors: HerdrColors.carbon,
  );

  static const HerdrPalette papel = HerdrPalette(
    id: papelId,
    name: 'Papel',
    note: 'Blanco roto neutro',
    brightness: Brightness.light,
    colors: HerdrColors.papel,
  );

  static const HerdrPalette medianoche = HerdrPalette(
    id: medianocheId,
    name: 'Medianoche',
    note: 'Azul profundo',
    brightness: Brightness.dark,
    colors: HerdrColors.medianoche,
  );

  static const HerdrPalette contraste = HerdrPalette(
    id: contrasteId,
    name: 'Alto contraste',
    note: 'Negro y blanco',
    brightness: Brightness.dark,
    colors: HerdrColors.contraste,
  );

  static const List<HerdrPalette> all = [
    noche,
    pergamino,
    carbon,
    papel,
    medianoche,
    contraste,
  ];

  static HerdrPalette byId(String id) =>
      all.firstWhere((palette) => palette.id == id, orElse: () => noche);
}

/// What the user chose. `followSystem` wins over [id] while it is on.
@immutable
class PalettePreference {
  const PalettePreference({required this.id, required this.followSystem});

  static const PalettePreference fallback = PalettePreference(
    id: HerdrPalettes.nocheId,
    followSystem: false,
  );

  final String id;
  final bool followSystem;

  PalettePreference copyWith({String? id, bool? followSystem}) =>
      PalettePreference(
        id: id ?? this.id,
        followSystem: followSystem ?? this.followSystem,
      );
}

/// The palette that paints the app right now: the system's pair when the user
/// follows it, otherwise the explicit choice.
HerdrPalette resolvePalette(PalettePreference preference, Brightness system) {
  if (preference.followSystem) {
    return system == Brightness.dark
        ? HerdrPalettes.noche
        : HerdrPalettes.pergamino;
  }
  return HerdrPalettes.byId(preference.id);
}
