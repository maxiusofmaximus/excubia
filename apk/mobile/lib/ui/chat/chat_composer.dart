import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/error/chat.dart';

import '../../core/state/chat_attachment_tray.dart';
import '../../core/state/voice_notes.dart';
import '../../core/state/voice_send_settings.dart';
import '../theme.dart';
import '../widgets/comic/comic_button.dart';
import '../widgets/comic/comic_slab.dart';
import 'chat_attachments_picker.dart';
import 'chat_attachments_tray_view.dart';
import 'voice_button.dart';

/// The hard shadow of the message field: the slab draws it past its box.
const double _fieldShadow = 3;

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

  /// Called with what the «+» menu picked: an image or a document.
  final ValueChanged<ChatAttachKind> onAttach;
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
      decoration: BoxDecoration(color: c.bg),
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
              PopupMenuButton<ChatAttachKind>(
                tooltip: 'Adjuntar',
                enabled: enabled && attachments.canAdd,
                icon: const Icon(Icons.attach_file),
                onSelected: onAttach,
                itemBuilder: (_) => const [
                  PopupMenuItem(
                    value: ChatAttachKind.image,
                    child: ListTile(
                      leading: Icon(Icons.image_outlined),
                      title: Text('Imagen'),
                    ),
                  ),
                  PopupMenuItem(
                    value: ChatAttachKind.file,
                    child: ListTile(
                      leading: Icon(Icons.description_outlined),
                      title: Text('Archivo'),
                    ),
                  ),
                ],
              ),
              // The input is a leaning slab; the send button is the hot one.
              Expanded(
                // The field is a touch target too: never under 48 dp.
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    minHeight: 48 + _fieldShadow,
                  ),
                  child: ComicSlab(
                    color: c.deep,
                    shadow: _fieldShadow,
                    padding: EdgeInsets.zero,
                    child: Semantics(
                      label: 'Escribe tu mensaje',
                      textField: true,
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
                          filled: false,
                          border: InputBorder.none,
                          enabledBorder: InputBorder.none,
                          focusedBorder: InputBorder.none,
                          contentPadding: const EdgeInsets.symmetric(
                            horizontal: Spacing.md,
                            vertical: Spacing.md,
                          ),
                        ),
                      ),
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
    return ComicButton(
      label: busy ? 'Detener' : 'Enviar mensaje',
      tooltip: busy ? 'Detener' : 'Enviar',
      square: true,
      onPressed: busy
          ? (canStop ? onStop : null)
          : (enabled && attachments.allReady && voice.phase == VoicePhase.idle
                ? onSend
                : null),
      icon: busy
          ? const Icon(Icons.stop)
          : sending
          ? SizedBox.square(
              dimension: 18,
              child: CircularProgressIndicator(
                strokeWidth: 2,
                color: context.colors.onHot,
              ),
            )
          : const Icon(Icons.arrow_upward),
    );
  }
}
