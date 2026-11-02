import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/state/chat_attachment_images.dart';
import 'package:herdr_notify/core/state/chat_attachment_tray.dart';
import 'package:herdr_notify/core/state/voice_notes.dart';
import 'package:herdr_notify/core/state/voice_send_settings.dart';
import 'package:herdr_notify/src/rust/api/chat/attach_model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/chat_attachments_view.dart';
import 'package:herdr_notify/ui/chat/chat_composer.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/fake_attachments.dart';
import 'support/fake_chat_transport.dart';
import 'support/fake_voice_recorder.dart';

ChatAttachment _file() => ChatAttachment(
  id: 'att_1',
  name: 'captura.png',
  mime: 'image/webp',
  size: BigInt.from(12),
  w: 2,
  h: 3,
);

Widget _host(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(body: child),
);

/// A tray whose drafts the test sets by hand. The real upload needs the Rust
/// calls, which do not finish inside the widget test's fake clock; the upload
/// itself is covered in chat_attachments_test.dart.
class _HeldTray extends ChatAttachmentTray {
  _HeldTray(super.transport) : super(pause: noPause);

  List<AttachmentDraft> _held = <AttachmentDraft>[];

  @override
  List<AttachmentDraft> get items => List.unmodifiable(_held);

  @override
  bool get allReady => _held.every((d) => d.status == DraftStatus.ready);

  void hold(AttachmentDraft draft) {
    _held = [draft];
    notifyListeners();
  }
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('Enviar stays off until every picked file is ready', (
    tester,
  ) async {
    final tray = _HeldTray(UploadTransport(chatRoutes()));
    addTearDown(tray.dispose);
    final text = TextEditingController();
    addTearDown(text.dispose);
    final focus = FocusNode();
    addTearDown(focus.dispose);
    final voice = VoiceNotes(FakeChatTransport({}), FakeVoiceRecorder());
    addTearDown(voice.dispose);
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
          onAttach: () {},
          onSend: () {},
          onStop: () {},
        ),
      ),
    );

    IconButton send() => tester.widget<IconButton>(
      find.widgetWithIcon(IconButton, Icons.arrow_upward),
    );
    final picked = AttachmentDraft('cnv_1', png());
    tray.hold(picked);
    await tester.pump();
    expect(send().onPressed, isNull);

    picked.status = DraftStatus.ready;
    tray.hold(picked);
    await tester.pump();
    expect(send().onPressed, isNotNull);
  });

  testWidgets('a thumbnail spins while loading, then marks a failed load', (
    tester,
  ) async {
    final pending = Completer<List<int>>();
    final images = ChatAttachmentImages((_) => pending.future);
    await tester.pumpWidget(
      _host(ChatAttachmentStrip(attachments: [_file()], images: images)),
    );
    expect(find.byType(CircularProgressIndicator), findsOneWidget);

    pending.completeError(const HostError(0, 'sin conexión'));
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.broken_image_outlined), findsOneWidget);
  });

  testWidgets('a thumbnail with its bytes shows the picture, not a spinner', (
    tester,
  ) async {
    final images = ChatAttachmentImages((_) async => const [1, 2, 3]);
    await tester.pumpWidget(
      _host(ChatAttachmentStrip(attachments: [_file()], images: images)),
    );
    await tester.pump();

    expect(find.byType(CircularProgressIndicator), findsNothing);
    expect(find.byType(Image), findsOneWidget);
  });
}
