import 'dart:async';

import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/state.dart';

import '../theme.dart';

/// The clips an avatar plays. The body stays still in every clip and only the
/// head acts. `idle`, `working`, `done` and `error` follow an agent's status;
/// `thinking` is Alfred waiting for a reply and `look` is his occasional glance
/// on the welcome screen.
enum AgentLook { idle, look, thinking, working, done, error }

/// Name and accent colours of a rendered agent. `background` is the light
/// disc from `avatars/assets/agents/agents.json`; `foreground` is its dark
/// pair, used for the fallback initial.
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

const List<AgentProfile> _agentProfiles = [
  AgentProfile(
    id: 'alfred',
    background: Color(0xFFF7D27A),
    foreground: Color(0xFFB7791F),
  ),
  AgentProfile(
    id: 'bob',
    background: Color(0xFF9EE6D4),
    foreground: Color(0xFF0F7F6C),
  ),
  AgentProfile(
    id: 'nicole',
    background: Color(0xFFCDB9FF),
    foreground: Color(0xFF6D3FD1),
  ),
  AgentProfile(
    id: 'ines',
    background: Color(0xFFFFB3A8),
    foreground: Color(0xFFC2413A),
  ),
  AgentProfile(
    id: 'hugo',
    background: Color(0xFFA9CBFF),
    foreground: Color(0xFF1E5DB3),
  ),
  AgentProfile(
    id: 'sami',
    background: Color(0xFFCDEB9A),
    foreground: Color(0xFF4D7F12),
  ),
  AgentProfile(
    id: 'dani',
    background: Color(0xFFFFB9DA),
    foreground: Color(0xFFB0286A),
  ),
  AgentProfile(
    id: 'conan',
    background: Color(0xFFC9D2DE),
    foreground: Color(0xFF334155),
  ),
  AgentProfile(
    id: 'ernesto',
    background: Color(0xFFE7C9A8),
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

/// Sizes up to 64 dp load the 64 px render, larger ones the 128 px render.
String _renderSize(double size) => size <= 64 ? '64' : '128';

/// The still for [look] at [size]. Clips without a still of their own (idle,
/// look, thinking) use the neutral one, which is what every agent showed before
/// the animations existed.
String _stillAsset(AgentProfile profile, AgentLook look, double size) {
  final name = switch (look) {
    AgentLook.working => 'working',
    AgentLook.done => 'done',
    AgentLook.error => 'error',
    AgentLook.idle || AgentLook.look || AgentLook.thinking => 'neutral',
  };
  return 'assets/agents/${profile.id}-$name-${_renderSize(size)}.png';
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

/// The animated head for [look] at [size], one WebP per character and clip.
String _clipAsset(AgentProfile profile, AgentLook look, double size) =>
    'assets/agents_anim/${profile.id}/${look.name}-head-${_renderSize(size)}.webp';

/// How long Alfred's glance lasts. One pass of the `look` clip is about 3 s.
const _glanceLength = Duration(seconds: 3);

/// How often Alfred's welcome screen glances.
const _glanceInterval = Duration(seconds: 20);

/// The name a row should show for a live agent: the seat label in the middle
/// of the pane title ("desconocido · Ana · especialista"), else the CLI id.
String agentNameOf(AgentEntry agent) {
  final segments = (agent.title ?? '')
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

/// A square avatar that fills its circle. Agents with a render keep their own
/// face; anyone else gets their initial over their accent colour, never the
/// generic robot. List rows pass [ringed] for the 2 px outline
/// that keeps the disc visible on light paper.
///
/// The face comes from [characterId] when it is given, so a renamed hire keeps
/// it; without one the visible [name] is matched, as before.
///
/// The head animates unless [animated] is off or the system asks to reduce
/// motion; then, or when a clip is missing, the still is shown instead.
class AgentAvatar extends StatefulWidget {
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
  });

  /// The visible name: the label of the avatar, and the initial when there is
  /// no face.
  final String name;

  /// The character id of the face (`alfred`, `bob`…), not a name.
  final String? characterId;
  final AgentStatus status;

  /// Plays this clip whatever [status] says, e.g. Alfred's `thinking` while
  /// his reply is pending.
  final AgentLook? look;
  final double size;
  final bool ringed;

  /// Turns the animation off for this avatar alone.
  final bool animated;

  /// Welcome screens only. Read once when the avatar is created: every ~20 s
  /// the head plays one `look` and goes back to idle. Ignored while [look] is
  /// set.
  final bool glance;

  /// Without a [characterId], match the face by [name] (the agents list). A
  /// contracted hire turns this off: no character means its initial.
  final bool faceFromName;

  @override
  State<AgentAvatar> createState() => _AgentAvatarState();
}

class _AgentAvatarState extends State<AgentAvatar> {
  Timer? _glanceTimer;
  Timer? _glanceEndTimer;
  bool _glancing = false;

  @override
  void initState() {
    super.initState();
    if (widget.glance) {
      _glanceTimer = Timer.periodic(_glanceInterval, (_) => _startGlance());
    }
  }

  @override
  void dispose() {
    _glanceTimer?.cancel();
    _glanceEndTimer?.cancel();
    super.dispose();
  }

  /// The head may move: the avatar is animated and the system has not asked to
  /// reduce motion.
  bool get _motion =>
      widget.animated && !MediaQuery.disableAnimationsOf(context);

  void _startGlance() {
    if (!mounted || !_motion) return;
    setState(() => _glancing = true);
    _glanceEndTimer = Timer(_glanceLength, () {
      if (mounted) setState(() => _glancing = false);
    });
  }

  AgentLook _lookFor() {
    final forced = widget.look;
    if (forced != null) return forced;
    if (_glancing && _motion) return AgentLook.look;
    return agentLookFor(widget.status);
  }

  @override
  Widget build(BuildContext context) {
    final size = widget.size;
    final id = widget.characterId;
    final profile = id != null
        ? agentProfileById(id)
        : widget.faceFromName
        ? agentProfileFor(widget.name)
        : null;
    final look = _lookFor();
    final fallback = _Initial(
      initial: _initial(widget.name),
      color: profile?.foreground ?? context.colors.accentText,
      fontSize: size * 0.44,
    );
    Widget art = fallback;
    if (profile != null) {
      final still = _head(_stillAsset(profile, look, size), fallback);
      art = _motion ? _head(_clipAsset(profile, look, size), still) : still;
    }
    final disc = ClipOval(
      child: ColoredBox(
        color: profile?.background ?? context.colors.accentSoft,
        child: art,
      ),
    );
    return Semantics(
      image: true,
      label: widget.name,
      excludeSemantics: true,
      child: SizedBox.square(
        dimension: size,
        child: widget.ringed
            ? DecoratedBox(
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(
                    color: context.colors.outlineStrong,
                    width: 2,
                  ),
                ),
                child: Padding(padding: const EdgeInsets.all(2), child: disc),
              )
            : disc,
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
