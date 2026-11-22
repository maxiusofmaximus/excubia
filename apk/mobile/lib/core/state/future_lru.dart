import 'dart:collection';

/// The values of the last [capacity] keys, loaded once each as futures. A load
/// that failed stays until [forget]; the least recently used key goes first.
class FutureLru<K, V> {
  FutureLru(this._load, {required this.capacity});

  final Future<V> Function(K key) _load;
  final int capacity;
  final LinkedHashMap<K, Future<V>> _cache = LinkedHashMap<K, Future<V>>();

  Future<V> load(K key) {
    final future = _cache.remove(key) ?? _load(key);
    _cache[key] = future;
    while (_cache.length > capacity) {
      _cache.remove(_cache.keys.first);
    }
    return future;
  }

  /// Drops [key], so the next [load] asks again.
  void forget(K key) => _cache.remove(key);
}
