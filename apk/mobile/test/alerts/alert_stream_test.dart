import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/alerts/alert_stream.dart';
import 'package:excubia/core/api/connection_failure.dart';
import 'package:excubia/core/api/host_client.dart';

import 'alert_fakes.dart';

const String _rulesState = '{"alert_rules":{"mode":"all","enabled_count":1}}';

void main() {
  late FakeAlertPlatform platform;

  setUp(() => platform = FakeAlertPlatform());

  // The real wait would outlast `until`, so reconnects are immediate unless a test asks for the wait.
  AlertStream streamWith(
    FakeAlertSource source, {
    Duration heartbeat = const Duration(seconds: 60),
    Future<void> Function(Duration)? delay,
  }) => AlertStream(
    source: source,
    platform: platform,
    newDecoder: scriptedDecode,
    heartbeat: heartbeat,
    delay: delay ?? (_) async {},
  );

  test('an approval alerts on the approvals channel with the state text', () async {
    final source = FakeAlertSource([Stream.fromIterable(['x'])])
      ..stateJson = '{"chat":{"pending_user_approvals":1},"counts":{}}';
    unawaited(streamWith(source).run());

    await until(() => platform.notices.isNotEmpty);
    expect(platform.notices.single.channel, 'agents');
    expect(platform.notices.single.title, 'Excubia');
    expect(platform.notices.single.body, 'Una aprobación espera tu decisión.');
  });

  test('a finished task on an older host alerts on the review channel with the generic text', () async {
    final source = FakeAlertSource([Stream.fromIterable(['y'])]);
    unawaited(streamWith(source).run());

    await until(() => platform.notices.isNotEmpty);
    expect(platform.notices.single.channel, 'agents-review');
    expect(platform.notices.single.body, 'Hay novedades de tus agentes.');
  });

  test('an approval answered before the state read stays silent', () async {
    final source = FakeAlertSource([Stream.fromIterable(['x'])])
      ..stateJson = '{"chat":{"pending_user_approvals":0},"counts":{}}';
    unawaited(streamWith(source).run());

    await until(() => platform.records.contains('sin aviso: no hay aprobaciones pendientes'));
    expect(platform.notices, isEmpty);
  });

  test('an alert with the app in front is reported as left out', () async {
    platform.notifyShown = false;
    final source = FakeAlertSource([Stream.fromIterable(['y'])]);
    unawaited(streamWith(source).run());

    await until(() => platform.records.contains('omitido: la app está en primer plano'));
    expect(platform.notices, hasLength(1));
  });

  test('a state read that fails is logged and the stream carries on', () async {
    final source = FakeAlertSource([Stream.fromIterable(['x', 'y'])])
      ..stateError = const SocketException('sin red');
    unawaited(streamWith(source).run());

    await until(() => platform.notices.isNotEmpty);
    expect(platform.records, contains('sin estado: sin red'));
    expect(platform.notices.single.channel, 'agents-review');
  });

  test('a blocked alert says a generic text on the approvals channel, with no agent name', () async {
    final source = FakeAlertSource([Stream.fromIterable(['b'])]);
    unawaited(streamWith(source).run());

    await until(() => platform.notices.isNotEmpty);
    expect(platform.notices.single.channel, 'agents');
    expect(platform.notices.single.title, 'Excubia');
    expect(platform.notices.single.body, 'Un agente necesita tu atención');
  });

  test('a done and a reminder alert each say their own text', () async {
    final source = FakeAlertSource([Stream.fromIterable(['d', 'r'])]);
    unawaited(streamWith(source).run());

    await until(() => platform.notices.length == 2);
    expect(platform.notices[0].channel, 'agents-review');
    expect(platform.notices[0].body, 'Un agente ha terminado');
    expect(platform.notices[1].channel, 'recordatorios');
    expect(platform.notices[1].body, 'Tienes un recordatorio del calendario');
  });

  test('an alert of an unknown kind is ignored', () async {
    final source = FakeAlertSource([Stream.fromIterable(['u', 'b'])]);
    unawaited(streamWith(source).run());

    await until(() => platform.notices.isNotEmpty);
    expect(platform.notices.single.body, 'Un agente necesita tu atención');
  });

  test('with alert rules, a finished task does not alert and the host alert does', () async {
    final source = FakeAlertSource([Stream.fromIterable(['y', 'd'])])
      ..stateJson = _rulesState;
    unawaited(streamWith(source).run());

    await until(() => platform.notices.isNotEmpty);
    expect(platform.notices.single.body, 'Un agente ha terminado');
  });

  test('with alert rules, an approval still alerts with the state text', () async {
    final source = FakeAlertSource([Stream.fromIterable(['x'])])
      ..stateJson = '{"alert_rules":{"mode":"rules_only","enabled_count":0},'
          '"chat":{"pending_user_approvals":1},"counts":{}}';
    unawaited(streamWith(source).run());

    await until(() => platform.notices.isNotEmpty);
    expect(platform.notices.single.channel, 'agents');
    expect(platform.notices.single.body, 'Una aprobación espera tu decisión.');
  });

  test('a state read at connect that fails keeps the older rule and says so', () async {
    final source = FakeAlertSource([Stream.fromIterable(['y'])])
      ..stateError = const SocketException('sin red');
    unawaited(streamWith(source).run());

    await until(() => platform.notices.isNotEmpty);
    expect(platform.records, contains('sin estado: sin red'));
    expect(platform.notices.single.channel, 'agents-review');
  });

  test('a reconnect resumes after the last id the PC sent', () async {
    final source = FakeAlertSource([Stream.fromIterable(['id: 7'])]);
    unawaited(streamWith(source).run());

    await until(() => source.resumedFrom.length >= 2);
    expect(source.resumedFrom, [null, '7']);
  });

  test('silence past the heartbeat drops the connection and reconnects', () async {
    final source = FakeAlertSource([StreamController<String>().stream]);
    unawaited(
      streamWith(source, heartbeat: const Duration(milliseconds: 30)).run(),
    );

    await until(() => source.resumedFrom.length >= 2);
    expect(platform.records.any((line) => line.contains('sin datos del PC')), isTrue);
    expect(platform.connections.take(2), [true, false]);
  });

  test('a refused connection says its status and waits the first step', () async {
    final source = FakeAlertSource([const HostError(503, 'no')]);
    final waits = <Duration>[];
    unawaited(
      streamWith(
        source,
        delay: (wait) async => waits.add(wait),
      ).run(),
    );

    await until(() => source.resumedFrom.length >= 2);
    expect(platform.records.first, contains('HTTP 503'));
    expect(platform.records.first, contains('5 s'));
    expect(waits.first, const Duration(seconds: 5));
  });

  test('a transport failure names its kind instead of HTTP 0', () async {
    final source = FakeAlertSource([
      const HostError.transport(ConnectionFailure.identityMismatch, 'x'),
    ]);
    unawaited(streamWith(source, delay: (wait) async {}).run());

    await until(() => source.resumedFrom.length >= 2);
    expect(platform.records.first, contains('transporte: identityMismatch'));
  });

  test('a network that comes back skips the wait', () async {
    final source = FakeAlertSource([const SocketException('sin ruta')]);
    final stream = streamWith(
      source,
      delay: (_) => Completer<void>().future,
    );
    unawaited(stream.run());

    await until(() => platform.records.any((line) => line.contains('nuevo intento')));
    expect(source.resumedFrom, hasLength(1));

    stream.networkUp();
    await until(() => source.resumedFrom.length == 2);
  });

  test('after a network wake a failed try starts the short steps again', () async {
    final source = FakeAlertSource([
      for (var i = 0; i < 6; i++) const SocketException('sin ruta'),
    ]);
    var calls = 0;
    final stream = streamWith(
      source,
      delay: (_) => ++calls <= 3 ? Future<void>.value() : Completer<void>().future,
    );
    unawaited(stream.run());

    await until(() => platform.records.where((line) => line.contains('nuevo intento')).length == 4);
    expect(platform.records.last, contains('20 s'));

    stream.networkUp();
    await until(() => platform.records.where((line) => line.contains('nuevo intento')).length == 5);
    expect(platform.records.last, contains('5 s'));
  });
}
