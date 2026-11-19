import 'package:flutter/material.dart';

import '../theme.dart';
import 'agent_avatar.dart';
import 'blinking_portrait.dart';

/// A character's portrait card: the 3:4 render, whose panel and monogram are
/// part of the picture, then the name in large type and a short line. It is a
/// button when [onTap] is set. Size it from outside (SizedBox or the parent).
class AgentPortraitCard extends StatelessWidget {
  const AgentPortraitCard({
    super.key,
    required this.id,
    required this.name,
    this.tagline,
    this.onTap,
    this.centered = false,
  });

  /// The character id (`alfred`, `bob`…), which picks the portrait.
  final String id;
  final String name;
  final String? tagline;
  final VoidCallback? onTap;

  /// Name and line on the portrait's centre line, for a welcome; left otherwise.
  final bool centered;

  static const _radius = BorderRadius.all(Radius.circular(28));

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final profile = agentProfileById(id);
    final initial = Center(
      child: Text(
        name.trim().isEmpty ? '?' : name.trim()[0].toUpperCase(),
        style: TextStyle(
          color: profile == null ? c.accentText : agentInitialInk,
          fontSize: 96,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
    final portrait = profile == null
        ? ColoredBox(color: c.accentSoft, child: initial)
        : BlinkingPortrait(
            base: 'assets/agents_v3/portraits/$id-neutral-600x750.webp',
            blink: 'assets/agents_v3/portraits/$id-blink-600x750.webp',
            enabled: id == 'alfred',
            fallback: ColoredBox(color: profile.background, child: initial),
          );
    final content = Column(
      crossAxisAlignment: centered
          ? CrossAxisAlignment.center
          : CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        AspectRatio(
          aspectRatio: 3 / 4,
          child: ClipRRect(borderRadius: _radius, child: portrait),
        ),
        const SizedBox(height: Spacing.md),
        Text(
          name,
          style: context.text.headline,
          textAlign: centered ? TextAlign.center : null,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        if (tagline != null)
          Text(
            tagline!,
            style: context.text.bodySecondary,
            textAlign: centered ? TextAlign.center : null,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
          ),
      ],
    );
    return Semantics(
      button: onTap != null,
      onTap: onTap,
      label: tagline == null ? name : '$name. $tagline',
      excludeSemantics: true,
      child: onTap == null
          ? content
          : Material(
              color: Colors.transparent,
              child: InkWell(
                borderRadius: _radius,
                onTap: onTap,
                child: content,
              ),
            ),
    );
  }
}
