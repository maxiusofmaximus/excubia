import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_transport.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/team_summary_api.dart';
import 'package:excubia/core/state/team_summary_controller.dart';

/// Answers each read with the next scripted answer: a `(name, status)` pair is
/// one member, a [HostError] is thrown. A read waits on [gate] when one is set,
/// so an overlap can be counted.
class _Host implements ChatTransport {
  final List<Object> answers = [];
  int reads = 0;
  int inFlight = 0;
  int maxInFlight = 0;
  Completer<void>? gate;

  String _members(String name, String status) => jsonEncode({
    'ok': true,
    'members': [
      {
        'agent_id': 'hire_1',
        'name': name,
        'role_title': 'Programador',
        'status': status,
        'headline': 'Sin actividad',
        'since': '2026-11-05T10:00:00.000Z',
      },
    ],
  });

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    reads++;
    inFlight++;
    if (inFlight > maxInFlight) maxInFlight = inFlight;
    try {
      await gate?.future;
      final answer = answers.isEmpty ? ('Bob', 'idle') : answers.removeAt(0);
      if (answer is HostError) throw answer;
      final (name, status) = answer as (String, String);
      return _members(name, status);
    } finally {
      inFlight--;
    }
  }

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();
}

const _tick = Duration(milliseconds: 15);

TeamSummaryController _controller(_Host host) {
  final controller = TeamSummaryController(
    api: TeamSummaryApi(host),
    interval: _tick,
  );
  addTearDown(controller.dispose);
  return controller;
}

void main() {
  test(
    'a failed read has no members yet; a good one clears the failure',
    () async {
      final host = _Host()
        ..answers.addAll([const HostError(500, '{}'), ('Bob', 'idle')]);
      final controller = _controller(host);

      await controller.refresh();
      expect(controller.error, isA<HostError>());
      expect(controller.loaded, isFalse);

      await controller.refresh();
      expect(controller.error, isNull);
      expect(controller.loaded, isTrue);
      expect(controller.members.single.name, 'Bob');
    },
  );

  test('a failed read after a good one keeps the cards on screen', () async {
    final host = _Host()
      ..answers.addAll([('Bob', 'idle'), const HostError(0, 'sin red')]);
    final controller = _controller(host);

    await controller.refresh();
    await controller.refresh();

    expect(controller.error, isNotNull);
    expect(controller.members.single.name, 'Bob');
  });

  test(
    'a read asked while one is in flight starts no second request',
    () async {
      final host = _Host()..gate = Completer<void>();
      final controller = _controller(host);

      final first = controller.refresh();
      final second = controller.refresh();
      host.gate!.complete();
      await Future.wait([first, second]);

      expect(host.reads, 1);
    },
  );

  test('the count of members waiting for the user', () async {
    final host = _Host()..answers.addAll([('Ana', 'waiting_for_you')]);
    final controller = _controller(host);

    await controller.refresh();

    expect(controller.waitingCount, 1);
  });

  test('being active polls, one read at a time; stopping ends it', () async {
    final host = _Host();
    final controller = _controller(host);

    controller.setActive(true);
    await Future<void>.delayed(const Duration(milliseconds: 100));
    expect(host.reads, greaterThan(2));
    expect(host.maxInFlight, 1);

    controller.setActive(false);
    final stopped = host.reads;
    await Future<void>.delayed(const Duration(milliseconds: 80));
    expect(host.reads, stopped);
  });

  test('a read that ends after stopping does not schedule another', () async {
    final host = _Host()..gate = Completer<void>();
    final controller = _controller(host);

    controller.setActive(true);
    controller.setActive(false);
    host.gate!.complete();
    await Future<void>.delayed(const Duration(milliseconds: 60));

    expect(host.reads, 1);
  });

  test('disposing stops the timer for good', () async {
    final host = _Host();
    final controller = TeamSummaryController(
      api: TeamSummaryApi(host),
      interval: _tick,
    );

    controller.setActive(true);
    await Future<void>.delayed(const Duration(milliseconds: 40));
    controller.dispose();
    final stopped = host.reads;
    await Future<void>.delayed(const Duration(milliseconds: 60));

    expect(host.reads, stopped);
  });
}
