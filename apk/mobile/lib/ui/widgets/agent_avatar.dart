import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/state.dart';

import '../theme.dart';
import 'comic/comic_portrait.dart';

/// The looks an avatar shows. v3 has one still per status, so `idle` and `look`
/// both show the neutral face; `thinking` is Alfred waiting for a reply.
enum AgentLook { idle, look, thinking, working, done, error }

/// Name and colours of a rendered agent, copied from the v3 manifest. The
/// `background` is the accent that the render's panel is painted in; the
/// `foreground` is its deep tone from the same manifest.
@immutable
class AgentProfile {
  const AgentProfile({
    required this.id,
    required this.background,
    required this.foreground,
  });

  final String id;
  final Color background;
  final Color foreground;
}

/// The ink of an initial drawn over an accent disc: the night colour of the
/// design. The deep tone misses 4.5:1 on the lighter accents (Ernesto 3.2:1).
const Color agentInitialInk = Color(0xFF0B1120);

const List<AgentProfile> _agentProfiles = [
  AgentProfile(
    id: 'alfred',
    background: Color(0xFFF2A93B),
    foreground: Color(0xFFB7791F),
  ),
  AgentProfile(
    id: 'bob',
    background: Color(0xFF3CC9A8),
    foreground: Color(0xFF0F7F6C),
  ),
  AgentProfile(
    id: 'nicole',
    background: Color(0xFF9B7BFF),
    foreground: Color(0xFF6D3FD1),
  ),
  AgentProfile(
    id: 'ines',
    background: Color(0xFFFF7A6B),
    foreground: Color(0xFFC2413A),
  ),
  AgentProfile(
    id: 'hugo',
    background: Color(0xFF5B9BFF),
    foreground: Color(0xFF1E5DB3),
  ),
  AgentProfile(
    id: 'sami',
    background: Color(0xFF9BD34A),
    foreground: Color(0xFF4D7F12),
  ),
  AgentProfile(
    id: 'dani',
    background: Color(0xFFFF6FAE),
    foreground: Color(0xFFB0286A),
  ),
  AgentProfile(
    id: 'conan',
    background: Color(0xFF8FA3BD),
    foreground: Color(0xFF334155),
  ),
  AgentProfile(
    id: 'ernesto',
    background: Color(0xFFE09A5F),
    foreground: Color(0xFF8A3E1F),
  ),
];

/// Finds the rendered agent behind a visible name. Comparsion ignores accents
/// and case, and accepts a decorated name such as «Alfred (lead)».
AgentProfile? agentProfileFor(String name) {
  final folded = _fold(name);
  for (final profile in _agentProfiles) {
    if (folded == profile.id || folded.startsWith('${profile.id} ')) {
      return profile;
    }
  }
  return null;
}

/// The rendered agent with this character id, whatever it is called now.
AgentProfile? agentProfileById(String id) {
  for (final profile in _agentProfiles) {
    if (profile.id == id) return profile;
  }
  return null;
}

AgentLook agentLookFor(AgentStatus status) => switch (status) {
  AgentStatus.working => AgentLook.working,
  AgentStatus.done => AgentLook.done,
  AgentStatus.blocked => AgentLook.error,
  AgentStatus.idle || AgentStatus.unknown => AgentLook.idle,
};

/// Under 48 dp the 128 px render; from 48 dp up the 384 px one, which stays sharp at 3×.
String _renderSize(double size) => size < 48 ? '128' : '384';

/// The square still for [look] at [size]. Each look has its own still, except
/// `idle` and `look`, which both show the neutral one.
String stillAssetFor(AgentProfile profile, AgentLook look, double size) {
  final state = switch (look) {
    AgentLook.working => 'working',
    AgentLook.done => 'done',
    AgentLook.error => 'error',
    AgentLook.thinking => 'thinking',
    AgentLook.idle || AgentLook.look => 'neutral',
  };
  return 'assets/portraits/${profile.id}-$state-${_renderSize(size)}.webp';
}

/// One head image. A file that cannot be decoded shows [onError] instead.
Image _head(String asset, Widget onError) => Image.asset(
  asset,
  fit: BoxFit.cover,
  filterQuality: FilterQuality.medium,
  gaplessPlayback: true,
  excludeFromSemantics: true,
  errorBuilder: (_, _, _) => onError,
);

/// How long a head takes to change from one state to the next.
const _fade = Duration(milliseconds: 150);

/// The host's role tag at the end of a pane title («Ernesto — lead»). A copy of
/// the one in agent_text.dart, which imports this file and cannot be imported back.
final _roleTagTitle = RegExp(
  r'\s+[—-]\s+(lead|specialist|especialista)$',
  caseSensitive: false,
);

/// The name a row should show for a live agent: the seat label in the middle
/// of the pane title ("desconocido · Ana · especialista"), else the character
/// the pane is titled after ("Ernesto — lead"), else the CLI id.
String agentNameOf(AgentEntry agent) {
  final title = agent.title ?? '';
  final segments = title
      .split('·')
      .map((segment) => segment.trim())
      .where((segment) => segment.isNotEmpty)
      .toList();
  if (segments.length >= 2) {
    final candidate = segments[1];
    final normalized = candidate.toLowerCase();
    if (normalized != 'desconocido' && normalized != 'unknown') {
      return candidate;
    }
  }
  // The profile's name, not the title: a title can be a whole task that
  // starts with a character's name.
  final profile = agentProfileFor(title.replaceFirst(_roleTagTitle, '').trim());
  if (profile != null) return capitalized(profile.id);
  final raw = agent.agent.trim();
  if (raw.isEmpty ||
      raw.toLowerCase() == 'unknown' ||
      raw.toLowerCase() == 'desconocido') {
    return 'Especialista';
  }
  return capitalized(raw);
}

/// A character or CLI id shown as a name: "bob" → "Bob".
String capitalized(String text) => text[0].toUpperCase() + text.substring(1);

/// A square avatar, never round. Agents with a render keep their own face;
/// anyone else gets their initial over their accent colour, never the generic
/// robot. List rows pass [ringed] for the 2 px outline that keeps the square
/// visible on light paper.
///
/// The face comes from [characterId] when it is given, so a renamed hire keeps
/// it; without one the visible [name] is matched, as before.
///
/// The face changes with the status through a short fade. The fade is off when
/// [animated] is off or the system asks to reduce motion; then the new still
/// simply appears. A still that is missing shows the initial.
class AgentAvatar extends StatelessWidget {
  const AgentAvatar({
    super.key,
    required this.name,
    this.characterId,
    this.status = AgentStatus.idle,
    this.look,
    this.size = 40,
    this.ringed = false,
    this.animated = true,
    this.glance = false,
    this.faceFromName = true,
    this.framed = false,
  });

  /// The comic frame instead of the disc: a straight panel with the head rising
  /// above it. Used in the chat, where the character is the protagonist.
  final bool framed;

  /// The visible name: the label of the avatar, and the initial when there is
  /// no face.
  final String name;

  /// The character id of the face (`alfred`, `bob`…), not a name.
  final String? characterId;
  final AgentStatus status;

  /// Shows this look whatever [status] says, e.g. Alfred's `thinking` while
  /// his reply is pending.
  final AgentLook? look;
  final double size;
  final bool ringed;

  /// Turns the fade off for this avatar alone.
  final bool animated;

  /// Kept so the welcome screens compile unchanged. The v3 stills have no
  /// glance, so this does nothing.
  final bool glance;

  /// Without a [characterId], match the face by [name] (the agents list). A
  /// contracted hire turns this off: no character means its initial.
  final bool faceFromName;

  @override
  Widget build(BuildContext context) {
    final id = characterId;
    final profile = id != null
        ? agentProfileById(id)
        : faceFromName
        ? agentProfileFor(name)
        : null;
    final motion = animated && !MediaQuery.disableAnimationsOf(context);
    final fallback = _Initial(
      initial: _initial(name),
      color: profile == null ? context.colors.accentText : agentInitialInk,
      fontSize: size * 0.44,
    );
    Widget art = fallback;
    if (profile != null) {
      final asset = stillAssetFor(profile, look ?? agentLookFor(status), size);
      art = AnimatedSwitcher(
        duration: motion ? _fade : Duration.zero,
        child: KeyedSubtree(
          key: ValueKey(asset),
          child: _head(asset, fallback),
        ),
      );
    }
    if (framed) {
      return Semantics(
        image: true,
        label: name,
        excludeSemantics: true,
        child: ComicPortrait(
          size: size,
          panel: profile?.background ?? context.colors.accentSoft,
          face: art,
        ),
      );
    }
    final square = ColoredBox(
      color: profile?.background ?? context.colors.accentSoft,
      child: art,
    );
    return Semantics(
      image: true,
      label: name,
      excludeSemantics: true,
      child: SizedBox.square(
        dimension: size,
        child: ringed
            ? DecoratedBox(
                decoration: BoxDecoration(
                  border: Border.all(
                    color: context.colors.outlineStrong,
                    width: 2,
                  ),
                ),
                child: Padding(padding: const EdgeInsets.all(2), child: square),
              )
            : square,
      ),
    );
  }
}

class _Initial extends StatelessWidget {
  const _Initial({
    required this.initial,
    required this.color,
    required this.fontSize,
  });

  final String initial;
  final Color color;
  final double fontSize;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Text(
        initial,
        style: TextStyle(
          color: color,
          fontSize: fontSize,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}

String _initial(String name) {
  final trimmed = name.trim();
  return trimmed.isEmpty ? '?' : trimmed[0].toUpperCase();
}

String _fold(String value) {
  const accents = {
    'á': 'a',
    'é': 'e',
    'í': 'i',
    'ó': 'o',
    'ú': 'u',
    'ü': 'u',
    'ñ': 'n',
  };
  final lowered = value.trim().toLowerCase();
  final buffer = StringBuffer();
  for (final rune in lowered.runes) {
    final char = String.fromCharCode(rune);
    buffer.write(accents[char] ?? char);
  }
  return buffer.toString();
}
