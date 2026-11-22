import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/live_triage.dart';
import 'package:excubia/src/rust/api/error.dart';
import 'package:excubia/ui/doc_tools/doc_tools_section.dart';
import 'package:excubia/ui/widgets/surface_card.dart';

import '../support/widget_mount.dart';

DocToolsLoad _threeTools() => (
  tools: const [
    DocTool(app: 'gridcraft', name: 'GridCraft', available: true),
    DocTool(app: 'wordcraft', name: 'WordCraft', available: false),
    DocTool(app: 'pdfcraft', name: 'PdfCraft', available: true),
  ],
  error: null,
  unsupported: false,
);

Future<void> _mount(WidgetTester tester, DocToolsLoader load) => mountWidget(
  tester,
  Scaffold(
    body: ListView(
      padding: const EdgeInsets.all(16),
      children: [DocToolsSection(load: load)],
    ),
  ),
);

void main() {
  testWidgets('lists the three tools with their state and the footer', (
    tester,
  ) async {
    await _mount(tester, () async => _threeTools());

    expect(find.text('HERRAMIENTAS DE DOCUMENTOS'), findsOneWidget);
    expect(find.text('GridCraft'), findsOneWidget);
    expect(find.text('WordCraft'), findsOneWidget);
    expect(find.text('PdfCraft'), findsOneWidget);
    expect(find.text('Instalada'), findsNWidgets(2));
    expect(find.text('Falta en este PC'), findsOneWidget);
    expect(
      find.text('Sirven para abrir y crear Excel, Word y PDF en el chat.'),
      findsOneWidget,
    );
  });

  testWidgets('shows the host error', (tester) async {
    await _mount(
      tester,
      () async => (
        tools: null,
        error: const ErrorInfo(
          message: 'No se pudo leer las herramientas.',
          kind: FailureKind.retry,
        ),
        unsupported: false,
      ),
    );

    expect(find.text('HERRAMIENTAS DE DOCUMENTOS'), findsOneWidget);
    expect(find.text('No se pudo leer las herramientas.'), findsOneWidget);
  });

  testWidgets('hides the section when the host does not have the route', (
    tester,
  ) async {
    await _mount(
      tester,
      () async => (tools: null, error: null, unsupported: true),
    );

    expect(find.text('HERRAMIENTAS DE DOCUMENTOS'), findsNothing);
    expect(find.byType(SurfaceCard), findsNothing);
  });

  testWidgets('reads before the host answers', (tester) async {
    final pending = Completer<DocToolsLoad>();
    await _mount(tester, () => pending.future);

    expect(find.text('HERRAMIENTAS DE DOCUMENTOS'), findsOneWidget);
    expect(find.text('Leyendo…'), findsOneWidget);

    pending.complete((
      tools: const <DocTool>[],
      error: null,
      unsupported: false,
    ));
    await tester.pump();
  });

  test('parses the host body and refuses another shape', () {
    final tools = parseDocTools(
      json:
          '{"ok":true,"tools":['
          '{"app":"gridcraft","name":"GridCraft","available":true,'
          '"tested_version":"0.16.1"}]}',
    );

    expect(tools, hasLength(1));
    expect(tools!.single.name, 'GridCraft');
    expect(tools.single.available, isTrue);
    expect(parseDocTools(json: '{"ok":true,"tools":[{"app":"x"}]}'), isNull);
    expect(parseDocTools(json: 'not json'), isNull);
  });
}
