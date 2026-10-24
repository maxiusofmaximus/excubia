import '../../src/rust/api/chat/plan.dart';
import 'chat_transport.dart';

/// Raised when a Rust plan refuses to build a request (a bad id or mode). The
/// request never reaches the wire.
class PlanRefused implements Exception {
  const PlanRefused();
}

/// Sends the request that a Rust plan built. Dart never builds a route itself.
Future<String> runPlan(ChatTransport transport, ChatPlan plan) {
  final request = plan.request;
  if (request == null) throw const PlanRefused();
  return transport.call(request.method, request.target, request.body);
}
