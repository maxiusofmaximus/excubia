import '../../src/rust/api/alfred.dart';
import '../../src/rust/api/error.dart';
import 'alfred_backend_models.dart';
import 'chat_transport.dart';
import 'host_client.dart';
import 'plan_runner.dart';

/// The engine of Alfred on the PC (`GET` and `POST /v1/alfred/backend`). The routes are
/// built in Rust; a change is asked for only after the person confirms it on the phone.
class AlfredBackendApi {
  const AlfredBackendApi(this._transport);

  final ChatTransport _transport;

  Future<AlfredBackendStatus> read() async => parseAlfredBackendStatus(
    await runPlan(_transport, planAlfredBackendRead()),
  );

  /// [backend] is `local` or `claude`. The PC answers 200 when it is already there and
  /// 202 when the change has started; both carry the status.
  Future<AlfredBackendStatus> change(String backend) async =>
      parseAlfredBackendStatus(
        await runPlan(_transport, planAlfredBackendSet(backend: backend)),
      );
}

/// What the screen says for a failed call. A refusal from the PC has its Rust words; a
/// bad answer reads as the app's generic one.
ErrorInfo alfredFailureOf(Object error) => switch (error) {
  HostError(:final status, :final body) => explainAlfredBackendError(
    status: status,
    body: body,
  ),
  _ => unreadableAnswerInfo(),
};
