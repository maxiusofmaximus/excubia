import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/chat_attachment_tray.dart';
import 'package:excubia/core/state/voice_notes.dart';
import 'package:excubia/core/state/voice_send_settings.dart';
import 'package:excubia/ui/chat/chat_composer.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_chat_transport.dart';
import 'support/fake_voice_host.dart';
import 'support/fake_voice_recorder.dart';
import 'support/memory_store.dart';

void main() {
  testWidgets('the message field is 48 dp or more, its shadow not counted', (
    tester,
  ) async {
    final text = TextEditingController();
    addTearDown(text.dispose);
    final focus = FocusNode();
    addTearDown(focus.dispose);
    final voice = VoiceNotes(FakeVoiceHost(), FakeVoiceRecorder());
    addTearDown(voice.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: ChatComposer(
            controller: text,
            focus: focus,
            enabled: true,
            sending: false,
            offline: false,
            busy: false,
            canStop: false,
            stopDenial: null,
            attachments: ChatAttachmentTray(FakeChatTransport({})),
            voice: voice,
            voiceSend: VoiceSendSettings(MemoryKeyValueStore()),
            onVoiceSend: (_) async => false,
            onAttach: (_) {},
            onSend: () {},
            onStop: () {},
          ),
        ),
      ),
    );

    expect(
      tester.getSize(find.byType(TextField)).height,
      greaterThanOrEqualTo(48),
    );
    expect(find.bySemanticsLabel('Escribe tu mensaje'), findsOneWidget);
  });
}
