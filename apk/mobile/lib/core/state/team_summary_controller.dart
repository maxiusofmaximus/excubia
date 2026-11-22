import '../api/team_summary_api.dart';
import '../api/team_summary_models.dart';
import 'active_poll_controller.dart';

/// «Tu equipo»: the contracted people and their state, read again while the screen is
/// showing ([setActive]). A failed read keeps the last members, so a dropped link does not
/// empty the list.
class TeamSummaryController extends ActivePollController {
  TeamSummaryController({
    required this.api,
    super.interval = kPollInterval,
  });

  final TeamSummaryApi api;

  List<TeamMember> _members = const <TeamMember>[];
  Object? _error;
  bool _loaded = false;

  /// The members in the host's order, or the last ones read before a failure.
  List<TeamMember> get members => _members;

  /// The last read's failure; null once a read has answered.
  Object? get error => _error;

  /// True after the first answer, even an empty one.
  bool get loaded => _loaded;

  /// How many members the host says are waiting for the user's approval.
  int get waitingCount => _members
      .where((member) => member.status == TeamStatus.waitingForYou)
      .length;

  @override
  Future<void> read() async {
    try {
      _members = await api.members();
      _error = null;
      _loaded = true;
    } catch (error) {
      _error = error;
    }
  }
}
