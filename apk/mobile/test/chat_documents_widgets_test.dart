import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_document_files.dart';
import 'package:excubia/core/api/chat_document_previews.dart';
import 'package:excubia/core/state/chat_attachment_errors.dart';
import 'package:excubia/core/state/chat_attachment_images.dart';
import 'package:excubia/core/state/chat_document_opener.dart';
import 'package:excubia/src/rust/api/chat/attach_model.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/chat/chat_attachments_view.dart';
import 'package:excubia/ui/chat/document_preview_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_documents.dart';
import 'support/fake_preview_transport.dart';

ChatAttachment _doc() => ChatAttachment(
  id: 'att_doc',
  name: 'Informe final trimestral.xlsx',
  mime: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  size: BigInt.from(3565158),
  w: 0,
  h: 0,
);

/// A presentation the preview does not take and the app does not read as text:
/// it opens with the system app.
ChatAttachment _slides() => ChatAttachment(
  id: 'att_slides',
  name: 'Informe final trimestral.pptx',
  mime: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  size: BigInt.from(3565158),
  w: 0,
  h: 0,
);

Widget _host(Widget child) => MaterialApp(
  // Without the ink splash: the test runner here lacks its shader.
  theme: buildTheme().copyWith(splashFactory: NoSplash.splashFactory),
  home: Scaffold(body: child),
);

/// An opener that never touches the disk. The card's tap reaches it, and its
/// download and write are covered by chat_documents_test.dart.
class _FakeOpener extends ChatDocumentOpener {
  _FakeOpener(this.system, FakePreviewTransport transport)
    : super(
        fetch: (_) async => const [1, 2, 3],
        files: system,
        root: () async => throw UnsupportedError('la caché no se usa aquí'),
        previews: ChatDocumentPreviews(transport, conversation: () => 'cnv_1'),
      );

  final FakeDocumentFiles system;
  int opens = 0;
  bool failDownload = false;
  Completer<void>? downloading;

  @override
  Future<DocumentOpen> open(ChatAttachment file) async {
    opens++;
    if (failDownload) throw StateError('sin red');
    await downloading?.future;
    final outcome = await system.open('/cache/${file.name}', file.mime);
    return outcome == OpenOutcome.opened
        ? const DocumentOpened()
        : DocumentNoApp(Uint8List.fromList(const [1, 2, 3]));
  }
}

void main() {
  setUpAll(() async => RustLib.init());

  late FakeDocumentFiles files;
  late FakePreviewTransport transport;
  late _FakeOpener opener;
  var fetches = 0;

  setUp(() {
    files = FakeDocumentFiles();
    transport = FakePreviewTransport();
    opener = _FakeOpener(files, transport);
    fetches = 0;
  });

  ChatAttachmentImages images() => ChatAttachmentImages((_) {
    fetches++;
    return Completer<List<int>>().future;
  });

  Future<void> pumpStrip(WidgetTester tester, {ChatAttachment? file}) =>
      tester.pumpWidget(
        _host(
          ChatAttachmentStrip(
            attachments: [file ?? _doc()],
            images: images(),
            documents: opener,
          ),
        ),
      );

  testWidgets(
    'una tarjeta de documento muestra icono, nombre y tamaño, sin miniatura',
    (tester) async {
      await pumpStrip(tester);

      expect(find.text('Informe final trimestral.xlsx'), findsOneWidget);
      expect(find.text('3,4 MB'), findsOneWidget);
      expect(find.byIcon(Icons.table_chart_outlined), findsOneWidget);
      expect(find.byType(Image), findsNothing);
      expect(
        fetches,
        0,
        reason: 'un documento no pide su contenido como imagen',
      );
    },
  );

  testWidgets(
    'tocar una presentación abre el archivo con la app del sistema, sin vista previa',
    (tester) async {
      await pumpStrip(tester, file: _slides());

      await tester.tap(find.text('Informe final trimestral.pptx'));
      await tester.pumpAndSettle();

      expect(opener.opens, 1);
      expect(files.openedPath, endsWith('/Informe final trimestral.pptx'));
      expect(
        files.openedMime,
        'application/vnd.openxmlformats-officedocument.presentationml.presentation',
      );
      expect(find.byType(DocumentPreviewScreen), findsNothing);
      expect(transport.calls, isEmpty);
    },
  );

  testWidgets('tocar un xlsx abre su vista previa en vez de abrirlo', (
    tester,
  ) async {
    await pumpStrip(tester);

    await tester.tap(find.text('Informe final trimestral.xlsx'));
    await tester.pumpAndSettle();

    expect(find.byType(DocumentPreviewScreen), findsOneWidget);
    expect(opener.opens, 0);
    expect(transport.calls.single, endsWith('/preview?page=1'));
  });

  testWidgets('mientras se descarga muestra progreso', (tester) async {
    opener.downloading = Completer<void>();
    await pumpStrip(tester, file: _slides());

    await tester.tap(find.text('Informe final trimestral.pptx'));
    await tester.pump();
    expect(find.byType(CircularProgressIndicator), findsOneWidget);

    opener.downloading!.complete();
    await tester.pumpAndSettle();
    expect(find.byType(CircularProgressIndicator), findsNothing);
  });

  testWidgets('sin app que lo abra, ofrece guardarlo en Descargas', (
    tester,
  ) async {
    files.outcome = OpenOutcome.noApp;
    await pumpStrip(tester, file: _slides());

    await tester.tap(find.text('Informe final trimestral.pptx'));
    await tester.pumpAndSettle();
    expect(find.text(attachmentNoAppText), findsOneWidget);

    await tester.tap(find.text(attachmentSaveLabel));
    await tester.pumpAndSettle();
    expect(files.saves.single.name, 'Informe final trimestral.pptx');
    expect(files.saves.single.bytes, [1, 2, 3]);
  });

  testWidgets('si la descarga falla, lo dice sin cerrar la conversación', (
    tester,
  ) async {
    opener.failDownload = true;
    await pumpStrip(tester, file: _slides());

    await tester.tap(find.text('Informe final trimestral.pptx'));
    await tester.pumpAndSettle();
    expect(find.text(attachmentOpenFailedText), findsOneWidget);
  });
}
