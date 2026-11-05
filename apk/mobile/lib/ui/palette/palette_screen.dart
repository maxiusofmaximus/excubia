import 'dart:async';

import 'package:flutter/material.dart';

import '../theme.dart';
import 'palette_controller.dart';
import 'palette_scope.dart';

enum PaletteScreenMode { firstRun, settings }

/// The shared «Elige tu paleta» screen. In first-run mode it has the logo, the
/// heading and «Continuar»; in settings mode it is the same grid behind the
/// «Apariencia» app bar, and every tap is saved at once.
class PaletteScreen extends StatelessWidget {
  const PaletteScreen({super.key, required this.mode, this.onContinue});

  final PaletteScreenMode mode;
  final VoidCallback? onContinue;

  bool get _firstRun => mode == PaletteScreenMode.firstRun;

  @override
  Widget build(BuildContext context) {
    final controller = PaletteScope.of(context);
    final system = MediaQuery.platformBrightnessOf(context);
    final active = controller.paletteFor(system);
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: _firstRun ? null : AppBar(title: const Text('Apariencia')),
      body: SafeArea(
        top: _firstRun,
        bottom: false,
        child: Column(
          children: [
            Expanded(
              child: SingleChildScrollView(
                padding: EdgeInsets.fromLTRB(20, _firstRun ? 24 : 16, 20, 24),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (_firstRun) ...[
                      const _PaletteLogo(),
                      const SizedBox(height: Spacing.lg),
                      Semantics(
                        header: true,
                        child: Text(
                          'Elige tu paleta',
                          style: context.text.headline.copyWith(fontSize: 26),
                        ),
                      ),
                      const SizedBox(height: Spacing.xs),
                      Text(
                        'La puedes cambiar luego en Ajustes.',
                        style: context.text.bodySecondary,
                      ),
                      const SizedBox(height: 20),
                    ],
                    _PaletteGrid(
                      active: active,
                      onChoose: (id) => _choose(controller, id),
                    ),
                    const SizedBox(height: Spacing.sm),
                    _FollowSystemRow(
                      value: controller.followSystem,
                      onChanged: (value) => _followSystem(controller, value),
                    ),
                  ],
                ),
              ),
            ),
            if (_firstRun)
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 0, 20, Spacing.lg),
                child: FilledButton(
                  onPressed: () => _continue(controller),
                  child: const Text('Continuar'),
                ),
              ),
          ],
        ),
      ),
    );
  }

  void _choose(PaletteController controller, String id) {
    if (_firstRun) {
      controller.preview(id);
    } else {
      unawaited(controller.choose(id));
    }
  }

  void _followSystem(PaletteController controller, bool value) {
    if (_firstRun) {
      controller.previewFollowSystem(value);
    } else {
      unawaited(controller.setFollowSystem(value));
    }
  }

  Future<void> _continue(PaletteController controller) async {
    await controller.completeFirstRun();
    onContinue?.call();
  }
}

class _PaletteLogo extends StatelessWidget {
  const _PaletteLogo();

  @override
  Widget build(BuildContext context) => Align(
    alignment: Alignment.centerLeft,
    child: ClipRRect(
      borderRadius: BorderRadius.circular(12),
      child: Image.asset(
        'assets/logo.png',
        width: 40,
        height: 40,
        excludeFromSemantics: true,
      ),
    ),
  );
}

class _PaletteGrid extends StatelessWidget {
  const _PaletteGrid({required this.active, required this.onChoose});

  final HerdrPalette active;
  final ValueChanged<String> onChoose;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      const gap = 12.0;
      final width = (constraints.maxWidth - gap) / 2;
      return Wrap(
        spacing: gap,
        runSpacing: gap,
        children: [
          for (final palette in HerdrPalettes.all)
            SizedBox(
              width: width,
              child: _PaletteCard(
                palette: palette,
                selected: palette.id == active.id,
                onTap: () => onChoose(palette.id),
              ),
            ),
        ],
      );
    },
  );
}

class _PaletteCard extends StatelessWidget {
  const _PaletteCard({
    required this.palette,
    required this.selected,
    required this.onTap,
  });

  final HerdrPalette palette;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Semantics(
      key: ValueKey('palette-${palette.id}'),
      button: true,
      selected: selected,
      label: '${palette.name}. ${palette.note}',
      onTap: onTap,
      child: ExcludeSemantics(
        child: Material(
          color: colors.surface,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(16),
            side: BorderSide(
              color: selected ? colors.accentText : colors.outlineSoft,
              width: selected ? 2 : 1,
            ),
          ),
          clipBehavior: Clip.antiAlias,
          child: InkWell(
            onTap: onTap,
            child: Padding(
              padding: const EdgeInsets.all(10),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                mainAxisSize: MainAxisSize.min,
                children: [
                  _PalettePreview(colors: palette.colors),
                  const SizedBox(height: Spacing.sm),
                  Text(
                    palette.name,
                    style: context.text.title.copyWith(fontSize: 14),
                  ),
                  const SizedBox(height: 2),
                  Text(palette.note, style: context.text.small),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _PalettePreview extends StatelessWidget {
  const _PalettePreview({required this.colors});

  final HerdrColors colors;

  @override
  Widget build(BuildContext context) => Container(
    height: 70,
    clipBehavior: Clip.antiAlias,
    decoration: BoxDecoration(
      color: colors.bg,
      borderRadius: BorderRadius.circular(10),
      border: Border.all(color: colors.outlineSoft),
    ),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          flex: 3,
          child: ColoredBox(
            color: colors.surface,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _Bar(color: colors.accent),
                  const SizedBox(height: 5),
                  _Bar(color: colors.textMuted, opacity: .55),
                  const SizedBox(height: 5),
                  _Bar(color: colors.textMuted, opacity: .55),
                ],
              ),
            ),
          ),
        ),
        Expanded(
          flex: 7,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _Bar(color: colors.textPrimary, widthFactor: .7),
                const SizedBox(height: 6),
                _Bar(color: colors.textMuted, widthFactor: .5, opacity: .55),
                const Spacer(),
                Align(
                  alignment: Alignment.centerLeft,
                  child: Container(
                    width: 36,
                    height: 14,
                    decoration: BoxDecoration(
                      color: colors.accent,
                      borderRadius: BorderRadius.circular(5),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    ),
  );
}

class _Bar extends StatelessWidget {
  const _Bar({required this.color, this.widthFactor = 1, this.opacity = 1});

  final Color color;
  final double widthFactor;
  final double opacity;

  @override
  Widget build(BuildContext context) => Align(
    alignment: Alignment.centerLeft,
    child: FractionallySizedBox(
      widthFactor: widthFactor,
      child: Container(
        height: 5,
        decoration: BoxDecoration(
          color: color.withValues(alpha: opacity),
          borderRadius: BorderRadius.circular(3),
        ),
      ),
    ),
  );
}

class _FollowSystemRow extends StatelessWidget {
  const _FollowSystemRow({required this.value, required this.onChanged});

  final bool value;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) => SwitchListTile(
    value: value,
    onChanged: onChanged,
    controlAffinity: ListTileControlAffinity.leading,
    contentPadding: EdgeInsets.zero,
    title: Text('Seguir el tema del sistema', style: context.text.body),
  );
}
