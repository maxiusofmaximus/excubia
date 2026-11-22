import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/recipe_models.dart';
import 'package:excubia/core/state/recipes_controller.dart';
import 'package:excubia/src/rust/api/chat/model.dart';

Map<String, Object?> recipeJson({
  String id = 'rcp_1',
  String title = 'Resumen',
  String body = 'Haz un resumen.',
  int uses = 0,
  String updatedAt = '2026-11-05T10:00:00.000Z',
}) => {
  'id': id,
  'role_id': 'audio-editor',
  'title': title,
  'body': body,
  'uses': uses,
  'created_at': '2026-11-01T10:00:00.000Z',
  'updated_at': updatedAt,
};

ChatConversation conversation({
  required String id,
  String? projectId,
  bool archived = false,
  String lastMessageAt = '2026-11-05T10:00:00.000Z',
}) => ChatConversation(
  id: id,
  title: 'Charla $id',
  projectId: projectId,
  pinned: false,
  archived: archived,
  lastMessageAt: lastMessageAt,
  lastSeq: 0,
  lastReadSeq: 0,
  unread: BigInt.zero,
  activeRuns: 0,
  createdAt: '2026-11-01T10:00:00.000Z',
  updatedAt: lastMessageAt,
);

void main() {
  group('reading a recipe', () {
    test('keeps the fields the host sends', () {
      final recipe = Recipe.fromJson(recipeJson(uses: 3));

      expect(recipe.id, 'rcp_1');
      expect(recipe.roleId, 'audio-editor');
      expect(recipe.title, 'Resumen');
      expect(recipe.body, 'Haz un resumen.');
      expect(recipe.uses, 3);
    });

    test('a missing field is a broken answer, not an empty recipe', () {
      final json = recipeJson()..remove('title');

      expect(() => Recipe.fromJson(json), throwsFormatException);
    });

    test('a uses count that is not a whole number is broken too', () {
      final json = recipeJson()..['uses'] = 1.0;

      expect(() => Recipe.fromJson(json), throwsFormatException);
    });
  });

  group('the order of the list', () {
    test('most used first, then the most recently updated', () {
      final sorted = sortRecipes([
        Recipe.fromJson(
          recipeJson(id: 'a', uses: 1, updatedAt: '2026-11-01T00:00:00.000Z'),
        ),
        Recipe.fromJson(
          recipeJson(id: 'b', uses: 4, updatedAt: '2026-10-01T00:00:00.000Z'),
        ),
        Recipe.fromJson(
          recipeJson(id: 'c', uses: 1, updatedAt: '2026-11-03T00:00:00.000Z'),
        ),
      ]);

      expect(sorted.map((recipe) => recipe.id), ['b', 'c', 'a']);
    });

    test('the envelope is read and sorted the same way', () {
      final list = recipesFrom({
        'ok': true,
        'recipes': [recipeJson(id: 'a'), recipeJson(id: 'b', uses: 2)],
      });

      expect(list.map((recipe) => recipe.id), ['b', 'a']);
    });

    test('a list without recipes is a broken answer', () {
      expect(() => recipesFrom({'ok': true}), throwsFormatException);
    });
  });

  group('how often a recipe was used', () {
    test('says it in plain Spanish', () {
      expect(recipeUsesText(0), 'aún sin usar');
      expect(recipeUsesText(1), 'usada 1 vez');
      expect(recipeUsesText(7), 'usada 7 veces');
    });
  });

  group('the conversation a recipe runs in', () {
    test('the open conversation, when it has a project', () {
      final open = conversation(id: 'open', projectId: 'proj_1');

      expect(
        runConversationId(open, [conversation(id: 'other', projectId: 'p2')]),
        'open',
      );
    });

    test('otherwise the most recent one that has a project', () {
      final list = [
        conversation(id: 'none', lastMessageAt: '2026-11-05T12:00:00.000Z'),
        conversation(
          id: 'old',
          projectId: 'p1',
          lastMessageAt: '2026-11-01T00:00:00.000Z',
        ),
        conversation(
          id: 'new',
          projectId: 'p2',
          lastMessageAt: '2026-11-04T00:00:00.000Z',
        ),
      ];

      expect(runConversationId(null, list), 'new');
    });

    test('an archived conversation never takes the run', () {
      final list = [conversation(id: 'gone', projectId: 'p1', archived: true)];

      expect(runConversationId(null, list), isNull);
    });

    test('null when no conversation has a project', () {
      expect(
        runConversationId(conversation(id: 'x'), [conversation(id: 'y')]),
        isNull,
      );
    });
  });
}
