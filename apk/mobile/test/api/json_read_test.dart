import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/json_read.dart';

/// The host's JSON as it arrives, so an int and a double stay the types the wire gives.
Map<String, Object?> wire(String json) =>
    jsonDecode(json) as Map<String, Object?>;

void main() {
  group('texto requerido', () {
    test('un texto se lee; ausente, null, número o lista lanzan', () {
      expect(jsonText(wire('{"k":"hola"}'), 'k'), 'hola');
      for (final json in ['{}', '{"k":null}', '{"k":1}', '{"k":[]}']) {
        expect(
          () => jsonText(wire(json), 'k'),
          throwsFormatException,
          reason: json,
        );
      }
    });
  });

  group('texto opcional', () {
    test('un texto se lee; ausente, null o de otro tipo dan null', () {
      expect(jsonOptionalText(wire('{"k":"hola"}'), 'k'), 'hola');
      for (final json in ['{}', '{"k":null}', '{"k":1}', '{"k":[]}']) {
        expect(jsonOptionalText(wire(json), 'k'), isNull, reason: json);
      }
    });
  });

  group('entero', () {
    test('solo un entero JSON vale; un double o un texto lanzan', () {
      expect(jsonInt(wire('{"k":3}'), 'k'), 3);
      for (final json in ['{}', '{"k":null}', '{"k":"3"}', '{"k":1.0}']) {
        expect(
          () => jsonInt(wire(json), 'k'),
          throwsFormatException,
          reason: json,
        );
      }
    });
  });

  group('lista', () {
    test('una lista se lee tal cual; ausente o de otro tipo lanza', () {
      expect(jsonList(wire('{"k":[1,"a"]}'), 'k'), [1, 'a']);
      for (final json in ['{}', '{"k":null}', '{"k":{}}', '{"k":"a"}']) {
        expect(
          () => jsonList(wire(json), 'k'),
          throwsFormatException,
          reason: json,
        );
      }
    });
  });

  group('textos de una lista', () {
    test('se saltan los elementos que no son texto', () {
      expect(jsonStrings(wire('{"k":["a",1,null,"b",{}]}'), 'k'), ['a', 'b']);
      expect(jsonStrings(wire('{"k":[]}'), 'k'), isEmpty);
    });

    test('la lista ausente, null o de otro tipo lanza', () {
      for (final json in ['{}', '{"k":null}', '{"k":"a"}']) {
        expect(
          () => jsonStrings(wire(json), 'k'),
          throwsFormatException,
          reason: json,
        );
      }
    });
  });

  group('objeto', () {
    test('un objeto se devuelve; cualquier otro valor lanza', () {
      expect(jsonObject(jsonDecode('{"a":1}')), {'a': 1});
      for (final json in ['null', '[]', '"x"', '1']) {
        expect(
          () => jsonObject(jsonDecode(json)),
          throwsFormatException,
          reason: json,
        );
      }
    });

    test('un objeto opcional: cualquier valor que no sea objeto da vacío', () {
      expect(jsonObjectOrEmpty(jsonDecode('{"a":1}')), {'a': 1});
      for (final json in ['null', '[]', '"x"', '1']) {
        expect(jsonObjectOrEmpty(jsonDecode(json)), isEmpty, reason: json);
      }
    });
  });

  group('sobre {ok: true}', () {
    test('solo ok exactamente true pasa; el resto es una respuesta rota', () {
      expect(jsonEnvelope('{"ok":true,"x":1}')['x'], 1);
      for (final body in [
        '{"ok":false}',
        '{"ok":"true"}',
        '{"x":1}',
        '[]',
        '"ok"',
        'no es json',
      ]) {
        expect(() => jsonEnvelope(body), throwsFormatException, reason: body);
      }
    });
  });
}
