import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';

import '../api/host_client.dart';
import '../api/recipe_models.dart';
import '../api/recipes_api.dart';

/// The conversation a recipe runs in: the one open in the chat when it has a project, else
/// the most recent one that has. Null when none has, and then nothing is started.
String? runConversationId(
  ChatConversation? current,
  Iterable<ChatConversation> conversations,
) {
  bool hasProject(ChatConversation conversation) =>
      conversation.projectId != null && !conversation.archived;
  if (current != null && hasProject(current)) return current.id;
  final withProject = conversations.where(hasProject).toList()
    ..sort((a, b) => b.lastMessageAt.compareTo(a.lastMessageAt));
  return withProject.isEmpty ? null : withProject.first.id;
}

/// The recipes of the oficio whose ficha is open, and one run of them. Nothing is read or
/// run on its own: the screen asks. A busy agent counts down; nothing retries by itself.
class RecipesController extends ChangeNotifier {
  RecipesController({required this.api, required this.conversationId});

  final RecipesApi api;

  /// Where a run goes, read at the moment of the run; null when no conversation has a project.
  final String? Function() conversationId;

  String? _roleId;
  List<Recipe> _recipes = const <Recipe>[];
  Object? _readError;
  bool _loaded = false;
  bool _running = false;
  bool _denied = false;
  Object? _runError;
  RecipeRun? _lastRun;
  int _retryIn = 0;
  Timer? _ticker;
  bool _disposed = false;

  /// Most used first, then the most recently updated.
  List<Recipe> get recipes => _recipes;

  /// The last read's failure; null once a read has answered.
  Object? get readError => _readError;

  /// True after the first answer, even an empty one.
  bool get loaded => _loaded;

  bool get running => _running;

  /// True after a 403 on a run: the device may not run recipes until a list is opened again.
  bool get denied => _denied;

  /// The failure of the last run, or null.
  Object? get runError => _runError;

  /// What the last run started, or null.
  RecipeRun? get lastRun => _lastRun;

  /// Seconds until a busy agent may be asked again; 0 when it may be.
  int get retryIn => _retryIn;

  /// Opens the list of one oficio and reads it. Opening lifts a denial, since the grant may have come since.
  Future<void> open(String roleId) async {
    _roleId = roleId;
    _recipes = const <Recipe>[];
    _loaded = false;
    _denied = false;
    _runError = null;
    _lastRun = null;
    _cancelCountdown();
    await reload();
  }

  /// Reads the list of the oficio that is open, again.
  Future<void> reload() async {
    final roleId = _roleId;
    if (roleId == null) return;
    try {
      _recipes = await api.recipes(roleId);
      _readError = null;
      _loaded = true;
    } catch (error) {
      _readError = error;
    }
    _notify();
  }

  /// Runs one recipe. False when nothing was started; the reason is in [runError].
  Future<bool> run(Recipe recipe) async {
    if (_running || _denied || _retryIn > 0) return false;
    _runError = null;
    final conversation = conversationId();
    if (conversation == null) {
      _runError = const RecipeNoProject();
      _notify();
      return false;
    }
    _running = true;
    _notify();
    try {
      _lastRun = await api.run(recipe.id, conversationId: conversation);
      await reload();
      return true;
    } catch (error) {
      _fail(error);
      return false;
    } finally {
      _running = false;
      _notify();
    }
  }

  void _fail(Object error) {
    _runError = error;
    if (error is! HostError) return;
    if (error.status == 403) _denied = true;
    if (recipeIsBusy(error)) _countdown(error.retryAfter ?? 0);
  }

  void _countdown(int seconds) {
    _cancelCountdown();
    if (seconds <= 0) return;
    _retryIn = seconds;
    _ticker = Timer.periodic(const Duration(seconds: 1), (timer) {
      _retryIn -= 1;
      if (_retryIn <= 0) {
        _retryIn = 0;
        timer.cancel();
        _ticker = null;
      }
      _notify();
    });
  }

  void _cancelCountdown() {
    _ticker?.cancel();
    _ticker = null;
    _retryIn = 0;
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _cancelCountdown();
    super.dispose();
  }
}
