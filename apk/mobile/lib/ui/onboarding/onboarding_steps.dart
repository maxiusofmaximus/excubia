import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/onboarding/model.dart';

import '../../core/state/onboarding_controller.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import 'onboarding_parts.dart';

/// Step 0: Alfred introduces the assistant.
class WelcomeStep extends StatelessWidget {
  const WelcomeStep({super.key});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Center(
          child: AgentAvatar(
            name: 'Alfred',
            characterId: 'alfred',
            size: 128,
            glance: true,
          ),
        ),
        const SizedBox(height: 24),
        const OnboardingHeading(title: '¿Para qué lo vas a usar?'),
        Text(
          'Voy a montar tu equipo. Dime para qué lo vas a usar y te propongo a quién contratar; luego puedes cambiar todo.',
          style: Theme.of(context).textTheme.bodyLarge?.copyWith(
            color: context.colors.textSecondary,
          ),
        ),
      ],
    );
  }
}

/// Step 1: the areas of work, several at once.
class AreasStep extends StatelessWidget {
  const AreasStep({super.key, required this.controller});

  final OnboardingController controller;

  @override
  Widget build(BuildContext context) {
    final areas = controller.catalog?.areas ?? const <RoleArea>[];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const OnboardingHeading(
          title: '¿En qué trabajas?',
          subtitle: 'Elige una o varias áreas.',
        ),
        for (final area in areas)
          OnboardingChoice(
            label: area.label,
            icon: areaIcon(area.id),
            selected: controller.isAreaChosen(area.id),
            onTap: () => controller.toggleArea(area.id),
          ),
      ],
    );
  }
}

/// Step 2: how the phone will be used. Optional: «Saltar» keeps «Profesional».
class LevelStep extends StatelessWidget {
  const LevelStep({super.key, required this.controller});

  final OnboardingController controller;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const OnboardingHeading(title: '¿Cómo lo vas a usar?'),
        for (final level in OnboardingLevel.values)
          OnboardingChoice(
            label: levelLabel(level),
            selected: controller.level == level,
            exclusive: true,
            onTap: () => controller.chooseLevel(level),
          ),
      ],
    );
  }
}

/// The Material icon of each area of the catalogue. An unknown area gets a
/// generic one, so a new area on the host still shows.
IconData areaIcon(String areaId) =>
    _areaIcons[areaId] ?? Icons.category_outlined;

const Map<String, IconData> _areaIcons = {
  'programacion': Icons.code,
  'video': Icons.movie_outlined,
  'diseno_grafico': Icons.brush_outlined,
  'audio': Icons.music_note_outlined,
  'documentos': Icons.description_outlined,
  'investigacion': Icons.travel_explore,
  'ingenieria_cad': Icons.architecture,
  'marketing': Icons.campaign_outlined,
  'otras': Icons.more_horiz,
};

String levelLabel(OnboardingLevel level) => switch (level) {
  OnboardingLevel.personal => 'Personal',
  OnboardingLevel.profesional => 'Profesional',
  OnboardingLevel.equipo => 'Equipo',
};
