import 'dart:collection';
import 'dart:typed_data';

/// The stored images of the timeline, kept for the last [capacity] ids. A load
/// that failed stays cached too, so a rebuild does not ask the host again; the
/// least recently used entry goes first.
class ChatAttachmentImages {
  ChatAttachmentImages(this._fetch, {this.capacity = 12});

  final Future<List<int>> Function(String attachmentId) _fetch;
  final int capacity;
  final LinkedHashMap<String, Future<Uint8List>> _cache =
      LinkedHashMap<String, Future<Uint8List>>();

  Future<Uint8List> load(String attachmentId) {
    final future =
        _cache.remove(attachmentId) ??
        _fetch(attachmentId).then<Uint8List>(Uint8List.fromList);
    _cache[attachmentId] = future;
    while (_cache.length > capacity) {
      _cache.remove(_cache.keys.first);
    }
    return future;
  }
}
