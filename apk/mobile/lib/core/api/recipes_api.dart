import 'dart:convert';

import 'chat_transport.dart';
import 'connection_text.dart';
import 'host_client.dart';
import 'json_read.dart';
import 'read_grant.dart';
import 'recipe_models.dart';

part 'recipes_failure_text.dart';

/// The recipe routes, `/v1/recipes` (contract `api.md`, F5). Reading needs `READ_STATE`;
/// running one needs `APPROVE`, signed the way an approval is signed. This device never
/// saves, edits or deletes a recipe: that is done from the desktop.
class RecipesApi {
  const RecipesApi(this._transport);

  final ChatTransport _transport;

  /// The recipes of one oficio, most used first.
  Future<List<Recipe>> recipes(String roleId) async => recipesFrom(
    jsonEnvelope(
      await _transport.call(
        'GET',
        Uri(path: '/v1/recipes', queryParameters: {'role': roleId}).toString(),
      ),
    ),
  );

  /// Starts the recipe in a conversation that has a project. The answer names the delegation.
  Future<RecipeRun> run(
    String recipeId, {
    required String conversationId,
  }) async => RecipeRun.fromJson(
    jsonEnvelope(
      await _transport.call(
        'POST',
        '/v1/recipes/${Uri.encodeComponent(recipeId)}/run',
        utf8.encode(jsonEncode({'conversation_id': conversationId})),
      ),
    ),
  );
}
