import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/src/rust/api/onboarding/model.dart';

import '../../core/state/hires_controller.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import '../widgets/grouped_card.dart';
import 'hire_actions.dart';
import 'hire_catalog_screen.dart';

/// «Tu equipo» on the Equipo tab: the people the user has contracted, grouped
/// by area. «Contratar» and the actions on a person appear only when the host
/// says this device may contract (`can_hire`).
class TeamHiresSection extends StatelessWidget {
  const TeamHiresSection({super.key, required this.hires});

  final HiresController hires;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: hires,
      builder: (context, _) {
        final c = context.colors;
        final failure = hires.failure;
        return Padding(
          padding: const EdgeInsets.fromLTRB(
            Spacing.lg,
            Spacing.xs,
            Spacing.lg,
            Spacing.sm,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(child: Text('TU EQUIPO', style: context.text.label)),
                  if (hires.canHire)
                    FilledButton.icon(
                      style: FilledButton.styleFrom(
                        backgroundColor: c.accent,
                        foregroundColor: c.onAccent,
                        minimumSize: const Size(48, 48),
                      ),
                      onPressed: hires.busy
                          ? null
                          : () => _openCatalog(context),
                      icon: const Icon(Icons.person_add_alt_1_outlined),
                      label: const Text('Contratar'),
                    ),
                ],
              ),
              if (failure != null)
                Padding(
                  padding: const EdgeInsets.only(top: Spacing.sm),
                  child: HireFailureBanner(
                    failure: failure,
                    onClose: hires.clearFailure,
                    onRetry: hires.busy ? null : () => hires.load(),
                  ),
                ),
              const SizedBox(height: Spacing.sm),
              _body(context),
            ],
          ),
        );
      },
    );
  }

  Widget _body(BuildContext context) {
    if (hires.hires.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: Spacing.md),
        child: Text(
          hires.canHire
              ? 'Todavía no has contratado a nadie. Pulsa «Contratar» para '
                    'elegir por área.'
              : 'Todavía no hay nadie contratado en tu equipo.',
          style: context.text.bodySecondary,
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final group in hires.hiresByArea()) ...[
          Padding(
            padding: const EdgeInsets.only(top: Spacing.sm, bottom: Spacing.xs),
            child: Text(group.label, style: context.text.small),
          ),
          GroupedCard(
            children: [
              for (final hire in group.hires)
                _HireRow(
                  hire: hire,
                  title: hires.roleById(hire.role)?.title ?? '',
                  canHire: hires.canHire,
                  onTap: () => showHireActions(context, hires, hire),
                ),
            ],
          ),
        ],
      ],
    );
  }

  void _openCatalog(BuildContext context) {
    Navigator.of(context).push<void>(
      MaterialPageRoute<void>(builder: (_) => HireCatalogScreen(hires: hires)),
    );
  }
}

/// A failed change or read, said once in words, with the way out. It stays
/// until the user closes it or the next change starts.
class HireFailureBanner extends StatelessWidget {
  const HireFailureBanner({
    super.key,
    required this.failure,
    required this.onClose,
    this.onRetry,
  });

  final ErrorInfo failure;
  final VoidCallback onClose;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final action = failure.action;
    return Semantics(
      liveRegion: true,
      child: Container(
        padding: const EdgeInsets.fromLTRB(
          Spacing.md,
          Spacing.sm,
          Spacing.xs,
          Spacing.sm,
        ),
        decoration: BoxDecoration(
          color: c.accentSoft,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Icon(Icons.error_outline, size: 20, color: c.accentText),
            ),
            const SizedBox(width: Spacing.sm),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(failure.message, style: context.text.body),
                  if (action != null && action.isNotEmpty)
                    Text(action, style: context.text.small),
                  if (onRetry != null)
                    TextButton(
                      style: TextButton.styleFrom(
                        minimumSize: const Size(48, 48),
                        padding: EdgeInsets.zero,
                      ),
                      onPressed: onRetry,
                      child: const Text('Reintentar'),
                    ),
                ],
              ),
            ),
            IconButton(
              tooltip: 'Cerrar aviso',
              constraints: const BoxConstraints.tightFor(width: 48, height: 48),
              onPressed: onClose,
              icon: Icon(Icons.close, color: c.textSecondary),
            ),
          ],
        ),
      ),
    );
  }
}

/// «12 en el equipo · 3 activos»: the contracted people (N) and the live agents
/// (M). The Equipo header and the chat's team card say it the same way.
String teamCountLabel({required int hired, required int active}) =>
    '$hired en el equipo · ${active == 1 ? '1 activo' : '$active activos'}';

class _HireRow extends StatelessWidget {
  const _HireRow({
    required this.hire,
    required this.title,
    required this.canHire,
    required this.onTap,
  });

  final Hire hire;
  final String title;
  final bool canHire;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final label = title.isEmpty ? hire.name : '${hire.name}, $title';
    return Semantics(
      button: canHire,
      label: label,
      excludeSemantics: true,
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: canHire ? onTap : null,
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            padding: const EdgeInsets.symmetric(
              horizontal: Spacing.xs,
              vertical: Spacing.sm,
            ),
            child: Row(
              children: [
                AgentAvatar(
                  name: hire.name,
                  characterId: hire.avatar,
                  size: 40,
                  ringed: true,
                  animated: false,
                  faceFromName: false,
                ),
                const SizedBox(width: Spacing.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        hire.name,
                        style: context.text.body,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      if (title.isNotEmpty)
                        Text(
                          title,
                          style: context.text.small,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                    ],
                  ),
                ),
                if (canHire) Icon(Icons.more_horiz, color: c.accentText),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
