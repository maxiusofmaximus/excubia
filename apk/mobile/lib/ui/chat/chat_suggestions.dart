import 'package:excubia/src/rust/api/onboarding/model.dart';

/// One suggestion on the empty chat: [text] is what gets sent, [detail] says why.
class ChatSuggestion {
  const ChatSuggestion({required this.text, required this.detail});

  final String text;
  final String detail;
}

typedef _AreaLine = ({String label, String line});

/// The catalogue areas, each with its label and one concrete line for that trade.
const _areas = <String, _AreaLine>{
  'programacion': (
    label: 'Programación',
    line: 'Revisa que todo funcione antes de entregar',
  ),
  'video': (label: 'Vídeo', line: 'Revisa el montaje y dime qué falta'),
  'diseno_grafico': (
    label: 'Diseño gráfico',
    line: 'Propón tres ideas para la portada',
  ),
  'audio': (label: 'Audio', line: 'Revisa el audio y dime qué ajustar'),
  'documentos': (
    label: 'Documentos',
    line: 'Prepara un informe de esta semana',
  ),
  'investigacion': (
    label: 'Investigación',
    line: 'Busca fuentes fiables sobre este tema',
  ),
  'ingenieria_cad': (
    label: 'Ingeniería CAD',
    line: 'Revisa las medidas del plano',
  ),
  'marketing': (
    label: 'Marketing',
    line: 'Redacta el plan de publicaciones de la semana',
  ),
  'otras': (
    label: 'Otros trabajos',
    line: 'Ayúdame a ordenar las tareas de hoy',
  ),
};

const _general = ChatSuggestion(
  text: '¿Cómo va el equipo?',
  detail: 'qué hace cada persona y qué está pendiente',
);
const _tellMe = ChatSuggestion(
  text: 'Cuéntame a qué te dedicas y armo tu equipo',
  detail: 'Te propongo personas para tu trabajo',
);
const _whatCanYouDo = ChatSuggestion(
  text: '¿Qué puedes hacer por mí?',
  detail: 'Te cuento lo que sé hacer',
);
const _note = ChatSuggestion(
  text: 'Apunta una nota para mañana',
  detail: 'Queda guardada para que no se te olvide',
);

/// The three suggestions for the empty chat. Without a team: neutral starters.
/// With a team: the general one first, then up to two lines from the areas
/// the team covers (those the catalogue knows, in the order they were hired),
/// rotated by [rotation] so the same ones do not always come first. Any slot
/// left over takes a neutral line.
List<ChatSuggestion> chatSuggestions({
  required List<Hire> hires,
  int rotation = 0,
}) {
  if (hires.isEmpty) return const [_tellMe, _whatCanYouDo, _note];
  final lines = <String, _AreaLine>{};
  for (final hire in hires) {
    final line = _areas[hire.area];
    if (line != null) lines.putIfAbsent(hire.area, () => line);
  }
  final covered = lines.values.toList();
  final picks = [
    for (var i = 0; i < covered.length && i < 2; i++)
      _areaSuggestion(covered[(rotation + i) % covered.length]),
  ];
  return [
    _general,
    ...picks,
    ...const [_note, _whatCanYouDo].take(2 - picks.length),
  ];
}

ChatSuggestion _areaSuggestion(_AreaLine area) =>
    ChatSuggestion(text: area.line, detail: area.label);
