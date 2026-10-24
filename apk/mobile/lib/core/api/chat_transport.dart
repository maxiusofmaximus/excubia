/// The signed HTTP surface the chat controller needs. `GatewayClient` is the
/// production implementation; tests inject a fake one.
abstract interface class ChatTransport {
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]);
}
