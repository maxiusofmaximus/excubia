import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/error/chat.dart';

import '../../core/state/chat_attachment_tray.dart';
import '../../core/state/voice_notes.dart';
import '../../core/state/voice_send_settings.dart';
import '../theme.dart';
import 'chat_attachments_tray_view.dart';
import 'voice_button.dart';

class ChatComposer extends StatelessWidget {
  const ChatComposer({
    super.key,
    required this.controller,
    required this.focus,
    required this.enabled,
    required this.sending,
    required this.offline,
    required this.busy,
    required this.canStop,
    required this.stopDenial,
    required this.attachments,
    required this.voice,
    required this.voiceSend,
    required this.onVoiceSend,
    required this.onAttach,
    required this.onSend,
    required this.onStop,
  });

  final TextEditingController controller;
  final FocusNode focus;
  final bool enabled;
  final bool sending;
  final bool offline;

  /// A turn is in flight: the main button stops it instead of sending.
  final bool busy;
  final bool canStop;

  /// Why the device may not stop: shown above the row while it holds.
  final ChatFailure? stopDenial;
  final ChatAttachmentTray attachments;
  final VoiceNotes voice;
  final VoiceSendSettings voiceSend;

  /// Sends a dictated note in auto mode; false when it did not go.
  final Future<bool> Function(String text) onVoiceSend;
  final VoidCallback onAttach;
  final VoidCallback onSend;
  final VoidCallback onStop;

  @override
  Widget build(BuildContext context) => VoiceInsertion(
    voice: voice,
    voiceSend: voiceSend,
    onVoiceSend: onVoiceSend,
    controller: controller,
    focus: focus,
    child: ListenableBuilder(
      listenable: Listenable.merge([attachments, controller, voice, voiceSend]),
      builder: (context, _) => _bar(context),
    ),
  );

  /// Nothing typed and nothing attached: the mic may take the send button's place.
  bool get _empty =>
      controller.text.trim().isEmpty && attachments.items.isEmpty;

  /// In auto mode the mic stays beside the stop button, so the next note can
  /// start while a turn runs.
  bool get _micShown =>
      _empty && (!busy || voiceSend.mode == VoiceSendMode.auto);

  Widget _bar(BuildContext context) {
    final c = context.colors;
    final denial = stopDenial;
    return Container(
      padding: const EdgeInsets.fromLTRB(
        Spacing.md,
        Spacing.sm,
        Spacing.md,
        Spacing.sm,
      ),
      decoration: BoxDecoration(
        color: c.surface,
        border: Border(top: BorderSide(color: c.outlineSoft)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (denial != null)
            Padding(
              padding: const EdgeInsets.only(bottom: Spacing.sm),
              child: Text(
                '${denial.message} ${denial.action ?? ''}'.trim(),
                style: context.text.small.copyWith(color: c.statusBlocked),
              ),
            ),
          VoiceStatusLine(voice: voice, micMode: voiceSend.micMode),
          if (attachments.items.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(bottom: Spacing.sm),
              child: ChatAttachmentTrayView(tray: attachments),
            ),
          Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              IconButton(
                tooltip: 'Adjuntar imagen',
                onPressed: enabled && attachments.canAdd ? onAttach : null,
                icon: const Icon(Icons.attach_file),
              ),
              Expanded(
                child: TextField(
                  controller: controller,
                  focusNode: focus,
                  enabled: enabled,
                  minLines: 1,
                  maxLines: 4,
                  textInputAction: TextInputAction.newline,
                  style: context.text.body,
                  decoration: InputDecoration(
                    hintText: offline
                        ? 'Reconecta para escribir a Alfred'
                        : 'Mensaje a Alfred',
                    isDense: true,
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: Spacing.md,
                      vertical: Spacing.md,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: Spacing.sm),
              if (busy || !_empty) _sendOrStop(context),
              if (_micShown)
                VoiceButton(
                  voice: voice,
                  enabled: enabled,
                  autoSend: voiceSend.mode == VoiceSendMode.auto,
                  micMode: voiceSend.micMode,
                ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _sendOrStop(BuildContext context) {
    final c = context.colors;
    return Semantics(
      button: true,
      label: busy ? 'Detener' : 'Enviar mensaje',
      child: IconButton.filled(
        onPressed: busy
            ? (canStop ? onStop : null)
            : (enabled && attachments.allReady && voice.phase == VoicePhase.idle
                  ? onSend
                  : null),
        tooltip: busy ? 'Detener' : 'Enviar',
        style: IconButton.styleFrom(
          minimumSize: const Size(48, 48),
          backgroundColor: c.accent,
          foregroundColor: c.onAccent,
          disabledBackgroundColor: c.raised,
          disabledForegroundColor: c.textMuted,
        ),
        icon: busy
            ? const Icon(Icons.stop)
            : sending
            ? SizedBox(
                width: 18,
                height: 18,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  color: c.onAccent,
                ),
              )
            : const Icon(Icons.arrow_upward),
      ),
    );
  }
}
