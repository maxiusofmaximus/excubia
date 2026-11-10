import 'dart:convert';

import 'package:excubia/core/api/chat_transport.dart';
import 'package:excubia/core/api/host_client.dart';

/// The host of the first-run team, as tests need it: one canned answer per
/// route (`METHOD /path`), a `HostError` where the host refuses, and a record
/// of every POST body. Nothing reaches the network.
class FakeOnboardingHost implements ChatTransport {
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  FakeOnboardingHost(this.answers);

  final Map<String, Object> answers;
  final List<String> calls = <String>[];
  final List<Map<String, Object?>> posts = <Map<String, Object?>>[];

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final route = '$method ${target.split('?').first}';
    calls.add(route);
    if (method == 'POST') {
      posts.add(jsonDecode(utf8.decode(body)) as Map<String, Object?>);
    }
    final answer = answers[route];
    if (answer is HostError) throw answer;
    if (answer is String) return answer;
    throw StateError('no canned answer for $route');
  }
}

/// A device that may contract: canned answers for the three routes the assistant
/// reads and writes. A test changes one answer when it needs another.
FakeOnboardingHost standardHost({
  String catalog = onboardingCatalogJson,
  Object onboarding = onboardingPendingJson,
  Object post = onboardingSavedJson,
}) => FakeOnboardingHost({
  'GET /v1/roles': catalog,
  'GET /v1/onboarding': onboarding,
  'POST /v1/onboarding': post,
});

/// Two areas: video (three roles, the level lists differ) and programación (one
/// role, avatar `bob`), plus «otras» with no team proposed.
const String onboardingCatalogJson = '''
{"ok": true, "version": 1,
 "areas": [
  {"id": "video", "label": "Vídeo y postproducción", "roles": ["vera", "jade", "karim"],
   "levels": {"personal": ["vera"], "profesional": ["vera", "jade"], "equipo": ["vera", "jade", "karim"]}},
  {"id": "programacion", "label": "Programación", "roles": ["bob"],
   "levels": {"personal": ["bob"], "profesional": ["bob"], "equipo": ["bob"]}},
  {"id": "otras", "label": "Otras", "roles": [],
   "levels": {"personal": [], "profesional": [], "equipo": []}}],
 "roles": [
  {"id": "vera", "name": "Vera", "title": "Editor de vídeo", "area": "video", "avatar": null},
  {"id": "jade", "name": "Jade", "title": "Colorista", "area": "video", "avatar": null},
  {"id": "karim", "name": "Karim", "title": "Motion", "area": "video", "avatar": null},
  {"id": "bob", "name": "Bob", "title": "Desarrollador", "area": "programacion", "avatar": "bob"}]}
''';

/// One area with nine roles, all in «equipo»: more than the host allows.
String get nineRolesCatalogJson {
  final ids = [for (var i = 1; i <= 9; i++) 'r$i'];
  return jsonEncode({
    'ok': true,
    'areas': [
      {
        'id': 'video',
        'label': 'Vídeo y postproducción',
        'roles': ids,
        'levels': {
          'personal': ['r1'],
          'profesional': ['r1'],
          'equipo': ids,
        },
      },
    ],
    'roles': [
      for (final id in ids)
        {
          'id': id,
          'name': 'R${id.substring(1)}',
          'title': 'Rol $id',
          'area': 'video',
          'avatar': null,
        },
    ],
  });
}

const String onboardingPendingJson = '{"ok": true, "done": false, "areas": []}';
const String onboardingPendingCanHireJson =
    '{"ok": true, "done": false, "areas": [], "can_hire": true}';
const String onboardingPendingNoHireJson =
    '{"ok": true, "done": false, "areas": [], "can_hire": false}';
const String onboardingDoneJson =
    '{"ok": true, "done": true, "areas": ["video"]}';
const String onboardingSavedJson = '{"ok": true, "done": true, "hires": []}';
