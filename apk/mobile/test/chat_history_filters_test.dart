import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/fake_chat_transport.dart';
import 'support/signal_triage.dart';

LiveTriage _dummyTriage() => LiveTriage(
  GatewayClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

Map<String, String> _routes() => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(),
  '/v1/chat/conversations/counts': countsJson(all: 4, attention: 2, working: 1),
};

Future<void> _settle() =>
    Future<void>.delayed(const Duration(milliseconds: 10));

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test('each filter asks the host for its own parameter only', () async {
    final transport = FakeChatTransport(_routes());
    final controller = ChatController(transport, _dummyTriage());
    addTearDown(controller.dispose);

    controller.setFilter(HistoryFilter.attention);
    await _settle();
    expect(
      transport.calls.last,
      'GET /v1/chat/conversations?archived=false&attention=1&limit=100',
    );

    controller.setFilter(HistoryFilter.working);
    await _settle();
    expect(
      transport.calls.last,
      'GET /v1/chat/conversations?archived=false&working=1&limit=100',
    );

    controller.setFilter(HistoryFilter.all);
    await _settle();
    expect(
      transport.calls.last,
      'GET /v1/chat/conversations?archived=false&limit=100',
    );
  });

  test('the chip counts come on open and again after a chat event', () async {
    final routes = _routes();
    final triage = SignalTriage();
    addTearDown(triage.signals.close);
    final controller = ChatController(
      FakeChatTransport(routes),
      triage,
      refreshDelay: Duration.zero,
    );
    addTearDown(controller.dispose);
    await controller.start();

    await controller.loadHistory();
    expect(controller.countsSupported, isTrue);
    expect(controller.counts!.all.toInt(), 4);
    expect(controller.counts!.attention.toInt(), 2);
    expect(controller.counts!.working.toInt(), 1);

    routes['/v1/chat/conversations/counts'] = countsJson(
      all: 5,
      attention: 3,
      working: 0,
    );
    triage.signals.add(
      parseChatEvent(
        json:
            '{"event":"chat.conversation","revision":12,'
            '"conversation_id":"cnv_1","change":"updated"}',
      )!,
    );
    await _settle();

    expect(controller.counts!.attention.toInt(), 3);
    expect(controller.counts!.working.toInt(), 0);
  });

  test('a host without the counts route hides the new chips and drops their filter', () async {
    final transport = OldHostChatTransport(_routes());
    final controller = ChatController(transport, _dummyTriage());
    addTearDown(controller.dispose);

    controller.setFilter(HistoryFilter.attention);
    await controller.loadHistory();

    expect(controller.countsSupported, isFalse);
    expect(controller.counts, isNull);
    expect(controller.filter, HistoryFilter.all);
    expect(
      transport.calls.last,
      'GET /v1/chat/conversations?archived=false&limit=100',
    );
  });
}
