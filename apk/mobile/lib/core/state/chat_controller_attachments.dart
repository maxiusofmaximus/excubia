part of 'chat_controller.dart';

/// The files an unsent message names, and the stored bytes the timeline shows.
extension ChatControllerAttachments on ChatController {
  /// The files of an unsent message, so its bubble shows them; null otherwise.
  List<ChatAttachment>? attachmentsOfSend(String? clientId) =>
      clientId == null ? null : _attachmentsByClient[clientId];

  /// The stored WebP of a ready file in the open conversation. Async, so a
  /// missing conversation reaches the image as an error, not a throw.
  Future<List<int>> _attachmentBytes(String attachmentId) async {
    final conversationId = current?.id;
    if (conversationId == null) {
      throw StateError('No hay conversación abierta.');
    }
    return runPlanBytes(
      _transport,
      planAttachmentContent(
        conversationId: conversationId,
        attachmentId: attachmentId,
      ),
    );
  }
}
