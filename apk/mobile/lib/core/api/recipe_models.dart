import 'json_read.dart';

// The recipes of an oficio («Recetas», contract `api.md`, F5). The host keeps the text and
// the counts; a device reads them with READ_STATE and runs one with APPROVE.

/// A saved text the person can repeat. Its text is never written to a log.
class Recipe {
  const Recipe({
    required this.id,
    required this.roleId,
    required this.title,
    required this.body,
    required this.uses,
    required this.updatedAt,
  });

  factory Recipe.fromJson(Map<String, Object?> json) => Recipe(
    id: jsonText(json, 'id'),
    roleId: jsonText(json, 'role_id'),
    title: jsonText(json, 'title'),
    body: jsonText(json, 'body'),
    uses: jsonInt(json, 'uses'),
    updatedAt: jsonText(json, 'updated_at'),
  );

  final String id;
  final String roleId;
  final String title;
  final String body;
  final int uses;

  /// ISO 8601 from the host, so the text order is the time order.
  final String updatedAt;
}

/// Most used first, then the most recently updated. The list it is given is left as it came.
List<Recipe> sortRecipes(Iterable<Recipe> recipes) =>
    List<Recipe>.of(recipes)..sort((a, b) {
      final byUses = b.uses.compareTo(a.uses);
      return byUses != 0 ? byUses : b.updatedAt.compareTo(a.updatedAt);
    });

/// The recipes of a `GET /v1/recipes` envelope, in list order.
List<Recipe> recipesFrom(Map<String, Object?> envelope) => sortRecipes([
  for (final item in jsonList(envelope, 'recipes'))
    Recipe.fromJson(jsonObject(item)),
]);

/// «usada 3 veces», «usada 1 vez», or «aún sin usar» for a recipe nobody ran yet.
String recipeUsesText(int uses) => switch (uses) {
  0 => 'aún sin usar',
  1 => 'usada 1 vez',
  _ => 'usada $uses veces',
};

/// What a run started: the delegation Alfred opened, and the state it has at once.
class RecipeRun {
  const RecipeRun({required this.delegationId, required this.status});

  factory RecipeRun.fromJson(Map<String, Object?> json) => RecipeRun(
    delegationId: jsonText(json, 'delegation_id'),
    status: jsonText(json, 'status'),
  );

  final String delegationId;
  final String status;
}
