/// The signed HTTP surface the chat controller needs. `HostClient` is the
/// production implementation; tests inject a fake one.
abstract interface class ChatTransport {
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]);

  /// A binary answer, such as the stored WebP of an attachment.
  Future<List<int>> callBytes(String method, String target);

  /// A raw body under its own content type, such as a voice note's Ogg bytes.
  /// The answer is the JSON text.
  Future<String> postBytes(String target, List<int> body, String contentType);
}
