import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/onboarding/model.dart';

import '../theme.dart';
import '../widgets/agent_portrait_card.dart';

/// The contracted people as a swipeable row of portraits, above the list. A tap
/// opens the person's sheet, the same as their row does.
class TeamPortraitRow extends StatelessWidget {
  const TeamPortraitRow({super.key, required this.hires, required this.onOpen});

  final List<Hire> hires;

  /// Receives the tapped person.
  final ValueChanged<Hire> onOpen;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        // Two cards and a bit of the third show, so the row reads as swipeable.
        final width = constraints.maxWidth * 0.42;
        // The 3:4 portrait, the name line and its gap; text scale gets the rest.
        final height = width * 4 / 3 + 80;
        return SizedBox(
          height: height,
          child: ListView.separated(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.symmetric(horizontal: Spacing.lg),
            itemCount: hires.length,
            separatorBuilder: (_, _) => const SizedBox(width: Spacing.md),
            itemBuilder: (context, index) {
              final hire = hires[index];
              return SizedBox(
                width: width,
                child: AgentPortraitCard(
                  id: hire.avatar ?? '',
                  name: hire.name,
                  onTap: () => onOpen(hire),
                ),
              );
            },
          ),
        );
      },
    );
  }
}
