import 'package:path_provider/path_provider.dart';
import 'package:record/record.dart';

/// Records one voice note to a file. The caller owns the file [stop] returns.
abstract interface class VoiceRecorder {
  Future<bool> hasPermission();

  Future<void> start();

  /// Ends the recording; null when nothing was recorded.
  Future<String?> stop();

  Future<void> dispose();
}

/// Ogg Opus at the rate the host's speech engine reads: mono 16 kHz, 24 kbps.
/// Android writes Ogg for Opus from API 29 on.
class RecordVoiceRecorder implements VoiceRecorder {
  /// Made on first use: its constructor already talks to the platform.
  AudioRecorder? _recorder;

  AudioRecorder get _audio => _recorder ??= AudioRecorder();

  static const _config = RecordConfig(
    encoder: AudioEncoder.opus,
    sampleRate: 16000,
    numChannels: 1,
    bitRate: 24000,
  );

  @override
  Future<bool> hasPermission() => _audio.hasPermission();

  @override
  Future<void> start() async {
    final dir = await getTemporaryDirectory();
    await _audio.start(
      _config,
      path: '${dir.path}/voz-${DateTime.now().microsecondsSinceEpoch}.ogg',
    );
  }

  @override
  Future<String?> stop() => _audio.stop();

  @override
  Future<void> dispose() async {
    await _recorder?.dispose();
  }
}
