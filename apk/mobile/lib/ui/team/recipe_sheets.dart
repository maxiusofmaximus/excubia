import 'package:flutter/material.dart';

import '../../core/api/recipe_models.dart';
import '../theme.dart';

/// Asks once before a recipe runs. The person is named, so the order cannot go to someone else.
Future<bool> confirmRepeatRecipe(
  BuildContext context,
  String personName,
) async =>
    await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Repetir receta'),
        content: Text('Se lanzará este encargo a $personName.'),
        actions: [
          TextButton(
            style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(minimumSize: const Size(48, 48)),
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('Lanzar encargo'),
          ),
        ],
      ),
    ) ??
    false;

/// The whole text of a recipe, read only. The desktop is where a recipe is edited.
Future<void> showRecipeText(BuildContext context, Recipe recipe) =>
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheetContext) => SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(
            Spacing.lg,
            0,
            Spacing.lg,
            Spacing.lg,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(recipe.title, style: sheetContext.text.title),
              const SizedBox(height: Spacing.sm),
              Text(recipe.body, style: sheetContext.text.body),
              const SizedBox(height: Spacing.lg),
              Align(
                alignment: Alignment.centerRight,
                child: TextButton(
                  style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
                  onPressed: () => Navigator.pop(sheetContext),
                  child: const Text('Cerrar'),
                ),
              ),
            ],
          ),
        ),
      ),
    );
