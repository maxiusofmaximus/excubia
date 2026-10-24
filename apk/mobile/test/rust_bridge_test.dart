import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api.dart' show coreVersion;
import 'package:herdr_notify/src/rust/api/signing.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/src/rust/api/stream.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

/// Proves the bridge itself works: the Dart VM loads the Rust library and the
/// contract logic (not a Dart copy) answers.
void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test('the Rust core answers through the bridge', () {
    expect(coreVersion(), isNotEmpty);

    final state = parseState(
      json: '{"revision":5,"agents":[{"pane_id":"w1:p1","status":"blocked"}]}',
    );
    expect(state, isNotNull);
    expect(state!.revision, 5);
    expect(state.agents.single.status, AgentStatus.blocked);
  });

  test('the SSE decoder frames state and reports sequence holes', () {
    final decoder = newStreamDecoder();
    final events = <StreamEvent>[];
    for (final line in <String>[
      'event: state',
      'data: {"event":"state","revision":1,"state":{"revision":1}}',
      '',
      'event: status',
      'data: {"revision":2,"state_change_seq":4}',
      '',
      'event: status',
      'data: {"revision":3,"state_change_seq":9}',
      '',
    ]) {
      final event = decoder.pushLine(line: line);
      if (event != null) events.add(event);
    }
    expect(events, hasLength(3));
    expect(events[0].kind, StreamEventKind.state);
    expect(events[1].seqGap, isFalse);
    expect(events[2].seqGap, isTrue);
  });

  test('keys and signatures are produced by Rust', () {
    final material = generateDeviceKey();
    expect(material.publicKeyB64, isNotEmpty);
    expect(material.fingerprint.split(':'), hasLength(8));

    final signed = signRequest(
      privateKeyB64: material.privateKeyB64,
      method: 'GET',
      target: '/v1/state',
      body: const <int>[],
    );
    expect(signed, isNotNull);
    expect(signed!.signature, isNotEmpty);
    expect(signed.nonce, isNotEmpty);
  });
}
