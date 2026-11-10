import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_document_files.dart';
import 'package:excubia/core/api/chat_document_previews.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/chat_attachment_errors.dart';
import 'package:excubia/core/state/chat_document_opener.dart';
import 'package:excubia/src/rust/api/chat/attach_model.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/chat/document_preview_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_documents.dart';
import 'support/fake_preview_transport.dart';

ChatAttachment _sheet() => ChatAttachment(
  id: 'att_1',
  name: 'Informe.xlsx',
  mime: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  size: BigInt.from(3000),
  w: 0,
  h: 0,
);

/// The opener of the screen: the real download and save, with the phone's
/// dialogs faked, so nothing touches the disk.
class _Documents extends ChatDocumentOpener {
  _Documents(this.system, this.transport)
    : super(
        fetch: (_) async => const [7, 8, 9],
        files: system,
        root: () async => throw UnsupportedError('no se abre aquí'),
        previews: ChatDocumentPreviews(
          transport,
          conversation: () => 'cnv_1',
        ),
      );

  final FakeDocumentFiles system;
  final FakePreviewTransport transport;

  @override
  Future<DocumentOpen> open(ChatAttachment file) async {
    final outcome = await system.open('/cache/${file.name}', file.mime);
    return outcome == OpenOutcome.opened
        ? const DocumentOpened()
        : DocumentNoApp(Uint8List.fromList(const [7, 8, 9]));
  }
}

Widget _host(Widget child) => MaterialApp(
  // Without the ink splash: the test runner here lacks its shader.
  theme: buildTheme().copyWith(splashFactory: NoSplash.splashFactory),
  home: child,
);

void main() {
  setUpAll(() async => RustLib.init());

  late FakeDocumentFiles system;
  late FakePreviewTransport transport;
  late _Documents documents;

  setUp(() {
    system = FakeDocumentFiles();
    transport = FakePreviewTransport();
    documents = _Documents(system, transport);
  });

  Future<void> pumpScreen(WidgetTester tester) => tester.pumpWidget(
    _host(DocumentPreviewScreen(file: _sheet(), documents: documents)),
  );

  Finder button(String label) => find.widgetWithText(TextButton, label);

  testWidgets('muestra la página 1 con el nombre del archivo y su número', (tester) async {
    final semantics = tester.ensureSemantics();
    await pumpScreen(tester);
    await tester.pumpAndSettle();

    expect(find.text('Informe.xlsx'), findsOneWidget);
    expect(find.text('Página 1'), findsOneWidget);
    expect(find.byType(Image), findsOneWidget);
    expect(find.bySemanticsLabel('Página 1 de Informe.xlsx'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('Siguiente pide la página siguiente y Anterior vuelve a la que ya tiene', (
    tester,
  ) async {
    await pumpScreen(tester);
    await tester.pumpAndSettle();

    await tester.tap(button('Siguiente'));
    await tester.pumpAndSettle();
    expect(find.text('Página 2'), findsOneWidget);
    expect(transport.calls.last, endsWith('page=2'));

    await tester.tap(button('Anterior'));
    await tester.pumpAndSettle();
    expect(find.text('Página 1'), findsOneWidget);
    expect(transport.calls, hasLength(2), reason: 'la página 1 sale de la caché');
  });

  testWidgets('Anterior no baja de la página 1', (tester) async {
    await pumpScreen(tester);
    await tester.pumpAndSettle();

    expect(tester.widget<TextButton>(button('Anterior')).onPressed, isNull);
  });

  testWidgets('Siguiente se desactiva al llegar al fin del documento', (tester) async {
    transport.errorByPage[2] = HostError(
      404,
      '{"detail":{"code":"page_out_of_range","message":"x"}}',
    );
    await pumpScreen(tester);
    await tester.pumpAndSettle();

    await tester.tap(button('Siguiente'));
    await tester.pumpAndSettle();

    expect(find.text('Página 1'), findsOneWidget, reason: 'no existe la página 2');
    expect(tester.widget<TextButton>(button('Siguiente')).onPressed, isNull);
  });

  testWidgets('sin herramienta en el PC muestra el texto del host y sus botones', (
    tester,
  ) async {
    transport.errorByPage[1] = HostError(
      503,
      '{"detail":{"code":"tool_missing","message":"Falta GridCraft en este PC."}}',
    );
    await pumpScreen(tester);
    await tester.pumpAndSettle();

    expect(find.text('Falta GridCraft en este PC.'), findsOneWidget);
    expect(button('Abrir con otra app'), findsOneWidget);
    expect(button('Guardar'), findsOneWidget);
    expect(find.byType(Image), findsNothing);
  });

  testWidgets('un fallo de red dice que no se pudo cargar y Reintentar vuelve a pedirla', (
    tester,
  ) async {
    transport.errorByPage[1] = HostError(0, 'sin ruta al equipo');
    await pumpScreen(tester);
    await tester.pumpAndSettle();

    expect(find.text('No se pudo cargar la página.'), findsOneWidget);

    transport.errorByPage.clear();
    await tester.tap(button('Reintentar'));
    await tester.pumpAndSettle();
    expect(find.byType(Image), findsOneWidget);
    expect(find.text('Página 1'), findsOneWidget);
  });

  testWidgets('la barra abre el archivo con otra app y lo guarda entero', (tester) async {
    await pumpScreen(tester);
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Abrir con otra app'));
    await tester.pumpAndSettle();
    expect(system.openedPath, endsWith('/Informe.xlsx'));

    await tester.tap(find.byTooltip('Guardar'));
    await tester.pumpAndSettle();
    expect(system.saves.single.name, 'Informe.xlsx');
    expect(system.saves.single.bytes, [7, 8, 9]);
    expect(find.text(attachmentSavedText), findsOneWidget);
  });
}
