import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/agenda_api.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/agenda_list_controller.dart';

void main() {
  test('a search shows its first page and says whether more follow', () async {
    final calls = <(String, String?)>[];
    final list = AgendaListController<String>((query, cursor) async {
      calls.add((query, cursor));
      return AgendaPage(items: ['Ana', 'Ana 2'], next: 'c1');
    });

    await list.search('ana');

    expect(calls, [('ana', null)]);
    expect(list.items, ['Ana', 'Ana 2']);
    expect(list.hasMore, isTrue);
    expect(list.loading, isFalse);
  });

  test('loadMore adds the next page after the rows shown', () async {
    final cursors = <String?>[];
    final list = AgendaListController<String>((query, cursor) async {
      cursors.add(cursor);
      return cursor == null
          ? AgendaPage(items: ['a'], next: 'c1')
          : AgendaPage(items: ['b'], next: null);
    });

    await list.search('');
    await list.loadMore();

    expect(cursors, [null, 'c1']);
    expect(list.items, ['a', 'b']);
    expect(list.hasMore, isFalse);
  });

  test('loadMore asks for nothing when there is no next page', () async {
    var calls = 0;
    final list = AgendaListController<String>((query, cursor) async {
      calls++;
      return AgendaPage(items: ['a'], next: null);
    });

    await list.search('');
    await list.loadMore();

    expect(calls, 1);
  });

  test('an answer to an older search is dropped', () async {
    final slow = Completer<AgendaPage<String>>();
    final list = AgendaListController<String>((query, cursor) {
      if (query == 'a') return slow.future;
      return Future.value(AgendaPage(items: ['B'], next: null));
    });

    final firstSearch = list.search('a');
    await list.search('b');
    slow.complete(AgendaPage(items: ['A'], next: null));
    await firstSearch;

    expect(list.query, 'b');
    expect(list.items, ['B']);
  });

  test('a failed search says why and shows no rows', () async {
    var fail = false;
    final list = AgendaListController<String>((query, cursor) async {
      if (fail) throw const HostError(500, '{"detail":{"code":"x"}}');
      return AgendaPage(items: ['a'], next: null);
    });

    await list.search('');
    expect(list.items, ['a']);

    fail = true;
    await list.search('');

    expect(list.failure, 'El equipo no pudo completar la acción. Inténtalo de nuevo.');
    expect(list.items, isEmpty);
    expect(list.loading, isFalse);
  });
}
