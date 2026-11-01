import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../api/chat_transport.dart';
import '../api/gateway_client.dart';
import 'voice_errors.dart';
import 'voice_recorder.dart';

enum VoicePhase { idle, starting, recording, transcribing }

/// One voice note from press to text. The text goes out on [transcripts] and
/// is only ever put in the composer; nothing here sends it.
class VoiceNotes extends ChangeNotifier {
  VoiceNotes(
    this._transport,
    this._recorder, {
    this.maxDuration = const Duration(seconds: 60),
    this.tick = const Duration(seconds: 1),
  });

  final ChatTransport _transport;
  final VoiceRecorder _recorder;

  /// The recording stops by itself here and is transcribed.
  final Duration maxDuration;
  final Duration tick;

  final _transcripts = StreamController<String>.broadcast();
  Stream<String> get transcripts => _transcripts.stream;

  VoicePhase _phase = VoicePhase.idle;
  VoicePhase get phase => _phase;

  Duration _elapsed = Duration.zero;
  Duration get elapsed => _elapsed;

  /// What the user reads after a refusal, an empty note or a denied permission.
  String? _notice;
  String? get notice => _notice;

  /// Set by the host's `Retry-After`: no note starts before it passes.
  DateTime _retryAt = DateTime.fromMillisecondsSinceEpoch(0);

  /// Whether the note is still wanted: its press is held down, or a toggle
  /// tap has not ended it.
  bool _wanted = false;
  Timer? _ticker;

  bool _explained = false;

  /// The first press says what is recorded and where it goes, before the
  /// system asks for the microphone.
  bool get explained => _explained;
  void markExplained() => _explained = true;

  /// Hold mode: the press starts the note and stays wanted until it ends.
  Future<void> start() async {
    if (_phase != VoicePhase.idle) return;
    _wanted = true;
    // The press ended while the recorder was starting: nothing to transcribe.
    if (await _begin() && !_wanted) await cancel();
  }

  /// Toggle mode: a tap starts the note and the next tap transcribes it.
  Future<void> toggle() {
    if (_phase == VoicePhase.recording) return release();
    return start();
  }

  /// The press ended, or the toggle tapped again: transcribes what was recorded.
  Future<void> release() async {
    _wanted = false;
    if (_phase != VoicePhase.recording) return;
    await _finish(transcribe: true);
  }

  /// Slid away, closed, or sent to the background: the note is thrown out
  /// without a request.
  Future<void> cancel() async {
    _wanted = false;
    if (_phase != VoicePhase.recording) return;
    await _finish(transcribe: false);
  }

  /// Opens the recorder; false when it could not, and the notice says why.
  Future<bool> _begin() async {
    _notice = null;
    if (DateTime.now().isBefore(_retryAt)) {
      _refuse(voiceBusyText);
      return false;
    }
    _setPhase(VoicePhase.starting);
    try {
      if (!await _recorder.hasPermission()) {
        _refuse(voicePermissionText);
        return false;
      }
      await _recorder.start();
    } on Exception {
      _refuse(voiceStartFailedText);
      return false;
    }
    _elapsed = Duration.zero;
    _ticker = Timer.periodic(tick, (_) => _onTick());
    _setPhase(VoicePhase.recording);
    return true;
  }

  void _onTick() {
    _elapsed += tick;
    if (_elapsed >= maxDuration) {
      unawaited(release());
      return;
    }
    notifyListeners();
  }

  Future<void> _finish({required bool transcribe}) async {
    _ticker?.cancel();
    // Claims the note before awaiting, so a second release or cancel is ignored.
    _phase = VoicePhase.transcribing;
    final path = await _recorder.stop();
    if (!transcribe) {
      await _discard(path);
      _setPhase(VoicePhase.idle);
      return;
    }
    _setPhase(VoicePhase.transcribing);
    try {
      final text = await _transcribe(path);
      if (text.isEmpty) {
        _notice = voiceEmptyText;
      } else {
        _transcripts.add(text);
      }
    } catch (error) {
      _notice = voiceErrorText(error);
      if (error is GatewayError && error.status == 429) {
        _retryAt = DateTime.now().add(Duration(seconds: error.retryAfter ?? 2));
      }
    } finally {
      await _discard(path);
      _setPhase(VoicePhase.idle);
    }
  }

  Future<String> _transcribe(String? path) async {
    if (path == null) return '';
    final bytes = await File(path).readAsBytes();
    final answer = await _transport.postBytes(
      '/v1/voice/transcribe',
      bytes,
      'audio/ogg',
    );
    final json = jsonDecode(answer) as Map<String, Object?>;
    return (json['text'] as String).trim();
  }

  /// The audio lives only in this temporary file, whatever the outcome.
  Future<void> _discard(String? path) async {
    if (path == null) return;
    try {
      await File(path).delete();
    } on FileSystemException {
      // Already gone; nothing else holds it.
    }
  }

  void _refuse(String text) {
    _wanted = false;
    _notice = text;
    _setPhase(VoicePhase.idle);
  }

  void _setPhase(VoicePhase phase) {
    _phase = phase;
    notifyListeners();
  }

  @override
  void dispose() {
    _ticker?.cancel();
    if (_phase == VoicePhase.recording) {
      unawaited(_recorder.stop().then(_discard));
    }
    unawaited(_recorder.dispose());
    unawaited(_transcripts.close());
    super.dispose();
  }
}
