import 'dart:async';

import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/onboarding/model.dart';

import '../../core/state/hires_controller.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import 'hires_section.dart';

/// «Contratar»: the same catalogue as the assistant, by area. A tap contracts
/// that one person; the people already on the team are marked, not offered
/// again. Contracting only registers them; their panel comes with their first
/// assignment.
class HireCatalogScreen extends StatefulWidget {
  const HireCatalogScreen({super.key, required this.hires});

  final HiresController hires;

  @override
  State<HireCatalogScreen> createState() => _HireCatalogScreenState();
}

class _HireCatalogScreenState extends State<HireCatalogScreen> {
  @override
  void initState() {
    super.initState();
    if (widget.hires.catalog == null) unawaited(widget.hires.load());
  }

  Future<void> _hire(CatalogRole role) async {
    final hire = await widget.hires.hire(role);
    if (hire == null || !mounted) return;
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text('Contratado: ${hire.name}')));
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.hires,
      builder: (context, _) {
        final hires = widget.hires;
        return Scaffold(
          backgroundColor: context.colors.bg,
          appBar: AppBar(title: const Text('Contratar')),
          body: _body(context, hires),
        );
      },
    );
  }

  Widget _body(BuildContext context, HiresController hires) {
    final catalog = hires.catalog;
    final failure = hires.failure;
    if (catalog == null) {
      if (failure != null) {
        return Padding(
          padding: const EdgeInsets.all(Spacing.lg),
          child: HireFailureBanner(
            failure: failure,
            onClose: hires.clearFailure,
            onRetry: hires.busy ? null : () => hires.load(),
          ),
        );
      }
      return const Center(child: CircularProgressIndicator());
    }
    return ListView(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.sm,
        Spacing.lg,
        Spacing.xl,
      ),
      children: [
        Text(
          'Contratar solo lo añade a tu equipo. Su panel aparece con su primer '
          'encargo.',
          style: context.text.bodySecondary,
        ),
        if (failure != null)
          Padding(
            padding: const EdgeInsets.only(top: Spacing.md),
            child: HireFailureBanner(
              failure: failure,
              onClose: hires.clearFailure,
            ),
          ),
        for (final area in catalog.areas)
          if (hires.rolesIn(area).isNotEmpty) ...[
            Padding(
              padding: const EdgeInsets.only(
                top: Spacing.lg,
                bottom: Spacing.xs,
              ),
              child: Text(area.label, style: context.text.label),
            ),
            for (final role in hires.rolesIn(area))
              _CatalogRow(
                role: role,
                hired: hires.isHired(role.id),
                enabled: !hires.busy,
                onHire: () => _hire(role),
              ),
          ],
      ],
    );
  }
}

class _CatalogRow extends StatelessWidget {
  const _CatalogRow({
    required this.role,
    required this.hired,
    required this.enabled,
    required this.onHire,
  });

  final CatalogRole role;
  final bool hired;
  final bool enabled;
  final VoidCallback onHire;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Container(
      constraints: const BoxConstraints(minHeight: 56),
      padding: const EdgeInsets.symmetric(vertical: Spacing.sm),
      child: Row(
        children: [
          AgentAvatar(
            name: role.name,
            characterId: role.avatar,
            size: 40,
            ringed: true,
            animated: false,
            faceFromName: false,
          ),
          const SizedBox(width: Spacing.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(role.name, style: context.text.body),
                Text(role.title, style: context.text.small),
              ],
            ),
          ),
          const SizedBox(width: Spacing.sm),
          if (hired)
            Semantics(
              label: '${role.name} ya está contratado',
              excludeSemantics: true,
              child: Row(
                children: [
                  Icon(
                    Icons.check_circle_outline,
                    size: 20,
                    color: c.accentText,
                  ),
                  const SizedBox(width: Spacing.xs),
                  Text('Contratado', style: context.text.small),
                ],
              ),
            )
          else
            Semantics(
              button: true,
              label: 'Contratar a ${role.name}',
              excludeSemantics: true,
              child: FilledButton(
                style: FilledButton.styleFrom(
                  backgroundColor: c.accent,
                  foregroundColor: c.onAccent,
                  minimumSize: const Size(48, 48),
                ),
                onPressed: enabled ? onHire : null,
                child: const Text('Contratar'),
              ),
            ),
        ],
      ),
    );
  }
}
