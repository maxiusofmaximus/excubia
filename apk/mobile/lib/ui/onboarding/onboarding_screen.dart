import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/state/onboarding_controller.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import 'onboarding_parts.dart';
import 'onboarding_steps.dart';
import 'onboarding_team.dart';

/// The first-run assistant, full screen: one step at a time, with «Saltar» on
/// the steps that allow it. [onDone] closes it, whether the team was contracted
/// or the onboarding was skipped.
class OnboardingScreen extends StatefulWidget {
  const OnboardingScreen({
    super.key,
    required this.controller,
    required this.onDone,
  });

  final OnboardingController controller;
  final VoidCallback onDone;

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  OnboardingController get _c => widget.controller;

  @override
  void initState() {
    super.initState();
    unawaited(_c.loadCatalog());
  }

  Future<void> _skip() async {
    await _c.skip();
    if (_c.step == OnboardingStep.skipped) widget.onDone();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: _c,
      builder: (context, _) => Scaffold(
        backgroundColor: context.colors.bg,
        body: SafeArea(
          child: Column(
            children: [
              Expanded(
                child: SingleChildScrollView(
                  // The bottom room is the «Continuar» button (48) and its margin
                  // (16), so the last row of a long list can scroll clear of it.
                  padding: const EdgeInsets.fromLTRB(16, 24, 16, 64),
                  child: _body(),
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                child: _footer(),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _body() => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      if (_c.failure case final failure?)
        OnboardingNotice(message: failure.message, action: failure.action),
      switch (_c.step) {
        OnboardingStep.welcome => const WelcomeStep(),
        OnboardingStep.areas => AreasStep(controller: _c),
        OnboardingStep.level => LevelStep(controller: _c),
        OnboardingStep.team => TeamStep(controller: _c),
        OnboardingStep.confirmed => _ConfirmedStep(controller: _c),
        OnboardingStep.skipped => const SizedBox.shrink(),
      },
    ],
  );

  Widget _footer() {
    final c = _c;
    final busy = c.busy;
    final primary = switch (c.step) {
      OnboardingStep.welcome ||
      OnboardingStep.level => (label: 'Continuar', onPressed: busy ? null : c.advance),
      OnboardingStep.areas when c.catalog == null => (
        label: 'Reintentar',
        onPressed: busy ? null : () => unawaited(c.loadCatalog()),
      ),
      OnboardingStep.areas => (
        label: 'Continuar',
        onPressed: busy || c.chosenAreas.isEmpty ? null : c.advance,
      ),
      OnboardingStep.team => (
        label: 'Contratar equipo',
        onPressed: busy || !c.canContract ? null : () => unawaited(c.contract()),
      ),
      OnboardingStep.confirmed => (label: 'Continuar', onPressed: widget.onDone),
      OnboardingStep.skipped => (label: 'Continuar', onPressed: null),
    };
    final back = switch (c.step) {
      OnboardingStep.areas || OnboardingStep.level || OnboardingStep.team =>
        c.back,
      _ => null,
    };
    final skip = switch (c.step) {
      OnboardingStep.level => c.skipLevel,
      OnboardingStep.welcome || OnboardingStep.areas || OnboardingStep.team =>
        () => unawaited(_skip()),
      _ => null,
    };
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        FilledButton(
          style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(48)),
          onPressed: primary.onPressed,
          child: Text(primary.label),
        ),
        Row(
          children: [
            if (back != null)
              TextButton(
                onPressed: busy ? null : back,
                child: const Text('Atrás'),
              ),
            const Spacer(),
            if (skip != null)
              TextButton(
                onPressed: busy ? null : skip,
                child: const Text('Saltar'),
              ),
          ],
        ),
      ],
    );
  }
}

/// The last screen after a contract: how many people were prepared, and where.
class _ConfirmedStep extends StatelessWidget {
  const _ConfirmedStep({required this.controller});

  final OnboardingController controller;

  @override
  Widget build(BuildContext context) {
    final count = controller.contracted;
    final people = count == 1 ? 'agente' : 'agentes';
    final areas = _spanishList([
      for (final area in controller.chosenAreas) area.label,
    ]);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Center(
          child: AgentAvatar(name: 'Alfred', characterId: 'alfred', size: 96),
        ),
        const SizedBox(height: 24),
        Text(
          'He preparado $count $people para $areas. Puedes ajustarlos en cualquier momento.',
          style: Theme.of(context).textTheme.bodyLarge?.copyWith(
            color: context.colors.textPrimary,
          ),
        ),
      ],
    );
  }
}

/// «a, b y c», the way the confirmation lists the areas.
String _spanishList(List<String> items) => switch (items.length) {
  0 => '',
  1 => items.first,
  _ => '${items.sublist(0, items.length - 1).join(', ')} y ${items.last}',
};
