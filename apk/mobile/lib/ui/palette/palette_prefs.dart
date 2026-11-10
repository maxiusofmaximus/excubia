import 'dart:convert';

import 'package:excubia/core/storage/pairing_store.dart';

import 'palette.dart';

/// Reads and writes `{id, followSystem}` beside the pairing. A missing value is
/// `null` (first run); a broken one falls back to Noche instead of failing.
class PalettePrefs {
  PalettePrefs(this._store);

  // legacy: persisted key from before the rename, kept as stored
  static const String storageKey = 'hn_palette';

  final KeyValueStore _store;

  Future<PalettePreference?> load() async {
    try {
      final raw = await _store.read(storageKey);
      if (raw == null || raw.isEmpty) return null;
      final decoded = jsonDecode(raw);
      if (decoded is! Map) return PalettePreference.fallback;
      final id = decoded['id'];
      if (id is! String) return PalettePreference.fallback;
      final followSystem = decoded['followSystem'];
      return PalettePreference(
        id: HerdrPalettes.byId(id).id,
        followSystem: followSystem is bool ? followSystem : false,
      );
    } catch (_) {
      return PalettePreference.fallback;
    }
  }

  Future<void> save(PalettePreference preference) async {
    try {
      await _store.write(
        storageKey,
        jsonEncode({
          'id': preference.id,
          'followSystem': preference.followSystem,
        }),
      );
    } catch (_) {
      // A preference is not worth failing a boot or a tap over.
    }
  }
}
