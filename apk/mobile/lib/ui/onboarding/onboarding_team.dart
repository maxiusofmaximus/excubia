import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/onboarding/model.dart';

import '../../core/state/onboarding_controller.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import 'onboarding_parts.dart';

/// Step 3: the people to contract. The level has preselected them; the user
/// adds or removes, and Alfred is always there.
class TeamStep extends StatelessWidget {
  const TeamStep({super.key, required this.controller});

  final OnboardingController controller;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const OnboardingHeading(
          title: 'Este es tu equipo',
          subtitle: 'Quita o añade personas; Alfred siempre está.',
        ),
        const _AlfredRow(),
        for (final area in controller.chosenAreas)
          _AreaTeam(controller: controller, area: area),
      ],
    );
  }
}

class _AlfredRow extends StatelessWidget {
  const _AlfredRow();

  @override
  Widget build(BuildContext context) {
    return const CheckboxListTile(
      value: true,
      onChanged: null,
      secondary: AgentAvatar(name: 'Alfred', characterId: 'alfred'),
      title: Text('Alfred'),
      subtitle: Text('Director técnico'),
      controlAffinity: ListTileControlAffinity.trailing,
    );
  }
}

class _AreaTeam extends StatelessWidget {
  const _AreaTeam({required this.controller, required this.area});

  final OnboardingController controller;
  final RoleArea area;

  @override
  Widget build(BuildContext context) {
    final roles = controller.rolesIn(area);
    final text = Theme.of(context).textTheme;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(top: 20, bottom: 4),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  area.label,
                  style: text.titleMedium?.copyWith(
                    color: context.colors.textPrimary,
                  ),
                ),
              ),
              Text(
                '${controller.selectedIn(area.id)}/${OnboardingController.maxPerArea}',
                style: text.bodyMedium?.copyWith(
                  color: context.colors.textSecondary,
                ),
              ),
            ],
          ),
        ),
        if (roles.isEmpty)
          Text(
            'Sin equipo propuesto para esta área.',
            style: text.bodyMedium?.copyWith(
              color: context.colors.textSecondary,
            ),
          ),
        for (final role in roles)
          CheckboxListTile(
            value: controller.hasRole(role.id),
            onChanged: (_) {
              // A SnackBar, not a notice up in the area header: the header may be
              // scrolled out of sight by the time the ninth person is tapped.
              if (!controller.toggleRole(role)) _sayLimit(context);
            },
            secondary: AgentAvatar(name: role.name, characterId: role.avatar),
            title: Text(role.name),
            subtitle: Text(role.title),
            controlAffinity: ListTileControlAffinity.trailing,
          ),
      ],
    );
  }

  void _sayLimit(BuildContext context) {
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text('Máximo ${OnboardingController.maxPerArea} por área'),
      ),
    );
  }
}
