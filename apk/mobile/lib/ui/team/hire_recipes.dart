import 'package:flutter/material.dart';

import '../../core/api/recipe_models.dart';
import '../../core/api/recipes_api.dart';
import '../../core/state/recipes_controller.dart';
import '../theme.dart';
import '../widgets/empty_state.dart';
import 'recipe_sheets.dart';
import 'team_panel_parts.dart';

/// «Recetas» in a contracted person's ficha: what this oficio can repeat. One tap repeats a
/// recipe after a short question; its whole text opens read only. Saving and editing are
/// done on the desktop, which is said in the empty state.
class HireRecipes extends StatelessWidget {
  const HireRecipes({
    super.key,
    required this.recipes,
    required this.personName,
    this.onHire,
  });

  final RecipesController recipes;
  final String personName;

  /// Opens the catalogue when the oficio has no hire left; null hides that way out.
  final VoidCallback? onHire;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: recipes,
      builder: (context, _) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('RECETAS', style: context.text.label),
          const SizedBox(height: Spacing.sm),
          _body(context),
        ],
      ),
    );
  }

  Widget _body(BuildContext context) {
    final error = recipes.readError;
    if (!recipes.loaded && error == null) {
      return const TeamLoadingBlock(label: 'Cargando recetas');
    }
    if (recipes.recipes.isEmpty) {
      if (error != null) return _readNotice(error);
      return const EmptyState(
        icon: Icons.menu_book_outlined,
        title: 'Aún no hay recetas.',
        detail: 'Guárdalas desde el escritorio cuando algo salga bien.',
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (error != null) ...[
          _readNotice(error),
          const SizedBox(height: Spacing.sm),
        ],
        if (recipes.runError != null) ...[
          _runNotice(),
          const SizedBox(height: Spacing.sm),
        ],
        for (final recipe in recipes.recipes)
          _RecipeTile(
            recipe: recipe,
            repeatLabel: _repeatLabel,
            onRepeat: _locked ? null : () => _repeat(context, recipe),
            onView: () => showRecipeText(context, recipe),
          ),
      ],
    );
  }

  /// Off while a run is out, after a refusal for lack of APPROVE, and during a busy agent's wait.
  bool get _locked => recipes.running || recipes.denied || recipes.retryIn > 0;

  String get _repeatLabel =>
      recipes.retryIn > 0 ? 'Repetir en ${recipes.retryIn} s' : 'Repetir';

  TeamNoticeBox _readNotice(Object error) => TeamNoticeBox(
    text: recipeReadFailureText(error),
    onRetry: recipeReadWaitingText(error) == null
        ? () => recipes.reload()
        : null,
  );

  Widget _runNotice() {
    final error = recipes.runError!;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TeamNoticeBox(
          text: recipeIsBusy(error)
              ? recipeBusyText(recipes.retryIn)
              : recipeRunFailureText(error),
        ),
        if (recipeNeedsHire(error) && onHire != null)
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton(
              style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
              onPressed: onHire,
              child: const Text('Contratar'),
            ),
          ),
      ],
    );
  }

  Future<void> _repeat(BuildContext context, Recipe recipe) async {
    final confirmed = await confirmRepeatRecipe(context, personName);
    if (!confirmed || !context.mounted) return;
    final started = await recipes.run(recipe);
    if (started && context.mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('Encargo lanzado')));
    }
  }
}

class _RecipeTile extends StatelessWidget {
  const _RecipeTile({
    required this.recipe,
    required this.repeatLabel,
    required this.onRepeat,
    required this.onView,
  });

  final Recipe recipe;
  final String repeatLabel;
  final VoidCallback? onRepeat;
  final VoidCallback onView;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Container(
      margin: const EdgeInsets.only(bottom: Spacing.sm),
      padding: const EdgeInsets.all(Spacing.md),
      decoration: BoxDecoration(
        color: c.raised,
        borderRadius: BorderRadius.circular(16),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(recipe.title, style: context.text.title),
          Text(recipeUsesText(recipe.uses), style: context.text.small),
          const SizedBox(height: Spacing.xs),
          Text(
            recipe.body.trim().split('\n').first,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.bodySecondary,
          ),
          const SizedBox(height: Spacing.sm),
          Wrap(
            spacing: Spacing.sm,
            runSpacing: Spacing.xs,
            children: [
              FilledButton(
                style: FilledButton.styleFrom(
                  backgroundColor: c.accent,
                  foregroundColor: c.onAccent,
                  minimumSize: const Size(48, 48),
                ),
                onPressed: onRepeat,
                child: Text(repeatLabel),
              ),
              TextButton(
                style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
                onPressed: onView,
                child: const Text('Ver receta'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
