import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/plan_runner.dart';
import 'package:excubia/src/rust/api/chat/plan.dart';
import 'package:excubia/src/rust/api/write.dart' show HttpCall;

import '../support/recording_transport.dart';

void main() {
  test('a POST with a body is sent as JSON (the host answers 415 otherwise)', () async {
    final transport = RecordingTransport();
    final body = Uint8List.fromList(utf8.encode('{"name":"codex"}'));
    final plan = ChatPlan(
      request: HttpCall(method: 'POST', target: '/v1/catalog/default', body: body),
    );

    await runPlan(transport, plan);

    expect(transport.targets, ['POST /v1/catalog/default']);
    expect(transport.contentTypes, ['application/json']);
    expect(transport.bodies.single, {'name': 'codex'});
  });

  test('a GET without body still goes through call', () async {
    final transport = RecordingTransport();
    final plan = ChatPlan(
      request: HttpCall(method: 'GET', target: '/v1/catalog', body: Uint8List(0)),
    );

    await runPlan(transport, plan);

    expect(transport.targets, ['GET /v1/catalog']);
    expect(transport.contentTypes, isEmpty);
  });
}
