import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_document_previews.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/src/rust/api/chat/attach_model.dart';
import 'package:excubia/src/rust/frb_generated.dart';

import 'support/fake_preview_transport.dart';

const _doc = 'att_1';
const _conversation = 'cnv_1';
const _unavailable = '{"detail":{"code":"preview_unavailable","message":"x"}}';

void main() {
  setUpAll(() async => RustLib.init());

  late FakePreviewTransport transport;
  late ChatDocumentPreviews previews;

  setUp(() {
    transport = FakePreviewTransport();
    previews = ChatDocumentPreviews(transport, conversation: () => _conversation);
  });

  Future<PreviewRefusal> refusal(Future<Uint8List> pending) async {
    try {
      await pending;
    } on PreviewRefusal catch (refused) {
      return refused;
    }
    fail('la página debía fallar');
  }

  test('pide la página en la ruta del adjunto, con su número', () async {
    await previews.page(_doc, 2);

    expect(
      transport.calls.single,
      'GET /v1/chat/conversations/cnv_1/attachments/att_1/preview?page=2',
    );
  });

  test('una página ya pedida sale de la caché y no vuelve al host', () async {
    final first = await previews.page(_doc, 1);
    final again = await previews.page(_doc, 1);

    expect(transport.calls, hasLength(1));
    expect(again, first);
    await previews.page(_doc, 2);
    expect(transport.calls, hasLength(2), reason: 'otra página sí se pide');
  });

  test('los bytes que no son un PNG se descartan y la página no queda guardada', () async {
    transport.bytesByPage[1] = const [1, 2, 3];

    final refused = await refusal(previews.page(_doc, 1));
    expect(refused.failure, PreviewFailure.other);

    transport.bytesByPage.remove(1);
    expect(await previews.page(_doc, 1), equals(kPreviewPng));
    expect(transport.calls, hasLength(2));
  });

  test('un 404 preview_unavailable es Unavailable', () async {
    transport.errorByPage[1] = HostError(404, _unavailable);

    expect((await refusal(previews.page(_doc, 1))).failure, PreviewFailure.unavailable);
  });

  test('una página fuera del documento es OutOfRange', () async {
    transport.errorByPage[9] = HostError(
      404,
      '{"detail":{"code":"page_out_of_range","message":"x"}}',
    );

    expect((await refusal(previews.page(_doc, 9))).failure, PreviewFailure.outOfRange);
  });

  test('un 503 tool_missing lleva el texto del host', () async {
    transport.errorByPage[1] = HostError(
      503,
      '{"detail":{"code":"tool_missing","message":"Falta GridCraft en este PC."}}',
    );

    final refused = await refusal(previews.page(_doc, 1));
    expect(refused.failure, PreviewFailure.toolMissing);
    expect(refused.message, 'Falta GridCraft en este PC.');
  });

  test('un fallo de red o un 502 es Other, y se vuelve a pedir', () async {
    transport.errorByPage[1] = HostError(0, 'sin ruta al equipo');
    expect((await refusal(previews.page(_doc, 1))).failure, PreviewFailure.other);

    transport.errorByPage[2] = HostError(502, '{"detail":{"code":"preview_failed","message":"x"}}');
    expect((await refusal(previews.page(_doc, 2))).failure, PreviewFailure.other);

    transport.errorByPage.clear();
    await previews.page(_doc, 1);
    expect(transport.calls.where((call) => call.endsWith('page=1')), hasLength(2));
  });

  test('sin conversación abierta no se pide nada', () async {
    final closed = ChatDocumentPreviews(transport, conversation: () => null);

    expect((await refusal(closed.page(_doc, 1))).failure, PreviewFailure.other);
    expect(transport.calls, isEmpty);
  });
}
