import 'package:flutter/foundation.dart';

import '../api/agenda_api.dart';

/// A searchable, paged agenda list. [loadMore] adds the next page to the rows
/// shown; a new search replaces them. An answer that arrives after a newer load
/// started is dropped, so the list always shows the last search asked for.
class AgendaListController<T> extends ChangeNotifier {
  AgendaListController(this._fetch);

  final Future<AgendaPage<T>> Function(String query, String? cursor) _fetch;
  List<T> _items = const [];
  String? _next;
  bool _truncated = false;
  String _query = '';
  bool _loading = false;
  String? _failure;
  int _generation = 0;
  bool _disposed = false;

  List<T> get items => _items;
  String get query => _query;
  bool get loading => _loading;
  bool get hasMore => _next != null;

  /// Whether the last page was cut short by the host (see [AgendaPage.truncated]).
  bool get truncated => _truncated;

  /// The failure text of the last load, or `null`. Cleared by the next load.
  String? get failure => _failure;

  Future<void> search(String query) => _load(query, more: false);

  /// Reads the same search again, e.g. after a row changed.
  Future<void> reload() => _load(_query, more: false);

  Future<void> loadMore() => _load(_query, more: true);

  Future<void> _load(String query, {required bool more}) async {
    if (more && (_next == null || _loading)) return;
    final generation = ++_generation;
    _query = query;
    _loading = true;
    _failure = null;
    _notify();
    AgendaPage<T>? page;
    String? failure;
    try {
      page = await _fetch(query, more ? _next : null);
    } catch (error) {
      failure = agendaFailureText(error);
    }
    if (generation != _generation) return;
    _loading = false;
    _failure = failure;
    if (page != null) {
      _items = more ? [..._items, ...page.items] : page.items;
      _next = page.next;
      _truncated = page.truncated;
    } else if (!more) {
      _items = const [];
      _next = null;
      _truncated = false;
    }
    _notify();
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
