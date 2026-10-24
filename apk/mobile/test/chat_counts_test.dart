import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/fake_chat_transport.dart';

/// Serves the pinned list in two pages. A count taken from the first page
/// alone would stop at the page size, so both pages must be added.
class _PagedTransport extends FakeChatTransport {
  _PagedTransport(super.routes);

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    calls.add('$method $target');
    if (target.contains('pinned=true') && target.contains('cursor=cnv_2')) {
      return conversationsJson(conversations: [_row('cnv_2')]);
    }
    if (target.contains('pinned=true')) {
      return conversationsJson(
        conversations: [_row('cnv_1')],
        nextCursor: 'cnv_2',
      );
    }
    return super.call(method, target, body);
  }
}

Map<String, Object?> _row(String id) => {
  'id': id,
  'title': 'Fijado $id',
  'pinned': true,
  'archived': false,
  'last_seq': 1,
  'last_read_seq': 0,
  'unread': 0,
  'last_message_at': '2026-10-22T10:00:00.000Z',
  'created_at': '2026-10-22T10:00:00.000Z',
  'updated_at': '2026-10-22T10:00:00.000Z',
};

LiveTriage _triage() => LiveTriage(
  GatewayClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test('the pinned count adds every page, not only the first', () async {
    final transport = _PagedTransport({
      '/v1/chat/sections': sectionsJson(const []),
      '/v1/chat/conversations': conversationsJson(),
    });
    final controller = ChatController(transport, _triage());
    addTearDown(controller.dispose);

    await controller.loadHistory();

    expect(controller.pinnedCount, 2);
    expect(controller.archivedCount, 0);
    expect(
      transport.calls.where((call) => call.contains('cursor=cnv_2')),
      hasLength(1),
    );
  });
}
