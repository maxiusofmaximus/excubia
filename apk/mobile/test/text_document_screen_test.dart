import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_document_previews.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/plan_runner.dart';
import 'package:excubia/core/state/chat_attachment_images.dart';
import 'package:excubia/core/state/chat_document_opener.dart';
import 'package:excubia/src/rust/api/chat/attach_model.dart';
import 'package:excubia/src/rust/api/chat/attach_plan.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/chat/chat_attachments_view.dart';
import 'package:excubia/ui/chat/text_document_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_documents.dart';
import 'support/recording_transport.dart';

const _conversation = 'cnv_1';

ChatAttachment _note() => ChatAttachment(
  id: 'att_note',
  name: 'nota.txt',
  mime: 'text/plain',
  size: BigInt.from(40),
  w: 0,
  h: 0,
);

Widget _host(Widget child) => MaterialApp(
  // Without the ink splash: the test runner here lacks its shader.
  theme: buildTheme().copyWith(splashFactory: NoSplash.splashFactory),
  home: Scaffold(body: child),
);

/// An opener whose download goes through the transport, as the app's does.
ChatDocumentOpener _opener(
  RecordingTransport transport,
  FakeDocumentFiles files,
) => ChatDocumentOpener(
  fetch: (id) => runPlanBytes(
    transport,
    planAttachmentContent(conversationId: _conversation, attachmentId: id),
  ),
  files: files,
  root: () async => throw UnsupportedError('no se abre aquí'),
  previews: ChatDocumentPreviews(transport, conversation: () => _conversation),
);

void main() {
  setUpAll(() async => RustLib.init());

  late RecordingTransport transport;
  late FakeDocumentFiles files;

  setUp(() {
    transport = RecordingTransport();
    files = FakeDocumentFiles();
  });

  Future<void> pumpNote(WidgetTester tester) => tester.pumpWidget(
    _host(
      ChatAttachmentStrip(
        attachments: [_note()],
        images: ChatAttachmentImages((_) => Completer<List<int>>().future),
        documents: _opener(transport, files),
      ),
    ),
  );

  testWidgets('tocar un txt lo muestra como texto, sin abrirlo con otra app', (
    tester,
  ) async {
    transport.bytes = utf8.encode('Primera línea\nSegunda línea\n');
    await pumpNote(tester);

    await tester.tap(find.text('nota.txt'));
    await tester.pumpAndSettle();

    expect(find.byType(TextDocumentScreen), findsOneWidget);
    expect(find.text('Primera línea'), findsOneWidget);
    expect(find.text('Segunda línea'), findsOneWidget);
    expect(transport.targets.single, endsWith('/attachments/att_note/content'));
    expect(files.openedPath, isNull);
  });

  testWidgets(
    'si el archivo no se descarga, lo dice y deja reintentar o abrirlo con otra app',
    (tester) async {
      transport.failure = HostError(0, 'sin ruta al equipo');
      await pumpNote(tester);

      await tester.tap(find.text('nota.txt'));
      await tester.pumpAndSettle();

      expect(find.text('No se pudo cargar el archivo.'), findsOneWidget);
      expect(
        find.widgetWithText(TextButton, 'Abrir con otra app'),
        findsOneWidget,
      );

      transport.failure = null;
      transport.bytes = utf8.encode('Ya llegó\n');
      await tester.tap(find.widgetWithText(TextButton, 'Reintentar'));
      await tester.pumpAndSettle();

      expect(find.text('Ya llegó'), findsOneWidget);
      expect(find.text('No se pudo cargar el archivo.'), findsNothing);
    },
  );
}
