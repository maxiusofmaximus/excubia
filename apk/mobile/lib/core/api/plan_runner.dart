import '../../src/rust/api/chat/plan.dart';
import '../../src/rust/api/error.dart';
import '../../src/rust/api/write.dart' show HttpCall;
import 'chat_transport.dart';

/// Raised when a Rust plan refuses to build a request (a bad id, mode or name).
/// The request never reaches the wire; [problem] says why, when the plan does.
class PlanRefused implements Exception {
  const PlanRefused([this.problem]);

  final ErrorInfo? problem;
}

/// Raised when the host answered, but not in a shape the app reads.
class UnreadableAnswer implements Exception {
  const UnreadableAnswer();
}

/// What the user sees for an [UnreadableAnswer].
ErrorInfo unreadableAnswerInfo() => ErrorInfo(
  message: 'La respuesta del equipo no es válida.',
  action: 'Vuelve a intentarlo.',
  kind: FailureKind.retry,
);

/// Sends the request that a Rust plan built. Dart never builds a route itself.
Future<String> runPlan(ChatTransport transport, ChatPlan plan) {
  final request = _requestOf(plan);
  return transport.call(request.method, request.target, request.body);
}

/// The bytes of the answer a Rust plan asked for, such as a stored attachment.
Future<List<int>> runPlanBytes(ChatTransport transport, ChatPlan plan) {
  final request = _requestOf(plan);
  return transport.callBytes(request.method, request.target);
}

HttpCall _requestOf(ChatPlan plan) {
  final request = plan.request;
  if (request == null) throw PlanRefused(plan.problem);
  return request;
}
