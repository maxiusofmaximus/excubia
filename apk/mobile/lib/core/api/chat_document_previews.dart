import 'dart:typed_data';

import '../../src/rust/api/chat/attach_model.dart';
import '../../src/rust/api/chat/attach_plan.dart';
import '../state/future_lru.dart';
import 'chat_transport.dart';
import 'host_client.dart';
import 'plan_runner.dart';

/// The pages of a document as PNG pictures. The last [capacity] pages stay in
/// memory; a page that failed is not kept, so the next call asks again.
class ChatDocumentPreviews {
  ChatDocumentPreviews(
    ChatTransport transport, {
    required String? Function() conversation,
    int capacity = 6,
  }) : _cache = FutureLru<(String, int), Uint8List>(
         (key) => _load(transport, conversation(), key),
         capacity: capacity,
       );

  final FutureLru<(String, int), Uint8List> _cache;

  /// The PNG of [page] (from 1) of an attachment of the open conversation. A
  /// refusal is a [PreviewRefusal] from the Rust mapping.
  Future<Uint8List> page(String attachmentId, int page) async {
    final key = (attachmentId, page);
    try {
      return await _cache.load(key);
    } catch (_) {
      _cache.forget(key);
      rethrow;
    }
  }

  static Future<Uint8List> _load(
    ChatTransport transport,
    String? conversationId,
    (String, int) key,
  ) async {
    final (attachmentId, page) = key;
    if (conversationId == null) throw _other;
    final List<int> answer;
    try {
      answer = await runPlanBytes(
        transport,
        planAttachmentPreview(
          conversationId: conversationId,
          attachmentId: attachmentId,
          page: page,
        ),
      );
    } on HostError catch (error) {
      throw explainPreviewError(status: error.status, body: error.body);
    } on PlanRefused {
      throw _other;
    }
    return checkedPreviewPng(bytes: answer) ?? (throw _other);
  }

  static const _other = PreviewRefusal(failure: PreviewFailure.other);
}
