import 'chat_transport.dart';
import 'connection_text.dart';
import 'host_client.dart';
import 'json_read.dart';
import 'read_grant.dart';
import 'team_summary_models.dart';

part 'team_summary_failure_text.dart';

/// `GET /v1/team/summary` (contract `api.md`, F1). Reads need `READ_STATE`.
class TeamSummaryApi {
  const TeamSummaryApi(this._transport);

  final ChatTransport _transport;

  Future<List<TeamMember>> members() async => teamMembersFrom(
    jsonEnvelope(await _transport.call('GET', '/v1/team/summary')),
  );
}
