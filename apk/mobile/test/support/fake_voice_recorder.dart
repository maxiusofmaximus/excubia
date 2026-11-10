import 'dart:io';

import 'package:excubia/core/state/voice_recorder.dart';

/// Writes a real file on [stop], so the notes' own cleanup is what gets tested.
class FakeVoiceRecorder implements VoiceRecorder {
  FakeVoiceRecorder({this.allowed = true, this.audio = const [1, 2, 3]});

  final bool allowed;
  final List<int> audio;
  int starts = 0;
  bool disposed = false;

  /// The file the last [stop] returned; null before a stop.
  String? lastPath;

  @override
  Future<bool> hasPermission() async => allowed;

  @override
  Future<void> start() async => starts++;

  @override
  Future<String?> stop() async {
    final dir = Directory.systemTemp.createTempSync('voz-fake-');
    final file = File('${dir.path}/note.ogg')..writeAsBytesSync(audio);
    lastPath = file.path;
    return file.path;
  }

  @override
  Future<void> dispose() async => disposed = true;
}
