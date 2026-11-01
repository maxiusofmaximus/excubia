import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/state/voice_notes.dart';
import '../../core/state/voice_send_settings.dart';
import '../theme.dart';

/// Pushes the text the user is editing; a transcript lands at the cursor.
TextEditingValue insertAtCursor(TextEditingValue value, String text) {
  final selection = value.selection;
  final start = selection.isValid ? selection.start : value.text.length;
  final end = selection.isValid ? selection.end : value.text.length;
  return value.copyWith(
    text: value.text.replaceRange(start, end, text),
    selection: TextSelection.collapsed(offset: start + text.length),
    composing: TextRange.empty,
  );
}

/// The microphone that takes the send button's place while the composer is
/// empty. Hold mode: hold to record, release to transcribe, slide left to
/// cancel. Toggle mode: tap to record, tap again to transcribe.
class VoiceButton extends StatefulWidget {
  const VoiceButton({
    super.key,
    required this.voice,
    required this.enabled,
    required this.autoSend,
    required this.micMode,
  });

  final VoiceNotes voice;
  final bool enabled;

  /// The note goes to Alfred when it ends, so the tooltip says so.
  final bool autoSend;
  final VoiceMicMode micMode;

  @override
  State<VoiceButton> createState() => _VoiceButtonState();
}

class _VoiceButtonState extends State<VoiceButton> {
  static const _slideToCancel = 80.0;

  bool _held = false;
  bool _slid = false;

  Future<void> _pressStart() async {
    if (widget.voice.phase != VoicePhase.idle) return;
    _held = true;
    _slid = false;
    if (!widget.voice.explained) {
      if (await _explain() != true || !_held || !mounted) return;
    }
    await widget.voice.start();
  }

  void _pressEnd() {
    _held = false;
    final voice = widget.voice;
    unawaited(_slid ? voice.cancel() : voice.release());
  }

  void _pressCancel() {
    _held = false;
    unawaited(widget.voice.cancel());
  }

  Future<void> _tap() async {
    final voice = widget.voice;
    if (voice.phase == VoicePhase.idle && !voice.explained) {
      if (await _explain() != true || !mounted) return;
    }
    await voice.toggle();
  }

  /// The first press says what is recorded and where it goes, before the
  /// system asks for the microphone.
  Future<bool?> _explain() {
    widget.voice.markExplained();
    return showDialog<bool>(
      context: context,
      builder: (_) => _VoiceExplainer(micMode: widget.micMode),
    );
  }

  @override
  Widget build(BuildContext context) {
    final mic = _micButton(context);
    final recording = widget.voice.phase == VoicePhase.recording;
    if (widget.micMode == VoiceMicMode.hold || !recording) return mic;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        IconButton(
          tooltip: 'Cancelar grabación',
          onPressed: () => unawaited(widget.voice.cancel()),
          icon: const Icon(Icons.close),
        ),
        mic,
      ],
    );
  }

  Widget _micButton(BuildContext context) {
    final phase = widget.voice.phase;
    final busy =
        phase == VoicePhase.starting || phase == VoicePhase.transcribing;
    final face = _face(context, busy: busy);
    return Semantics(
      button: true,
      enabled: widget.enabled && !busy,
      label: 'Nota de voz',
      hint: _hint,
      child: Tooltip(
        message: _tooltip,
        triggerMode: TooltipTriggerMode.tap,
        child: switch (widget.micMode) {
          VoiceMicMode.hold => _holdGesture(face),
          VoiceMicMode.toggle => _tapGesture(face, busy: busy),
        },
      ),
    );
  }

  Widget _holdGesture(Widget child) => GestureDetector(
    onLongPressStart: widget.enabled && widget.voice.phase == VoicePhase.idle
        ? (_) => _pressStart()
        : null,
    onLongPressMoveUpdate: (details) =>
        _slid = details.offsetFromOrigin.dx < -_slideToCancel,
    onLongPressEnd: (_) => _pressEnd(),
    onLongPressCancel: _pressCancel,
    child: child,
  );

  Widget _tapGesture(Widget child, {required bool busy}) => GestureDetector(
    onTap: widget.enabled && !busy ? () => unawaited(_tap()) : null,
    child: child,
  );

  Widget _face(BuildContext context, {required bool busy}) {
    final c = context.colors;
    final stopIcon =
        widget.micMode == VoiceMicMode.toggle &&
        widget.voice.phase == VoicePhase.recording;
    return Container(
      width: 48,
      height: 48,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: widget.enabled ? c.accent : c.raised,
        shape: BoxShape.circle,
      ),
      child: busy
          ? SizedBox(
              width: 18,
              height: 18,
              child: CircularProgressIndicator(
                strokeWidth: 2,
                color: c.onAccent,
              ),
            )
          : Icon(stopIcon ? Icons.stop : Icons.mic, color: c.onAccent),
    );
  }

  String get _hint {
    final end = widget.autoSend ? 'enviar a Alfred' : 'transcribir';
    return switch (widget.micMode) {
      VoiceMicMode.hold =>
        'Mantén pulsado para grabar, suelta para $end, o desliza a la izquierda para cancelar',
      VoiceMicMode.toggle =>
        'Toca para grabar, toca de nuevo para $end, o pulsa la x para cancelar',
    };
  }

  String get _tooltip => switch (widget.micMode) {
    VoiceMicMode.hold =>
      widget.autoSend
          ? 'Mantén pulsado para grabar · envía al soltar'
          : 'Mantén pulsado para grabar',
    VoiceMicMode.toggle =>
      widget.autoSend
          ? 'Toca para grabar · envía al tocar de nuevo'
          : 'Toca para grabar',
  };
}

/// Listens for transcripts while the composer is on screen, so one that
/// arrives after the mic gave way to send still lands in the field.
class VoiceInsertion extends StatefulWidget {
  const VoiceInsertion({
    super.key,
    required this.voice,
    required this.voiceSend,
    required this.onVoiceSend,
    required this.controller,
    required this.focus,
    required this.child,
  });

  final VoiceNotes voice;
  final VoiceSendSettings voiceSend;
  final Future<bool> Function(String text) onVoiceSend;
  final TextEditingController controller;
  final FocusNode focus;
  final Widget child;

  @override
  State<VoiceInsertion> createState() => _VoiceInsertionState();
}

class _VoiceInsertionState extends State<VoiceInsertion>
    with WidgetsBindingObserver {
  StreamSubscription<String>? _transcripts;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _listen();
  }

  @override
  void didUpdateWidget(VoiceInsertion old) {
    super.didUpdateWidget(old);
    if (old.voice != widget.voice) {
      unawaited(_transcripts?.cancel());
      _listen();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    unawaited(_transcripts?.cancel());
    // Leaving the chat ends a note in progress without sending it.
    unawaited(widget.voice.cancel());
    super.dispose();
  }

  /// Going to the background ends a note in progress without sending it.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused) unawaited(widget.voice.cancel());
  }

  void _listen() => _transcripts = widget.voice.transcripts.listen(_insert);

  /// In draft mode a transcript is text to read and edit. In auto mode it is
  /// sent at once; if that does not go, it lands in the field as in draft mode.
  Future<void> _insert(String text) async {
    final sent =
        widget.voiceSend.mode == VoiceSendMode.auto &&
        await widget.onVoiceSend(text);
    if (sent || !mounted) return;
    widget.controller.value = insertAtCursor(widget.controller.value, text);
    widget.focus.requestFocus();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// What the recording shows above the row, or what the last attempt said.
class VoiceStatusLine extends StatelessWidget {
  const VoiceStatusLine({
    super.key,
    required this.voice,
    required this.micMode,
  });

  final VoiceNotes voice;
  final VoiceMicMode micMode;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final (text, color) = switch (voice.phase) {
      VoicePhase.recording => (
        switch (micMode) {
          VoiceMicMode.hold =>
            '● ${voice.elapsed.inSeconds} s  Suelta para transcribir · desliza para cancelar',
          VoiceMicMode.toggle =>
            '● ${voice.elapsed.inSeconds} s  Toca para parar · pulsa la x para cancelar',
        },
        c.statusBlocked,
      ),
      VoicePhase.starting => ('Preparando el micrófono…', c.textMuted),
      VoicePhase.transcribing => ('Transcribiendo…', c.textMuted),
      VoicePhase.idle => (voice.notice ?? '', c.statusBlocked),
    };
    if (text.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(bottom: Spacing.sm),
      child: Semantics(
        liveRegion: true,
        child: Text(text, style: context.text.small.copyWith(color: color)),
      ),
    );
  }
}

class _VoiceExplainer extends StatelessWidget {
  const _VoiceExplainer({required this.micMode});

  final VoiceMicMode micMode;

  @override
  Widget build(BuildContext context) => AlertDialog(
    content: Text(switch (micMode) {
      VoiceMicMode.hold =>
        'Mientras mantienes pulsado se graba hasta 60 segundos. El audio va '
            'solo a tu PC para transcribirlo y no se guarda.',
      VoiceMicMode.toggle =>
        'Al tocar el micrófono se graba hasta 60 segundos o hasta que lo '
            'vuelvas a tocar. El audio va solo a tu PC para transcribirlo y no '
            'se guarda.',
    }),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(false),
        child: const Text('Ahora no'),
      ),
      FilledButton(
        onPressed: () => Navigator.of(context).pop(true),
        child: const Text('Continuar'),
      ),
    ],
  );
}
