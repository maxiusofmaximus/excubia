import 'dart:convert';

import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';

/// A stateful host of the team, as `errors.md` and `api.md` describe it: hires
/// are registered, renamed, given a character and dismissed, and the list
/// reads back what was written. Nothing reaches the network.
///
/// Two behaviours are kept from the real host on purpose: an `avatar: null` in
/// an update is read as "no change" (`routes/hires.rs`), and a second dismissal
/// answers the same hire.
class FakeTeamHost implements ChatTransport {
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  FakeTeamHost({this.canHire = true, List<Map<String, Object?>>? hires})
    : hires = hires ?? <Map<String, Object?>>[];

  /// The device holds `APPROVE`; reported as `can_hire` on the reads.
  bool canHire;

  /// Every hire the host keeps, dismissed ones included.
  final List<Map<String, Object?>> hires;

  /// When true, a contract answers `409 area_full`.
  bool areaFull = false;

  /// When true, every call fails with the device's network error.
  bool networkDown = false;

  final List<String> calls = <String>[];
  final List<Map<String, Object?>> posts = <Map<String, Object?>>[];
  int _nextId = 1;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final route = '$method ${target.split('?').first}';
    calls.add(route);
    if (networkDown) throw const GatewayError(0, 'sin red');
    final json = body.isEmpty
        ? <String, Object?>{}
        : jsonDecode(utf8.decode(body)) as Map<String, Object?>;
    if (method == 'POST') posts.add(json);

    if (route == 'GET /v1/roles') return jsonEncode(teamCatalog);
    if (route == 'GET /v1/hires') {
      return jsonEncode({
        'ok': true,
        'hires': [
          for (final hire in hires)
            if (hire['status'] == 'hired') hire,
        ],
        'can_hire': canHire,
      });
    }
    if (route == 'POST /v1/hires') return _contract(json);

    final match = RegExp(r'^POST /v1/hires/([^/]+)/(update|dismiss)$')
        .firstMatch(route);
    if (match != null) {
      final hire = _find(match.group(1)!);
      if (hire == null) throw _refusal(404, 'no_such_hire');
      if (match.group(2) == 'dismiss') {
        hire['status'] = 'removed';
      } else {
        _update(hire, json);
      }
      return jsonEncode({'ok': true, 'hire': hire});
    }
    throw StateError('no route in the fake team host: $route');
  }

  String _contract(Map<String, Object?> json) {
    if (!canHire) throw _refusal(403, null);
    if (areaFull) throw _refusal(409, 'area_full');
    final roleId = json['role'] as String;
    final role = teamRole(roleId);
    if (role == null) throw _refusal(422, 'unknown_role');
    final existing = [
      for (final hire in hires)
        if (hire['role'] == roleId && hire['status'] == 'hired') hire,
    ];
    if (existing.isNotEmpty) {
      return jsonEncode({'ok': true, 'hire': existing.first});
    }
    final hire = <String, Object?>{
      'id': 'hire_${_nextId++}',
      'role': roleId,
      'area': role['area'],
      'name': role['name'],
      'avatar': role['avatar'],
      'status': 'hired',
      'created_at': '2026-10-25T10:00:00.000Z',
    };
    hires.add(hire);
    return jsonEncode({'ok': true, 'hire': hire});
  }

  void _update(Map<String, Object?> hire, Map<String, Object?> json) {
    if (hire['status'] != 'hired') throw _refusal(409, 'hire_not_active');
    if (json.containsKey('name')) {
      final name = (json['name'] as String).trim();
      final length = name.runes.length;
      final clean = !name.runes.any((rune) => rune < 0x20 || rune == 0x7f);
      if (length < 1 || length > 40 || !clean) {
        throw _refusal(422, 'invalid_name', field: 'name');
      }
      hire['name'] = name;
    }
    if (json.containsKey('avatar') && json['avatar'] != null) {
      final avatar = json['avatar'] as String;
      if (!_characters.contains(avatar)) {
        throw _refusal(422, 'invalid_field', field: 'avatar');
      }
      hire['avatar'] = avatar;
    }
  }

  Map<String, Object?>? _find(String id) {
    for (final hire in hires) {
      if (hire['id'] == id) return hire;
    }
    return null;
  }

  GatewayError _refusal(int status, String? code, {String? field}) {
    final detail = <String, Object?>{'message': 'x'};
    if (code != null) detail['code'] = code;
    if (field != null) detail['field'] = field;
    return GatewayError(status, jsonEncode({'detail': detail}));
  }
}

const _characters = [
  'bob',
  'conan',
  'dani',
  'ernesto',
  'hugo',
  'ines',
  'nicole',
  'sami',
];

/// The catalogue the team tests use: video (Vera, Jade) and programación
/// (Bob, Ernesto). Bob has his character; the others have none.
const Map<String, Object?> teamCatalog = {
  'ok': true,
  'version': 1,
  'areas': [
    {
      'id': 'video',
      'label': 'Vídeo y postproducción',
      'roles': ['vera', 'jade'],
      'levels': {
        'personal': ['vera'],
        'profesional': ['vera'],
        'equipo': ['vera', 'jade'],
      },
    },
    {
      'id': 'programacion',
      'label': 'Programación',
      'roles': ['bob', 'ernesto'],
      'levels': {
        'personal': ['bob'],
        'profesional': ['bob'],
        'equipo': ['bob', 'ernesto'],
      },
    },
  ],
  'roles': [
    {
      'id': 'vera',
      'name': 'Vera',
      'title': 'Editor de vídeo',
      'area': 'video',
      'avatar': null,
    },
    {
      'id': 'jade',
      'name': 'Jade',
      'title': 'Colorista',
      'area': 'video',
      'avatar': null,
    },
    {
      'id': 'bob',
      'name': 'Bob',
      'title': 'Desarrollador',
      'area': 'programacion',
      'avatar': 'bob',
    },
    {
      'id': 'ernesto',
      'name': 'Ernesto',
      'title': 'Líder de proyecto',
      'area': 'programacion',
      'avatar': 'ernesto',
    },
  ],
};

Map<String, Object?>? teamRole(String id) {
  for (final role in teamCatalog['roles']! as List<Object?>) {
    final map = role! as Map<String, Object?>;
    if (map['id'] == id) return map;
  }
  return null;
}

/// One contracted Bob with his character, as the list reads him.
Map<String, Object?> bobHire({
  String id = 'hire_1',
  String name = 'Bob',
  String? avatar = 'bob',
}) => {
  'id': id,
  'role': 'bob',
  'area': 'programacion',
  'name': name,
  'avatar': avatar,
  'status': 'hired',
  'created_at': '2026-10-25T10:00:00.000Z',
};
