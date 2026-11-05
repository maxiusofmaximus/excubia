import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/onboarding/model.dart';
import 'package:herdr_notify/ui/chat/chat_suggestions.dart';

const allAreas = [
  'programacion',
  'video',
  'diseno_grafico',
  'audio',
  'documentos',
  'investigacion',
  'ingenieria_cad',
  'marketing',
  'otras',
];

Hire hireIn(String area, {String id = 'h1'}) => Hire(
  id: id,
  role: 'rol_$area',
  area: area,
  name: 'Persona',
  status: 'hired',
  createdAt: '2026-11-05T10:00:00.000Z',
);

void main() {
  test('sin equipo contratado: tres frases neutras de arranque', () {
    final suggestions = chatSuggestions(hires: const []);

    expect(suggestions.map((s) => s.text), [
      'Cuéntame a qué te dedicas y armo tu equipo',
      '¿Qué puedes hacer por mí?',
      'Apunta una nota para mañana',
    ]);
  });

  test('con equipo, la primera es siempre la general del equipo', () {
    final suggestions = chatSuggestions(hires: [hireIn('documentos')]);

    expect(suggestions.first.text, '¿Cómo va el equipo?');
    expect(
      suggestions.first.detail,
      'qué hace cada persona y qué está pendiente',
    );
  });

  test('un área contratada aporta su frase de oficio', () {
    final suggestions = chatSuggestions(hires: [hireIn('documentos')]);

    expect(suggestions.map((s) => s.text), [
      '¿Cómo va el equipo?',
      'Prepara un informe de esta semana',
      'Apunta una nota para mañana',
    ]);
    expect(suggestions[1].detail, 'Documentos');
  });

  test('varias áreas: dos frases, en el orden en que llegó el equipo', () {
    final suggestions = chatSuggestions(
      hires: [
        hireIn('video', id: 'a'),
        hireIn('documentos', id: 'b'),
      ],
    );

    expect(suggestions.map((s) => s.text), [
      '¿Cómo va el equipo?',
      'Revisa el montaje y dime qué falta',
      'Prepara un informe de esta semana',
    ]);
  });

  test('con varias áreas, la rotación cambia cuáles salen', () {
    final hires = [hireIn('video', id: 'a'), hireIn('documentos', id: 'b')];

    final turnOne = chatSuggestions(hires: hires, rotation: 1);

    expect(turnOne.map((s) => s.text), [
      '¿Cómo va el equipo?',
      'Prepara un informe de esta semana',
      'Revisa el montaje y dime qué falta',
    ]);
  });

  test('dos personas de la misma área no repiten la frase', () {
    final suggestions = chatSuggestions(
      hires: [
        hireIn('video', id: 'a'),
        hireIn('video', id: 'b'),
      ],
    );

    expect(suggestions.where((s) => s.detail == 'Vídeo'), hasLength(1));
  });

  test('un área fuera del catálogo se ignora', () {
    final suggestions = chatSuggestions(hires: [hireIn('area_fuera_catalogo')]);

    expect(suggestions.map((s) => s.text), [
      '¿Cómo va el equipo?',
      'Apunta una nota para mañana',
      '¿Qué puedes hacer por mí?',
    ]);
  });

  test('nunca devuelve más de tres', () {
    final suggestions = chatSuggestions(
      hires: [for (final area in allAreas) hireIn(area, id: area)],
      rotation: 3,
    );

    expect(suggestions, hasLength(3));
  });

  test('ninguna frase lleva jerga ni nombres de agentes', () {
    final banned = RegExp(
      r'\b(lead|host|e2e|suite|demo|ernesto|ines|bob|alfred)\b',
      caseSensitive: false,
    );
    final everything = [
      ...chatSuggestions(hires: const []),
      for (var rotation = 0; rotation < 2; rotation++)
        ...chatSuggestions(
          hires: [for (final area in allAreas) hireIn(area, id: area)],
          rotation: rotation,
        ),
    ];

    for (final suggestion in everything) {
      expect(
        banned.hasMatch(suggestion.text),
        isFalse,
        reason: suggestion.text,
      );
      expect(
        banned.hasMatch(suggestion.detail),
        isFalse,
        reason: suggestion.detail,
      );
    }
  });
}
