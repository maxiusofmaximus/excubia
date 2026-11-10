import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/alfred_backend_api.dart';
import 'package:excubia/core/api/alfred_backend_models.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/plan_runner.dart';
import 'package:excubia/src/rust/frb_generated.dart';

import '../support/scripted_transport.dart';

const String _read = 'GET /v1/alfred/backend';
const String _change = 'POST /v1/alfred/backend';

const String _claude =
    '{"backend":"claude","state":"claude","detail":null,"since":1762200000,"voice_suspended":false}';

void main() {
  setUpAll(() async => RustLib.init());

  group('the status reads tolerantly', () {
    test('reads the engine and ignores the fields it does not know', () {
      final status = parseAlfredBackendStatus(
        '{"backend":"local","state":"local","detail":null,"since":1762200000,'
        '"voice_suspended":true,"extra":{"x":1}}',
      );
      expect(status.backend, 'local');
      expect(status.state, AlfredEngineState.local);
      expect(status.detail, isNull);
      expect(status.since, 1762200000);
      expect(status.voiceSuspended, isTrue);
    });

    test('an unknown state reads as Claude and keeps the detail', () {
      final status = parseAlfredBackendStatus(
        '{"backend":"claude","state":"quantum","detail":"Algo nuevo"}',
      );
      expect(status.state, AlfredEngineState.claude);
      expect(status.detail, 'Algo nuevo');
      expect(status.voiceSuspended, isFalse);
    });

    test('an answer that is not an object is refused', () {
      expect(() => parseAlfredBackendStatus('[]'), throwsA(anything));
    });

    test('the ready CLI engines are read by name and version', () {
      final status = parseAlfredBackendStatus(
        '{"backend":"claude","state":"claude","detail":null,"since":1,'
        '"voice_suspended":false,"engines":[{"name":"opencode","version":"2.2.6"}]}',
      );
      expect(status.engines.single.name, 'opencode');
      expect(status.engines.single.version, '2.2.6');
    });

    test('a PC that predates the engines list reads as none listed', () {
      final status = parseAlfredBackendStatus(_claude);
      expect(status.engines, isEmpty);
    });
  });

  group('the calls', () {
    test('the read is a GET of the engine state', () async {
      final transport = ScriptedTransport({_read: _claude});
      final status = await AlfredBackendApi(transport).read();
      expect(transport.calls, [_read]);
      expect(status.state, AlfredEngineState.claude);
    });

    test(
      'a change posts the engine name and reads the status it answers',
      () async {
        final transport = ScriptedTransport({
          _change: '{"backend":"claude","state":"starting","detail":null,"since":1,"voice_suspended":true}',
        });
        final status = await AlfredBackendApi(transport).change('local');
        expect(transport.bodies[_change], [
          {'backend': 'local'},
        ]);
        expect(status.state, AlfredEngineState.starting);
        expect(status.voiceSuspended, isTrue);
      },
    );

    test(
      'a refusal keeps the words of the PC and the call is not retried',
      () async {
        final transport = ScriptedTransport({_change: _claude})
          ..failNext(_change, const HostError(403, '{"detail":"forbidden"}'));
        final api = AlfredBackendApi(transport);

        Object? thrown;
        try {
          await api.change('local');
        } catch (error) {
          thrown = error;
        }
        expect(thrown, isA<HostError>());
        expect(
          alfredFailureOf(thrown!).message,
          'Este móvil no puede cambiar el motor de Alfred.',
        );
        expect(transport.calls, [_change]);
      },
    );

    test('an answer that is not the status reads as the generic failure', () {
      expect(
        alfredFailureOf(const FormatException('x')).message,
        unreadableAnswerInfo().message,
      );
    });
  });
}
