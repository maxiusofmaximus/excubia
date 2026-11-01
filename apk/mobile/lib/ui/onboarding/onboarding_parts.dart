import 'package:flutter/material.dart';

import '../theme.dart';

/// The title and the line under it of one step.
class OnboardingHeading extends StatelessWidget {
  const OnboardingHeading({super.key, required this.title, this.subtitle});

  final String title;
  final String? subtitle;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final text = Theme.of(context).textTheme;
    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Semantics(
            header: true,
            child: Text(
              title,
              style: text.headlineSmall?.copyWith(color: colors.textPrimary),
            ),
          ),
          if (subtitle case final subtitle?)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(
                subtitle,
                style: text.bodyLarge?.copyWith(color: colors.textSecondary),
              ),
            ),
        ],
      ),
    );
  }
}

/// One option of a step. [exclusive] options are a single choice (a radio);
/// the others are a multi-selection (a checkbox).
class OnboardingChoice extends StatelessWidget {
  const OnboardingChoice({
    super.key,
    required this.label,
    required this.selected,
    required this.onTap,
    this.icon,
    this.exclusive = false,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;
  final IconData? icon;
  final bool exclusive;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final style = Theme.of(context).textTheme.titleMedium;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Semantics(
        button: true,
        label: label,
        checked: exclusive ? null : selected,
        selected: exclusive ? selected : null,
        inMutuallyExclusiveGroup: exclusive,
        child: Material(
          color: selected ? colors.accentSoft : colors.surface,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(12),
            side: BorderSide(
              color: selected ? colors.accentText : colors.outlineSoft,
              width: selected ? 2 : 1,
            ),
          ),
          child: InkWell(
            borderRadius: BorderRadius.circular(12),
            onTap: onTap,
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: 56),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 16,
                  vertical: 12,
                ),
                child: Row(
                  children: [
                    if (icon case final icon?) ...[
                      Icon(icon, color: colors.accentText),
                      const SizedBox(width: 12),
                    ],
                    Expanded(
                      child: Text(
                        label,
                        style: style?.copyWith(color: colors.textPrimary),
                      ),
                    ),
                    Icon(
                      _markIcon(exclusive: exclusive, selected: selected),
                      color: selected ? colors.accentText : colors.textMuted,
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

IconData _markIcon({required bool exclusive, required bool selected}) =>
    switch ((exclusive, selected)) {
      (true, true) => Icons.radio_button_checked,
      (true, false) => Icons.radio_button_unchecked,
      (false, true) => Icons.check_box,
      (false, false) => Icons.check_box_outline_blank,
    };

/// A notice in plain Spanish: what happened, then what the user can do.
class OnboardingNotice extends StatelessWidget {
  const OnboardingNotice({super.key, required this.message, this.action});

  final String message;
  final String? action;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final text = Theme.of(context).textTheme;
    final action = this.action;
    return Semantics(
      liveRegion: true,
      child: Padding(
        padding: const EdgeInsets.only(bottom: 16),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(Icons.warning_amber_rounded, color: colors.statusBlocked),
            const SizedBox(width: 8),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    message,
                    style: text.bodyMedium?.copyWith(
                      color: colors.textPrimary,
                    ),
                  ),
                  if (action != null)
                    Text(
                      action,
                      style: text.bodySmall?.copyWith(
                        color: colors.textSecondary,
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
