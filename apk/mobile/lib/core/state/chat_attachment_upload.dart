import '../../src/rust/api/chat/attach_model.dart';
import '../../src/rust/api/chat/attach_plan.dart';
import '../../src/rust/api/chat/attach_upload.dart';
import '../../src/rust/api/chat/plan.dart';
import '../api/chat_attachment_source.dart';
import '../api/chat_transport.dart';
import '../api/gateway_client.dart';
import '../api/plan_runner.dart';

/// The host refused the upload for good; [code] is the contract code.
class AttachmentRefused implements Exception {
  const AttachmentRefused(this.code);

  final String code;
}

/// How many `429`s one request waits out before it gives up.
const _maxSlowDowns = 5;

/// Uploads one picked file: create, chunks in series with resume, complete.
/// Returns the ready file, or null when [isCancelled] stopped it first.
Future<ChatAttachment?> uploadAttachment({
  required ChatTransport transport,
  required String conversationId,
  required ChatAttachmentSource source,
  required bool Function() isCancelled,
  required void Function(String remoteId) onCreated,
  required void Function(double progress) onProgress,
  required Future<void> Function(Duration) pause,
}) async {
  try {
    return await _upload(
      transport: transport,
      conversationId: conversationId,
      source: source,
      isCancelled: isCancelled,
      onCreated: onCreated,
      onProgress: onProgress,
      pause: pause,
    );
  } on AttachmentFailure catch (failure) {
    throw AttachmentRefused(failure.code);
  }
}

Future<ChatAttachment?> _upload({
  required ChatTransport transport,
  required String conversationId,
  required ChatAttachmentSource source,
  required bool Function() isCancelled,
  required void Function(String remoteId) onCreated,
  required void Function(double progress) onProgress,
  required Future<void> Function(Duration) pause,
}) async {
  final sha256 = await source.sha256();
  if (sha256 == null) throw const AttachmentRefused('unreadable');
  final created = await _send(
    transport,
    planAttachmentCreate(
      conversationId: conversationId,
      clientId: newChatClientId(),
      name: source.name,
      size: BigInt.from(source.size),
      sha256: sha256,
    ),
    pause,
  );
  final id = parseAttachmentCreated(json: created);
  if (id == null) throw const AttachmentRefused('invalid_answer');
  onCreated(id);

  var cursor = uploadStart(size: BigInt.from(source.size));
  while (true) {
    if (isCancelled()) return null;
    final span = uploadNext(cursor: cursor);
    if (span == null) break;
    final data = await source.read(span.offset.toInt(), span.len.toInt());
    cursor = await _sendChunk(
      transport: transport,
      conversationId: conversationId,
      attachmentId: id,
      cursor: cursor,
      span: span,
      data: data,
      pause: pause,
    );
    onProgress(uploadProgress(cursor: cursor));
  }

  final done = await _send(
    transport,
    planAttachmentComplete(conversationId: conversationId, attachmentId: id),
    pause,
  );
  final file = parseAttachmentCompleted(json: done);
  if (file == null) throw const AttachmentRefused('not_ready');
  return file;
}

/// One chunk. A refused chunk resumes from the host's offset, or repeats, as
/// the Rust cursor decides; when it gives up the refusal ends the upload.
Future<UploadCursor> _sendChunk({
  required ChatTransport transport,
  required String conversationId,
  required String attachmentId,
  required UploadCursor cursor,
  required ChunkSpan span,
  required List<int> data,
  required Future<void> Function(Duration) pause,
}) async {
  try {
    final answer = await _send(
      transport,
      planAttachmentChunk(
        conversationId: conversationId,
        attachmentId: attachmentId,
        offset: span.offset,
        data: data,
      ),
      pause,
    );
    final next = parseAttachmentNextOffset(json: answer);
    if (next == null) throw const AttachmentRefused('invalid_answer');
    return uploadAccepted(cursor: cursor, nextOffset: next);
  } on AttachmentFailure catch (failure) {
    final resumed = uploadRefused(cursor: cursor, failure: failure);
    if (resumed == null) throw AttachmentRefused(failure.code);
    return resumed;
  }
}

/// Sends one request. A `429` waits its `Retry-After` and sends it again; any
/// other refusal comes back as the [AttachmentFailure] the Rust side reads.
Future<String> _send(
  ChatTransport transport,
  ChatPlan plan,
  Future<void> Function(Duration) pause,
) async {
  for (var waits = 0; ; waits++) {
    try {
      return await runPlan(transport, plan);
    } on PlanRefused {
      throw const AttachmentRefused('invalid_request');
    } on GatewayError catch (error) {
      final failure = explainAttachmentError(
        status: error.status,
        body: error.body,
        retryAfter: error.retryAfter,
      );
      if (failure.status != 429 || waits >= _maxSlowDowns) throw failure;
      await pause(Duration(seconds: failure.retryAfter ?? 1));
    }
  }
}
