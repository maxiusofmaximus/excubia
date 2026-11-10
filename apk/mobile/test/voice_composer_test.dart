import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/chat_attachment_tray.dart';
import 'package:excubia/core/state/voice_notes.dart';
import 'package:excubia/core/state/voice_send_settings.dart';
import 'package:excubia/ui/chat/chat_composer.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_attachments.dart';
import 'support/fake_chat_transport.dart';
import 'support/fake_voice_host.dart';
import 'support/fake_voice_recorder.dart';

Widget _host(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(body: child),
);

/// A tray that already holds one file, so the send button has something to send.
class _WithFileTray extends ChatAttachmentTray {
  _WithFileTray(super.transport);

  @override
  List<AttachmentDraft> get items => [AttachmentDraft('cnv_1', png())];

  @override
  bool get allReady => true;
}

void main() {
  testWidgets(
    'el micrófono sustituye al enviar solo con campo vacío y sin adjuntos',
    (tester) async {
      final text = TextEditingController();
      addTearDown(text.dispose);
      final focus = FocusNode();
      addTearDown(focus.dispose);
      final tray = ChatAttachmentTray(FakeChatTransport({}));
      addTearDown(tray.dispose);
      final voice = VoiceNotes(FakeChatTransport({}), FakeVoiceRecorder());
      addTearDown(voice.dispose);

      Widget composer({bool busy = false, ChatAttachmentTray? attached}) =>
          ChatComposer(
            controller: text,
            focus: focus,
            enabled: true,
            sending: false,
            offline: false,
            busy: busy,
            canStop: busy,
            stopDenial: null,
            attachments: attached ?? tray,
            voice: voice,
            voiceSend: VoiceSendSettings(),
            onVoiceSend: (_) async => false,
            onAttach: (_) {},
            onSend: () {},
            onStop: () {},
          );

      await tester.pumpWidget(_host(composer()));
      expect(find.byIcon(Icons.mic), findsOneWidget);
      expect(find.byIcon(Icons.arrow_upward), findsNothing);

      text.text = 'hola';
      await tester.pump();
      expect(find.byIcon(Icons.mic), findsNothing);
      expect(find.byIcon(Icons.arrow_upward), findsOneWidget);

      text.clear();
      await tester.pump();
      expect(find.byIcon(Icons.mic), findsOneWidget);

      await tester.pumpWidget(_host(composer(busy: true)));
      expect(find.byIcon(Icons.mic), findsNothing);
      expect(find.byIcon(Icons.stop), findsOneWidget);

      await tester.pumpWidget(
        _host(composer(attached: _WithFileTray(FakeChatTransport({})))),
      );
      expect(find.byIcon(Icons.mic), findsNothing);
      expect(find.byIcon(Icons.arrow_upward), findsOneWidget);
    },
  );

  testWidgets(
    'el texto dictado entra en el cursor, sin enviarse, y devuelve el foco',
    (tester) async {
      final text = TextEditingController(text: 'hola  fin')
        ..selection = const TextSelection.collapsed(offset: 5);
      addTearDown(text.dispose);
      final focus = FocusNode();
      addTearDown(focus.dispose);
      final tray = ChatAttachmentTray(FakeChatTransport({}));
      addTearDown(tray.dispose);
      final host = FakeVoiceHost(answer: '{"text":"querido","ms":1}');
      final voice = VoiceNotes(host, FakeVoiceRecorder());
      addTearDown(voice.dispose);
      var sent = false;

      await tester.pumpWidget(
        _host(
          ChatComposer(
            controller: text,
            focus: focus,
            enabled: true,
            sending: false,
            offline: false,
            busy: false,
            canStop: false,
            stopDenial: null,
            attachments: tray,
            voice: voice,
            voiceSend: VoiceSendSettings(),
            onVoiceSend: (_) async => false,
            onAttach: (_) {},
            onSend: () => sent = true,
            onStop: () {},
          ),
        ),
      );
      await tester.runAsync(() async {
        await voice.start();
        await voice.release();
        await Future<void>.delayed(Duration.zero);
      });
      await tester.pump();

      expect(text.text, 'hola querido fin');
      expect(text.selection, const TextSelection.collapsed(offset: 12));
      expect(focus.hasFocus, isTrue);
      expect(sent, isFalse);
      expect(host.calls, isEmpty);
    },
  );
}
