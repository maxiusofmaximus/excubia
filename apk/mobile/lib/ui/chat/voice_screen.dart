import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/alerts/alert_platform.dart';
import '../../core/state/voice_errors.dart';
import '../../core/state/voice_notes.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import '../widgets/blinking_portrait.dart';

/// How long «Listo» stays before the screen closes and the chat takes over.
const _listoHold = Duration(milliseconds: 1500);

/// The ring's resting diameter; the ring grows from here.
const _orbCore = 168.0;

/// The face is 8 % smaller than the ring's base so its hair clears the gold ring.
const _faceCore = _orbCore * 0.92;

/// A full-screen voice note: an orb that follows the voice, what is happening
/// in words, and two buttons. The text reaches the chat through the composer's
/// own insertion (VoiceInsertion), so this screen never applies or sends it.
class VoiceScreen extends StatefulWidget {
  const VoiceScreen({super.key, required this.voice});

  final VoiceNotes voice;

  @override
  State<VoiceScreen> createState() => _VoiceScreenState();
}

class _VoiceScreenState extends State<VoiceScreen>
    with SingleTickerProviderStateMixin {
  /// The slow pulse: the ring's size when there is no level to follow.
  late final AnimationController _pulse = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 2400),
  );

  StreamSubscription<double>? _levels;
  StreamSubscription<String>? _transcripts;
  Timer? _leave;

  /// The transcript of the note that just ended; null until one arrives.
  String? _text;

  /// Smoothed level, 0 to 1: half the gap to each new sample.
  double _level = 0;
  bool _hasLevel = false;

  /// False until the first frame asks the recorder to start.
  bool _asked = false;
  bool _animate = true;

  @override
  void initState() {
    super.initState();
    widget.voice.addListener(_onVoice);
    _transcripts = widget.voice.transcripts.listen(_onText);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      setState(() => _asked = true);
      unawaited(widget.voice.start());
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _animate = !MediaQuery.disableAnimationsOf(context);
    if (_animate) {
      if (!_pulse.isAnimating) _pulse.repeat(reverse: true);
    } else {
      _pulse.stop();
    }
  }

  void _onVoice() {
    final voice = widget.voice;
    if (voice.phase == VoicePhase.recording) {
      _levels ??= voice.levels().listen(
        _onLevel,
        onError: (Object _) => _hasLevel = false,
      );
    } else {
      unawaited(_levels?.cancel());
      _levels = null;
      _hasLevel = false;
      _level = 0;
    }
    if (voice.phase == VoicePhase.starting ||
        voice.phase == VoicePhase.recording) {
      _text = null;
    }
    if (mounted) setState(() {});
  }

  void _onLevel(double target) {
    setState(() {
      _hasLevel = true;
      _level += (target - _level) * 0.5;
    });
  }

  void _onText(String text) {
    setState(() => _text = text);
    _leave?.cancel();
    _leave = Timer(_listoHold, () {
      if (mounted) unawaited(Navigator.of(context).maybePop());
    });
  }

  void _finish() => unawaited(widget.voice.release());

  void _retry() => unawaited(widget.voice.start());

  /// Discards the note; the dispose below does the same when the screen goes.
  void _cancel() {
    unawaited(widget.voice.cancel());
    unawaited(Navigator.of(context).maybePop());
  }

  @override
  void dispose() {
    widget.voice.removeListener(_onVoice);
    _leave?.cancel();
    unawaited(_transcripts?.cancel());
    unawaited(_levels?.cancel());
    _pulse.dispose();
    // Leaving while it records, or while it starts, ends the note unsent.
    unawaited(widget.voice.cancel());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final voice = widget.voice;
    final phase = voice.phase;
    final listo = phase == VoicePhase.idle && _text != null;
    // Nothing recorded and nothing transcribed: the note was refused, emptied,
    // or stopped by leaving the app. The text says which, and Reintentar starts over.
    final failure = phase == VoicePhase.idle && !listo && _asked
        ? (voice.notice ?? 'Se detuvo la grabación.')
        : null;

    final (String status, Color color) = switch (phase) {
      VoicePhase.recording => ('Te escucho', c.textPrimary),
      VoicePhase.transcribing => ('Transcribiendo…', c.textSecondary),
      VoicePhase.starting => ('Preparando el micrófono…', c.textSecondary),
      VoicePhase.idle when listo => ('Listo', c.textPrimary),
      VoicePhase.idle when failure == null => (
        'Preparando el micrófono…',
        c.textSecondary,
      ),
      VoicePhase.idle => (failure!, c.statusBlocked),
    };
    final recording = phase == VoicePhase.recording;
    final cancelable = phase != VoicePhase.transcribing && !listo;

    return Scaffold(
      backgroundColor: c.bg,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(Spacing.lg),
          child: Column(
            children: [
              Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  const AgentAvatar(
                    name: 'Alfred',
                    characterId: 'alfred',
                    size: 40,
                  ),
                  const SizedBox(width: Spacing.sm),
                  Text('Alfred', style: context.text.title),
                ],
              ),
              Expanded(
                child: Center(
                  child: SingleChildScrollView(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        _orb(c, listo, phase),
                        const SizedBox(height: Spacing.xl),
                        Semantics(
                          liveRegion: true,
                          child: Text(
                            status,
                            textAlign: TextAlign.center,
                            style:
                                (failure == null
                                        ? context.text.headline
                                        : context.text.title)
                                    .copyWith(color: color),
                          ),
                        ),
                        if (listo)
                          Padding(
                            padding: const EdgeInsets.only(top: Spacing.md),
                            child: Text(
                              _text!,
                              maxLines: 3,
                              overflow: TextOverflow.ellipsis,
                              textAlign: TextAlign.center,
                              style: context.text.bodySecondary,
                            ),
                          ),
                        if (failure != null) ...[
                          const SizedBox(height: Spacing.md),
                          TextButton.icon(
                            onPressed: _retry,
                            icon: const Icon(Icons.refresh),
                            label: const Text('Reintentar'),
                          ),
                          if (failure == voicePermissionText)
                            TextButton(
                              onPressed: () => unawaited(
                                const MethodChannelAlertPlatform()
                                    .openAppSettings(),
                              ),
                              child: const Text('Abrir ajustes'),
                            ),
                        ],
                      ],
                    ),
                  ),
                ),
              ),
              Text(
                'Habla con normalidad; pulsa Terminar cuando acabes.',
                textAlign: TextAlign.center,
                style: context.text.bodySecondary,
              ),
              const SizedBox(height: Spacing.lg),
              Row(
                children: [
                  Expanded(
                    child: OutlinedButton.icon(
                      onPressed: cancelable ? _cancel : null,
                      icon: const Icon(Icons.close),
                      label: const Text('Cancelar'),
                      style: OutlinedButton.styleFrom(
                        minimumSize: const Size.fromHeight(56),
                      ),
                    ),
                  ),
                  const SizedBox(width: Spacing.md),
                  Expanded(
                    child: FilledButton.icon(
                      onPressed: recording ? _finish : null,
                      icon: const Icon(Icons.check),
                      label: const Text('Terminar'),
                      style: FilledButton.styleFrom(
                        minimumSize: const Size.fromHeight(56),
                      ),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// The orb with its ring. The ring follows the level; without one it pulses
  /// slowly; with animations off it stays still at a middle size. Inside the
  /// ring is Alfred's face: neutral while he listens, thinking while the note
  /// is transcribed, done at «Listo». Without the picture, the plain orb.
  Widget _orb(HerdrColors c, bool listo, VoicePhase phase) {
    final face = listo
        ? 'done'
        : phase == VoicePhase.transcribing
        ? 'thinking'
        : 'neutral';
    return ExcludeSemantics(
      child: SizedBox(
        width: 232,
        height: 232,
        child: AnimatedBuilder(
          animation: _pulse,
          builder: (context, _) {
            final v = !_animate ? 0.3 : (_hasLevel ? _level : _pulse.value);
            final ring = _orbCore + 56 * v;
            return Stack(
              alignment: Alignment.center,
              children: [
                Container(
                  width: ring,
                  height: ring,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    border: Border.all(
                      color: c.accentText.withValues(alpha: 0.25 + 0.5 * v),
                      width: 2 + 4 * v,
                    ),
                  ),
                ),
                SizedBox.square(
                  dimension: _faceCore,
                  child: ClipOval(
                    child: BlinkingPortrait(
                      base: 'assets/agents_v3/portraits/alfred-$face-512.webp',
                      blink: 'assets/agents_v3/portraits/alfred-blink-512.webp',
                      enabled: face == 'neutral',
                      fallback: _plainCore(c),
                    ),
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
  }

  /// The orb's core when the picture is missing: a plain accent disc.
  Widget _plainCore(HerdrColors c) => Container(
    width: _faceCore,
    height: _faceCore,
    decoration: BoxDecoration(
      shape: BoxShape.circle,
      gradient: LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [c.accentSoft, c.accent],
      ),
    ),
  );
}
