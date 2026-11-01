import 'package:flutter/foundation.dart';

import '../storage/pairing_store.dart';

/// Where a voice note's text goes: into the composer to edit, or to Alfred at once.
enum VoiceSendMode { draft, auto }

/// How the microphone button records: while held down, or by taps.
enum VoiceMicMode { hold, toggle }

/// The voice choices: «Al terminar de hablar» and «Botón del micrófono». They
/// are kept in the phone's secure store and read once at start; without a
/// store they live only in memory.
class VoiceSendSettings extends ChangeNotifier {
  VoiceSendSettings([this._store]);

  static const String key = 'hn_voice_send_mode';
  static const String micKey = 'hn_voice_mic_mode';

  final KeyValueStore? _store;
  VoiceSendMode _mode = VoiceSendMode.draft;
  VoiceMicMode _micMode = VoiceMicMode.hold;

  VoiceSendMode get mode => _mode;
  VoiceMicMode get micMode => _micMode;

  Future<void> load() async {
    final stored = await _store?.read(key);
    _mode = stored == VoiceSendMode.auto.name
        ? VoiceSendMode.auto
        : VoiceSendMode.draft;
    final storedMic = await _store?.read(micKey);
    _micMode = storedMic == VoiceMicMode.toggle.name
        ? VoiceMicMode.toggle
        : VoiceMicMode.hold;
    notifyListeners();
  }

  Future<void> choose(VoiceSendMode mode) async {
    _mode = mode;
    notifyListeners();
    await _store?.write(key, mode.name);
  }

  Future<void> chooseMic(VoiceMicMode mode) async {
    _micMode = mode;
    notifyListeners();
    await _store?.write(micKey, mode.name);
  }
}
