import 'dart:typed_data';

import 'future_lru.dart';

/// The stored images of the timeline, kept for the last [capacity] ids. A load
/// that failed stays cached too, so a rebuild does not ask the host again.
class ChatAttachmentImages {
  ChatAttachmentImages(
    Future<List<int>> Function(String attachmentId) fetch, {
    int capacity = 12,
  }) : _cache = FutureLru<String, Uint8List>(
         (id) => fetch(id).then<Uint8List>(Uint8List.fromList),
         capacity: capacity,
       );

  final FutureLru<String, Uint8List> _cache;

  Future<Uint8List> load(String attachmentId) => _cache.load(attachmentId);
}
