import 'package:flutter/material.dart';

import 'palette.dart';
import 'palette_prefs.dart';

/// The live palette choice. The root listens to it to rebuild `MaterialApp`,
/// and the palette screen listens to it to move the selected border.
class PaletteController extends ChangeNotifier {
  PaletteController._(this._prefs, this._preference, this._needsChoice);

  final PalettePrefs _prefs;
  PalettePreference _preference;
  bool _needsChoice;

  /// A stored preference wins. Without one, an existing install migrates to
  /// «seguir el sistema» and a new one gets Noche plus the first-run screen.
  static Future<PaletteController> load({
    required PalettePrefs prefs,
    required bool hasPairing,
  }) async {
    final stored = await prefs.load();
    if (stored != null) {
      return PaletteController._(prefs, stored, false);
    }
    final migrated = PalettePreference(
      id: HerdrPalettes.nocheId,
      followSystem: hasPairing,
    );
    if (hasPairing) await prefs.save(migrated);
    return PaletteController._(prefs, migrated, !hasPairing);
  }

  bool get needsChoice => _needsChoice;

  bool get followSystem => _preference.followSystem;

  HerdrPalette paletteFor(Brightness system) =>
      resolvePalette(_preference, system);

  /// First-run changes are only previewed; «Continuar» persists them.
  void preview(String id) => _apply(
    PalettePreference(id: HerdrPalettes.byId(id).id, followSystem: false),
  );

  void previewFollowSystem(bool value) =>
      _apply(_preference.copyWith(followSystem: value));

  /// Settings changes persist on every tap.
  Future<void> choose(String id) {
    preview(id);
    return _prefs.save(_preference);
  }

  Future<void> setFollowSystem(bool value) {
    previewFollowSystem(value);
    return _prefs.save(_preference);
  }

  Future<void> completeFirstRun() {
    _needsChoice = false;
    notifyListeners();
    return _prefs.save(_preference);
  }

  void _apply(PalettePreference next) {
    _preference = next;
    notifyListeners();
  }
}
